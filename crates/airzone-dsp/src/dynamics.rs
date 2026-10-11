//! Multiband dynamics (3 bands) and shared gain-computer helpers.

use crate::biquad::Crossover;
use crate::util::{db_to_lin, lin_to_db, time_coeff};

/// Soft-knee downward compressor curve. Returns gain change in dB (<= 0).
#[inline]
pub fn comp_gain_db(level_db: f32, threshold_db: f32, ratio: f32, knee_db: f32) -> f32 {
    let over = level_db - threshold_db;
    let slope = 1.0 / ratio.max(1.0) - 1.0;
    if 2.0 * over < -knee_db {
        0.0
    } else if knee_db > 0.0 && 2.0 * over.abs() <= knee_db {
        slope * (over + knee_db * 0.5).powi(2) / (2.0 * knee_db)
    } else {
        slope * over
    }
}

#[derive(Clone, Copy, Debug)]
pub struct DynBandSettings {
    pub enabled: bool,
    pub threshold_db: f32,
    pub ratio: f32,
    pub knee_db: f32,
    pub attack_ms: f32,
    pub release_ms: f32,
    pub gain_db: f32,
    /// Downward expansion below this level (set very low to disable).
    pub expand_threshold_db: f32,
    pub expand_ratio: f32,
}

impl Default for DynBandSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            threshold_db: -18.0,
            ratio: 2.0,
            knee_db: 6.0,
            attack_ms: 15.0,
            release_ms: 150.0,
            gain_db: 0.0,
            expand_threshold_db: -90.0,
            expand_ratio: 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct BandRuntime {
    env: [f32; 2],
    gr_db: [f32; 2],
}

pub const MB_BANDS: usize = 3;

#[derive(Clone, Debug)]
pub struct MultibandDynamics {
    pub crossovers: [f32; 2],
    pub bands: [DynBandSettings; MB_BANDS],
    /// 0..1: stereo link of detectors.
    pub link: f32,
    pub mid_side: bool,
    pub output_db: f32,
    pub mix: f32,
    /// Per-band gain reduction in dB for metering (positive numbers).
    pub meter_gr: [f32; MB_BANDS],
    xo: [Crossover; 2],
    rt: [BandRuntime; MB_BANDS],
    sr: f32,
    last_xo: [f32; 2],
}

impl Default for MultibandDynamics {
    fn default() -> Self {
        Self {
            crossovers: [120.0, 2500.0],
            bands: [DynBandSettings::default(); MB_BANDS],
            link: 1.0,
            mid_side: false,
            output_db: 0.0,
            mix: 1.0,
            meter_gr: [0.0; MB_BANDS],
            xo: [Crossover::new(3), Crossover::new(3)],
            rt: [BandRuntime::default(); MB_BANDS],
            sr: 44100.0,
            last_xo: [0.0; 2],
        }
    }
}

impl MultibandDynamics {
    pub fn prepare(&mut self, sr: f32) {
        self.sr = sr;
        self.last_xo = [0.0; 2];
        self.reset();
    }
    pub fn reset(&mut self) {
        self.xo.iter_mut().for_each(|x| x.reset());
        self.rt = [BandRuntime::default(); MB_BANDS];
    }
    pub fn latency(&self) -> usize {
        0
    }

    pub fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        if self.last_xo != self.crossovers {
            let f = [self.crossovers[0], self.crossovers[1].max(self.crossovers[0] * 1.2)];
            for x in &mut self.xo {
                x.set_freqs(self.sr, &f);
            }
            self.last_xo = self.crossovers;
        }
        let sr = self.sr;
        let out_gain = db_to_lin(self.output_db);
        let det_rel = time_coeff(8.0, sr);
        let mut att = [0.0f32; MB_BANDS];
        let mut rel = [0.0f32; MB_BANDS];
        let mut mk = [1.0f32; MB_BANDS];
        for b in 0..MB_BANDS {
            att[b] = time_coeff(self.bands[b].attack_ms, sr);
            rel[b] = time_coeff(self.bands[b].release_ms, sr);
            mk[b] = db_to_lin(self.bands[b].gain_db);
        }
        let mut gr_peak = [0.0f32; MB_BANDS];
        for i in 0..l.len() {
            let (dl, dr) = (l[i], r[i]);
            let (mut a, mut b) = (dl, dr);
            if self.mid_side {
                a = 0.5 * (dl + dr);
                b = 0.5 * (dl - dr);
            }
            let bl = self.xo[0].process(a);
            let br = self.xo[1].process(b);
            let mut sum_l = 0.0;
            let mut sum_r = 0.0;
            for k in 0..MB_BANDS {
                let s = self.bands[k];
                if !s.enabled {
                    sum_l += bl[k];
                    sum_r += br[k];
                    continue;
                }
                let rt = &mut self.rt[k];
                let la = bl[k].abs();
                let lb = br[k].abs();
                let mx = la.max(lb);
                let d0 = self.link * mx + (1.0 - self.link) * la;
                let d1 = self.link * mx + (1.0 - self.link) * lb;
                let mut gains = [1.0f32; 2];
                for (c, d) in [d0, d1].iter().enumerate() {
                    let coef = if *d > rt.env[c] { 0.0 } else { det_rel };
                    rt.env[c] = *d + (rt.env[c] - *d) * coef;
                    let lvl = lin_to_db(rt.env[c]);
                    let mut g = comp_gain_db(lvl, s.threshold_db, s.ratio, s.knee_db);
                    if s.expand_ratio > 1.0 && lvl < s.expand_threshold_db {
                        g -= (s.expand_threshold_db - lvl) * (s.expand_ratio - 1.0);
                    }
                    let g = g.max(-60.0);
                    // smoothing of the gain in dB domain (attack on more reduction)
                    let sm = if g < rt.gr_db[c] { att[k] } else { rel[k] };
                    rt.gr_db[c] = g + (rt.gr_db[c] - g) * sm;
                    gains[c] = db_to_lin(rt.gr_db[c]) * mk[k];
                }
                gr_peak[k] = gr_peak[k].max(-rt.gr_db[0]).max(-rt.gr_db[1]);
                sum_l += bl[k] * gains[0];
                sum_r += br[k] * gains[1];
            }
            let (ol, or) = if self.mid_side { (sum_l + sum_r, sum_l - sum_r) } else { (sum_l, sum_r) };
            l[i] = dl + (ol * out_gain - dl) * self.mix;
            r[i] = dr + (or * out_gain - dr) * self.mix;
        }
        self.meter_gr = gr_peak;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comp_curve() {
        assert_eq!(comp_gain_db(-30.0, -20.0, 4.0, 0.0), 0.0);
        let g = comp_gain_db(-10.0, -20.0, 4.0, 0.0);
        assert!((g - (-7.5)).abs() < 1e-4);
    }

    #[test]
    fn multiband_unity_when_neutral() {
        let mut mb = MultibandDynamics::default();
        for b in &mut mb.bands {
            b.threshold_db = 0.0;
            b.ratio = 1.0;
        }
        mb.prepare(48000.0);
        let mut l: Vec<f32> = (0..4096).map(|i| 0.3 * (i as f32 * 0.05).sin()).collect();
        let mut r = l.clone();
        let src = l.clone();
        mb.process(&mut l, &mut r);
        // sum of crossovers is all-pass: energy preserved
        let e0: f32 = src[1024..].iter().map(|x| x * x).sum();
        let e1: f32 = l[1024..].iter().map(|x| x * x).sum();
        assert!((e1 / e0 - 1.0).abs() < 0.05, "{}", e1 / e0);
    }
}

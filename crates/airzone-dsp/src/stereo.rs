//! Stereo Imager (4-band width + stereoize) and Bass Control.

use crate::biquad::{Crossover, Lr4};
use crate::dynamics::comp_gain_db;
use crate::transient::TransientShaper;
use crate::util::{db_to_lin, lin_to_db, time_coeff, DelayLine};

/// Feedback all-pass used for decorrelation.
#[derive(Clone, Debug)]
struct Schroeder {
    line: DelayLine,
    delay: usize,
    g: f32,
}

impl Schroeder {
    fn new(delay: usize, g: f32) -> Self {
        Self { line: DelayLine::new(delay + 1), delay, g }
    }
    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        let d = self.line.tap(self.delay - 1);
        let v = x - self.g * d;
        self.line.push(v);
        d + self.g * v
    }
    fn clear(&mut self) {
        self.line.clear();
    }
}

#[derive(Clone, Debug)]
pub struct Imager {
    pub crossovers: [f32; 3],
    /// Width per band, 1.0 = unchanged, 0 = mono, 2 = double.
    pub widths: [f32; 4],
    pub stereoize: f32,
    pub balance: f32,
    xo: [Crossover; 2],
    deco: Vec<Schroeder>,
    last_xo: [f32; 3],
    sr: f32,
    /// Measured stereo width (0 mono .. 1 wide) for metering.
    pub meter_width: f32,
}

impl Default for Imager {
    fn default() -> Self {
        Self {
            crossovers: [150.0, 1500.0, 6000.0],
            widths: [1.0; 4],
            stereoize: 0.0,
            balance: 0.0,
            xo: [Crossover::new(4), Crossover::new(4)],
            deco: Vec::new(),
            last_xo: [0.0; 3],
            sr: 44100.0,
            meter_width: 0.0,
        }
    }
}

impl Imager {
    pub fn prepare(&mut self, sr: f32) {
        self.sr = sr;
        let scale = sr / 44100.0;
        self.deco = [(113usize, 0.55f32), (193, 0.5), (307, 0.45), (467, 0.4)]
            .iter()
            .map(|(d, g)| Schroeder::new(((*d as f32) * scale) as usize, *g))
            .collect();
        self.last_xo = [0.0; 3];
        self.reset();
    }
    pub fn reset(&mut self) {
        self.xo.iter_mut().for_each(|x| x.reset());
        self.deco.iter_mut().for_each(|d| d.clear());
    }
    pub fn latency(&self) -> usize {
        0
    }
    pub fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        if self.last_xo != self.crossovers {
            let c = self.crossovers;
            let f = [c[0], c[1].max(c[0] * 1.3), c[2].max(c[1] * 1.3)];
            for x in &mut self.xo {
                x.set_freqs(self.sr, &f);
            }
            self.last_xo = self.crossovers;
        }
        let gl = db_to_lin(-6.0 * self.balance.max(0.0));
        let gr = db_to_lin(6.0 * self.balance.min(0.0));
        let mut side_e = 0.0f32;
        let mut mid_e = 0.0f32;
        for i in 0..l.len() {
            let bl = self.xo[0].process(l[i]);
            let br = self.xo[1].process(r[i]);
            let (mut ol, mut or) = (0.0, 0.0);
            for k in 0..4 {
                let m = 0.5 * (bl[k] + br[k]);
                let s = 0.5 * (bl[k] - br[k]) * self.widths[k];
                ol += m + s;
                or += m - s;
            }
            if self.stereoize > 0.0 {
                let mut d = 0.5 * (ol + or);
                for a in &mut self.deco {
                    d = a.process(d);
                }
                d *= self.stereoize * 0.6;
                ol += d;
                or -= d;
            }
            side_e += (0.5 * (ol - or)).powi(2);
            mid_e += (0.5 * (ol + or)).powi(2);
            l[i] = ol * gl;
            r[i] = or * gr;
        }
        let ratio = (side_e / (mid_e + side_e + 1e-12)).sqrt();
        self.meter_width += (ratio - self.meter_width) * 0.2;
    }
}

// ---------------------------------------------------------------- Bass Control

#[derive(Clone, Debug)]
pub struct BassControl {
    pub sub_freq: f32,
    pub bass_freq: f32,
    pub sub_gain_db: f32,
    pub bass_gain_db: f32,
    /// 0..1: how much of the sub band is summed to mono.
    pub sub_mono: f32,
    /// 0..1: how much of the bass band is summed to mono.
    pub bass_mono: f32,
    /// Attack emphasis of the bass band in dB.
    pub punch_db: f32,
    /// 0..1: dynamic control (tightening) of the bass band.
    pub tighten: f32,
    /// Adapt the sub/bass ratio towards `target_ratio_db`.
    pub auto_balance: bool,
    pub target_ratio_db: f32,
    pub mix: f32,
    split_sub: [Lr4; 2],
    split_bass: [Lr4; 2],
    ap: [Lr4; 2],
    ts: TransientShaper,
    env: f32,
    gr: f32,
    e_sub: f32,
    e_bass: f32,
    auto_db: f32,
    last: [f32; 2],
    sr: f32,
    /// Smoothed band levels in dB for visualisation: sub, bass, low-mid(rest).
    pub meter_db: [f32; 3],
    e_rest: f32,
}

impl Default for BassControl {
    fn default() -> Self {
        Self {
            sub_freq: 70.0,
            bass_freq: 250.0,
            sub_gain_db: 0.0,
            bass_gain_db: 0.0,
            sub_mono: 1.0,
            bass_mono: 0.0,
            punch_db: 0.0,
            tighten: 0.0,
            auto_balance: false,
            target_ratio_db: 1.5,
            mix: 1.0,
            split_sub: [Lr4::default(); 2],
            split_bass: [Lr4::default(); 2],
            ap: [Lr4::default(); 2],
            ts: TransientShaper::default(),
            env: 0.0,
            gr: 0.0,
            e_sub: 0.0,
            e_bass: 0.0,
            auto_db: 0.0,
            last: [0.0; 2],
            sr: 44100.0,
            meter_db: [-100.0; 3],
            e_rest: 0.0,
        }
    }
}

impl BassControl {
    pub fn prepare(&mut self, sr: f32) {
        self.sr = sr;
        self.ts = TransientShaper::new(sr);
        self.last = [0.0; 2];
        self.reset();
    }
    pub fn reset(&mut self) {
        self.split_sub.iter_mut().for_each(|s| s.reset());
        self.split_bass.iter_mut().for_each(|s| s.reset());
        self.ap.iter_mut().for_each(|s| s.reset());
        self.ts.reset();
        self.env = 0.0;
        self.gr = 0.0;
        self.e_sub = 0.0;
        self.e_bass = 0.0;
        self.e_rest = 0.0;
        self.auto_db = 0.0;
    }
    pub fn latency(&self) -> usize {
        0
    }

    pub fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        if self.last != [self.sub_freq, self.bass_freq] {
            let bf = self.bass_freq.max(self.sub_freq * 1.5);
            for i in 0..2 {
                self.split_sub[i].set_freq(self.sr, self.sub_freq);
                self.split_bass[i].set_freq(self.sr, bf);
                self.ap[i].set_freq(self.sr, bf);
            }
            self.last = [self.sub_freq, self.bass_freq];
        }
        let rel = time_coeff(90.0, self.sr);
        let avg = time_coeff(400.0, self.sr);
        let thr = -16.0 - 14.0 * self.tighten;
        let ratio = 1.0 + 5.0 * self.tighten;
        let mut sub_gain = db_to_lin(self.sub_gain_db);
        let bass_gain = db_to_lin(self.bass_gain_db);
        for i in 0..l.len() {
            let (dl, dr) = (l[i], r[i]);
            // split: sub | bass | rest, rest-phase-aligned via all-pass on the lower part
            let (sub_l, upper_l) = self.split_sub[0].split(dl);
            let (sub_r, upper_r) = self.split_sub[1].split(dr);
            let (bass_l, rest_l) = self.split_bass[0].split(upper_l);
            let (bass_r, rest_r) = self.split_bass[1].split(upper_r);
            let sub_l = self.ap[0].allpass(sub_l);
            let sub_r = self.ap[1].allpass(sub_r);

            // analysis (mean-square, slow)
            let es = 0.5 * (sub_l * sub_l + sub_r * sub_r);
            let eb = 0.5 * (bass_l * bass_l + bass_r * bass_r);
            let er = 0.5 * (rest_l * rest_l + rest_r * rest_r);
            self.e_sub = es + (self.e_sub - es) * avg;
            self.e_bass = eb + (self.e_bass - eb) * avg;
            self.e_rest = er + (self.e_rest - er) * avg;

            // mono folding
            let (mut sl, mut sr_) = (sub_l, sub_r);
            let sm = 0.5 * (sl + sr_);
            sl += (sm - sl) * self.sub_mono;
            sr_ += (sm - sr_) * self.sub_mono;
            let (mut bl, mut br) = (bass_l, bass_r);
            let bm = 0.5 * (bl + br);
            bl += (bm - bl) * self.bass_mono;
            br += (bm - br) * self.bass_mono;

            // bass-band dynamics
            let det = bass_l.abs().max(bass_r.abs());
            let mut g = self.ts.gain(det, self.punch_db, 0.0);
            if self.tighten > 0.0 {
                self.env = if det > self.env { det } else { det + (self.env - det) * rel };
                let target = comp_gain_db(lin_to_db(self.env), thr, ratio, 6.0);
                self.gr = if target < self.gr { target } else { target + (self.gr - target) * rel };
                g *= db_to_lin(self.gr);
            }
            if i == 0 && self.auto_balance {
                let cur = 10.0 * ((self.e_sub + 1e-12) / (self.e_bass + 1e-12)).log10();
                let err = (self.target_ratio_db - cur).clamp(-6.0, 6.0);
                self.auto_db += (err - self.auto_db) * 0.01;
                sub_gain = db_to_lin(self.sub_gain_db + self.auto_db);
            } else if self.auto_balance {
                sub_gain = db_to_lin(self.sub_gain_db + self.auto_db);
            }
            let ol = sl * sub_gain + bl * g * bass_gain + rest_l;
            let or = sr_ * sub_gain + br * g * bass_gain + rest_r;
            l[i] = dl + (ol - dl) * self.mix;
            r[i] = dr + (or - dr) * self.mix;
        }
        self.meter_db = [
            10.0 * (self.e_sub + 1e-12).log10(),
            10.0 * (self.e_bass + 1e-12).log10(),
            10.0 * (self.e_rest + 1e-12).log10(),
        ];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imager_zero_width_gives_mono() {
        let mut im = Imager::default();
        im.widths = [0.0; 4];
        im.prepare(48000.0);
        let mut l: Vec<f32> = (0..4096).map(|i| (i as f32 * 0.07).sin()).collect();
        let mut r: Vec<f32> = (0..4096).map(|i| (i as f32 * 0.11).cos()).collect();
        im.process(&mut l, &mut r);
        let d: f32 = l.iter().zip(&r).skip(512).map(|(a, b)| (a - b).abs()).fold(0.0, f32::max);
        assert!(d < 1e-4, "{d}");
    }

    #[test]
    fn imager_default_is_transparent() {
        let mut im = Imager::default();
        im.prepare(48000.0);
        let src: Vec<f32> = (0..4096).map(|i| 0.4 * (i as f32 * 0.05).sin()).collect();
        let mut l = src.clone();
        let mut r: Vec<f32> = src.iter().map(|x| -x).collect();
        im.process(&mut l, &mut r);
        let e0: f32 = src.iter().skip(1024).map(|x| x * x).sum();
        let e1: f32 = l.iter().skip(1024).map(|x| x * x).sum();
        assert!((e1 / e0 - 1.0).abs() < 0.05);
    }
}

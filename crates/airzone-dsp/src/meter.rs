//! Metering: ITU-R BS.1770 / EBU R128 loudness, true peak, correlation, spectrum.

use std::sync::Arc;

use realfft::num_complex::Complex32;
use realfft::{RealFftPlanner, RealToComplex};

use crate::biquad::{Biquad, Coeffs};
use crate::util::{log_freq, TAU};

fn k_weight_coeffs(sr: f32) -> (Coeffs, Coeffs) {
    let sr = sr as f64;
    // stage 1: high shelf
    let (f0, g, q) = (1681.974450955533f64, 3.999843853973347f64, 0.7071752369554196f64);
    let k = (std::f64::consts::PI * f0 / sr).tan();
    let vh = 10f64.powf(g / 20.0);
    let vb = vh.powf(0.4996667741545416);
    let a0 = 1.0 + k / q + k * k;
    let shelf = Coeffs {
        b0: ((vh + vb * k / q + k * k) / a0) as f32,
        b1: (2.0 * (k * k - vh) / a0) as f32,
        b2: ((vh - vb * k / q + k * k) / a0) as f32,
        a1: (2.0 * (k * k - 1.0) / a0) as f32,
        a2: ((1.0 - k / q + k * k) / a0) as f32,
    };
    // stage 2: RLB high pass
    let (f0, q) = (38.13547087602444f64, 0.5003270373238773f64);
    let k = (std::f64::consts::PI * f0 / sr).tan();
    let a0 = 1.0 + k / q + k * k;
    let hp = Coeffs {
        b0: (1.0 / a0) as f32,
        b1: (-2.0 / a0) as f32,
        b2: (1.0 / a0) as f32,
        a1: (2.0 * (k * k - 1.0) / a0) as f32,
        a2: ((1.0 - k / q + k * k) / a0) as f32,
    };
    (shelf, hp)
}

const HIST_MIN: f32 = -70.0;
const HIST_BINS: usize = 800; // 0.1 LU steps from -70 to +10

#[derive(Clone)]
struct Histogram {
    counts: Vec<u32>,
    power: Vec<f64>,
}

impl Histogram {
    fn new() -> Self {
        Self { counts: vec![0; HIST_BINS], power: vec![0.0; HIST_BINS] }
    }
    fn clear(&mut self) {
        self.counts.iter_mut().for_each(|c| *c = 0);
        self.power.iter_mut().for_each(|c| *c = 0.0);
    }
    fn add(&mut self, lufs: f32, mean_square: f64) {
        if lufs < HIST_MIN {
            return;
        }
        let i = (((lufs - HIST_MIN) * 10.0) as usize).min(HIST_BINS - 1);
        self.counts[i] += 1;
        self.power[i] += mean_square;
    }
    /// Mean power of entries above `gate_lufs`.
    fn gated_mean(&self, gate_lufs: f32) -> Option<f64> {
        let start = (((gate_lufs - HIST_MIN) * 10.0).max(0.0)) as usize;
        let (mut n, mut p) = (0u64, 0.0f64);
        for i in start..HIST_BINS {
            n += self.counts[i] as u64;
            p += self.power[i];
        }
        if n == 0 {
            None
        } else {
            Some(p / n as f64)
        }
    }
}

fn power_to_lufs(p: f64) -> f32 {
    (-0.691 + 10.0 * p.max(1e-12).log10()) as f32
}

pub struct LoudnessMeter {
    sr: f32,
    shelf: [Biquad; 2],
    hp: [Biquad; 2],
    block_len: usize,
    block_pos: usize,
    block_sum: f64,
    blocks: [f64; 30], // last 3 s of 100 ms mean squares
    block_idx: usize,
    blocks_filled: usize,
    gated: Histogram,
    st_hist: Histogram,
    pub momentary: f32,
    pub short_term: f32,
    pub integrated: f32,
    pub lra: f32,
    pub sample_peak_db: f32,
    pub true_peak_db: f32,
    peak_hist: [[f32; 4]; 2],
    tp_max: f32,
    sp_max: f32,
}

impl LoudnessMeter {
    pub fn new(sr: f32) -> Self {
        let mut m = Self {
            sr,
            shelf: [Biquad::default(); 2],
            hp: [Biquad::default(); 2],
            block_len: (sr * 0.1) as usize,
            block_pos: 0,
            block_sum: 0.0,
            blocks: [0.0; 30],
            block_idx: 0,
            blocks_filled: 0,
            gated: Histogram::new(),
            st_hist: Histogram::new(),
            momentary: -70.0,
            short_term: -70.0,
            integrated: -70.0,
            lra: 0.0,
            sample_peak_db: -120.0,
            true_peak_db: -120.0,
            peak_hist: [[0.0; 4]; 2],
            tp_max: 0.0,
            sp_max: 0.0,
        };
        let (s, h) = k_weight_coeffs(sr);
        for c in 0..2 {
            m.shelf[c].set(s);
            m.hp[c].set(h);
        }
        m
    }

    pub fn reset(&mut self) {
        let sr = self.sr;
        *self = Self::new(sr);
    }

    pub fn process(&mut self, l: &[f32], r: &[f32]) {
        for i in 0..l.len() {
            let x = [l[i], r[i]];
            for c in 0..2 {
                let h = &mut self.peak_hist[c];
                *h = [h[1], h[2], h[3], x[c]];
                self.sp_max = self.sp_max.max(x[c].abs());
                // Hermite inter-sample estimate
                let (s0, s1, s2, s3) = (h[0], h[1], h[2], h[3]);
                let c1 = 0.5 * (s2 - s0);
                let c2 = s0 - 2.5 * s1 + 2.0 * s2 - 0.5 * s3;
                let c3 = 0.5 * (s3 - s0) + 1.5 * (s1 - s2);
                for t in [0.25f32, 0.5, 0.75] {
                    self.tp_max = self.tp_max.max((((c3 * t + c2) * t + c1) * t + s1).abs());
                }
                self.tp_max = self.tp_max.max(x[c].abs());
                let w = self.hp[c].process(self.shelf[c].process(x[c]));
                self.block_sum += (w * w) as f64;
            }
            self.block_pos += 1;
            if self.block_pos >= self.block_len {
                let ms = self.block_sum / self.block_len as f64;
                self.block_pos = 0;
                self.block_sum = 0.0;
                self.finish_block(ms);
            }
        }
        self.sample_peak_db = 20.0 * self.sp_max.max(1e-6).log10();
        self.true_peak_db = 20.0 * self.tp_max.max(1e-6).log10();
    }

    fn finish_block(&mut self, ms: f64) {
        self.blocks[self.block_idx] = ms;
        self.block_idx = (self.block_idx + 1) % 30;
        self.blocks_filled = (self.blocks_filled + 1).min(30);
        let get = |n: usize, s: &Self| -> f64 {
            let mut acc = 0.0;
            for k in 0..n {
                acc += s.blocks[(s.block_idx + 30 - 1 - k) % 30];
            }
            acc / n as f64
        };
        if self.blocks_filled >= 4 {
            let p = get(4, self);
            self.momentary = power_to_lufs(p);
            self.gated.add(self.momentary, p);
            self.update_integrated();
        }
        if self.blocks_filled >= 30 {
            let p = get(30, self);
            self.short_term = power_to_lufs(p);
            self.st_hist.add(self.short_term, p);
            self.update_lra();
        }
    }

    fn update_integrated(&mut self) {
        if let Some(abs) = self.gated.gated_mean(-70.0) {
            let rel_gate = power_to_lufs(abs) - 10.0;
            if let Some(m) = self.gated.gated_mean(rel_gate) {
                self.integrated = power_to_lufs(m);
            }
        }
    }

    fn update_lra(&mut self) {
        let Some(abs) = self.st_hist.gated_mean(-70.0) else { return };
        let gate = power_to_lufs(abs) - 20.0;
        let start = (((gate - HIST_MIN) * 10.0).max(0.0)) as usize;
        let total: u64 = self.st_hist.counts[start..].iter().map(|c| *c as u64).sum();
        if total < 2 {
            return;
        }
        let pick = |q: f64| -> f32 {
            let target = (q * total as f64) as u64;
            let mut acc = 0u64;
            for i in start..HIST_BINS {
                acc += self.st_hist.counts[i] as u64;
                if acc >= target {
                    return HIST_MIN + i as f32 * 0.1;
                }
            }
            HIST_MIN
        };
        self.lra = (pick(0.95) - pick(0.10)).max(0.0);
    }

    pub fn reset_peaks(&mut self) {
        self.tp_max = 0.0;
        self.sp_max = 0.0;
    }
}

/// Stereo correlation and balance (smoothed).
#[derive(Clone, Debug, Default)]
pub struct StereoMeter {
    ll: f32,
    rr: f32,
    lr: f32,
    pub correlation: f32,
}

impl StereoMeter {
    pub fn process(&mut self, l: &[f32], r: &[f32], sr: f32) {
        let c = (-1.0 / (0.3 * sr)).exp();
        for i in 0..l.len() {
            self.ll = l[i] * l[i] + (self.ll - l[i] * l[i]) * c;
            self.rr = r[i] * r[i] + (self.rr - r[i] * r[i]) * c;
            self.lr = l[i] * r[i] + (self.lr - l[i] * r[i]) * c;
        }
        self.correlation = if self.ll * self.rr > 1e-14 { self.lr / (self.ll * self.rr).sqrt() } else { 0.0 };
    }
}

pub const SPEC_POINTS: usize = 160;
const SPEC_FFT: usize = 4096;

pub struct SpectrumAnalyzer {
    fft: Arc<dyn RealToComplex<f32>>,
    window: Vec<f32>,
    ring: Vec<f32>,
    pos: usize,
    count: usize,
    buf: Vec<f32>,
    spec: Vec<Complex32>,
    scratch: Vec<Complex32>,
    sr: f32,
    /// Smoothed log-spaced magnitude in dB (20 Hz - 20 kHz).
    pub out: [f32; SPEC_POINTS],
}

impl SpectrumAnalyzer {
    pub fn new(sr: f32) -> Self {
        let mut planner = RealFftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(SPEC_FFT);
        let scratch = fft.make_scratch_vec();
        let window = (0..SPEC_FFT).map(|i| 0.5 - 0.5 * (TAU * i as f32 / SPEC_FFT as f32).cos()).collect();
        Self {
            fft,
            window,
            ring: vec![0.0; SPEC_FFT],
            pos: 0,
            count: 0,
            buf: vec![0.0; SPEC_FFT],
            spec: vec![Complex32::new(0.0, 0.0); SPEC_FFT / 2 + 1],
            scratch,
            sr,
            out: [-120.0; SPEC_POINTS],
        }
    }

    pub fn process(&mut self, l: &[f32], r: &[f32]) {
        for i in 0..l.len() {
            self.ring[self.pos] = 0.5 * (l[i] + r[i]);
            self.pos = (self.pos + 1) % SPEC_FFT;
            self.count += 1;
            if self.count >= SPEC_FFT / 2 {
                self.count = 0;
                self.compute();
            }
        }
    }

    fn compute(&mut self) {
        for k in 0..SPEC_FFT {
            self.buf[k] = self.ring[(self.pos + k) % SPEC_FFT] * self.window[k];
        }
        let _ = self.fft.process_with_scratch(&mut self.buf, &mut self.spec, &mut self.scratch);
        let bin_hz = self.sr / SPEC_FFT as f32;
        let norm = 4.0 / SPEC_FFT as f32;
        for p in 0..SPEC_POINTS {
            let f_lo = log_freq(p, SPEC_POINTS, 20.0, 20000.0) / 1.03;
            let f_hi = log_freq(p, SPEC_POINTS, 20.0, 20000.0) * 1.03;
            let k0 = ((f_lo / bin_hz).floor() as usize).max(1);
            let k1 = ((f_hi / bin_hz).ceil() as usize).min(SPEC_FFT / 2).max(k0);
            let mut pw = 0.0;
            for k in k0..=k1 {
                pw = f32::max(pw, self.spec[k].norm_sqr());
            }
            let db = 10.0 * (pw * norm * norm + 1e-12).log10();
            let o = &mut self.out[p];
            *o = if db > *o { *o + (db - *o) * 0.6 } else { *o + (db - *o) * 0.15 };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lufs_of_997hz_sine() {
        // BS.1770: a 0 dBFS (peak) 997 Hz stereo sine reads -3.01 LUFS; at -23 dBFS peak => ~-26 LUFS
        let sr = 48000.0;
        let mut m = LoudnessMeter::new(sr);
        let n = (sr * 6.0) as usize;
        let a = 10f32.powf(-23.0 / 20.0);
        let s: Vec<f32> = (0..n).map(|i| a * (TAU * 997.0 * i as f32 / sr).sin()).collect();
        for c in s.chunks(480) {
            m.process(c, c);
        }
        let expect = -23.0 - 0.691 + 10.0 * (1.0f32).log10() - 3.01 + 3.01 + 0.0;
        // mean square of sine = a^2/2 per channel, two channels summed => a^2 => -23 dB -> -23.69 LUFS (K gain ~ +0.7 dB at 1 kHz)
        assert!((m.integrated - expect).abs() < 1.2, "{} vs {}", m.integrated, expect);
        assert!(m.true_peak_db < -22.5 && m.true_peak_db > -23.5);
    }

    #[test]
    fn correlation_values() {
        let mut s = StereoMeter::default();
        let x: Vec<f32> = (0..48000).map(|i| (i as f32 * 0.1).sin()).collect();
        let neg: Vec<f32> = x.iter().map(|v| -v).collect();
        s.process(&x, &x, 48000.0);
        assert!(s.correlation > 0.99);
        let mut s2 = StereoMeter::default();
        s2.process(&x, &neg, 48000.0);
        assert!(s2.correlation < -0.99);
    }

    #[test]
    fn spectrum_peak_at_1k() {
        let sr = 48000.0;
        let mut a = SpectrumAnalyzer::new(sr);
        let x: Vec<f32> = (0..16384).map(|i| 0.5 * (TAU * 1000.0 * i as f32 / sr).sin()).collect();
        a.process(&x, &x);
        let (imax, _) = a.out.iter().enumerate().fold((0, f32::MIN), |(bi, bv), (i, v)| if *v > bv { (i, *v) } else { (bi, bv) });
        let f = log_freq(imax, SPEC_POINTS, 20.0, 20000.0);
        assert!((f - 1000.0).abs() < 100.0, "{f}");
    }
}

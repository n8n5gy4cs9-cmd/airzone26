//! RBJ biquads, Butterworth cascades and Linkwitz-Riley helpers.

use crate::util::{DENORM, PI, TAU};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FilterKind {
    #[default]
    Bell,
    LowShelf,
    HighShelf,
    HighPass,
    LowPass,
    Notch,
    BandPass,
    AllPass,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Coeffs {
    pub b0: f32,
    pub b1: f32,
    pub b2: f32,
    pub a1: f32,
    pub a2: f32,
}

impl Coeffs {
    pub const IDENTITY: Coeffs = Coeffs { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0 };

    pub fn design(kind: FilterKind, sr: f32, freq: f32, q: f32, gain_db: f32) -> Coeffs {
        let freq = freq.clamp(10.0, sr * 0.49);
        let q = q.clamp(0.05, 40.0);
        let w0 = TAU * freq / sr;
        let (sn, cs) = w0.sin_cos();
        let alpha = sn / (2.0 * q);
        let a = 10f32.powf(gain_db / 40.0);
        let (b0, b1, b2, a0, a1, a2);
        match kind {
            FilterKind::Bell => {
                b0 = 1.0 + alpha * a;
                b1 = -2.0 * cs;
                b2 = 1.0 - alpha * a;
                a0 = 1.0 + alpha / a;
                a1 = -2.0 * cs;
                a2 = 1.0 - alpha / a;
            }
            FilterKind::LowShelf => {
                let s = 2.0 * a.sqrt() * alpha;
                b0 = a * ((a + 1.0) - (a - 1.0) * cs + s);
                b1 = 2.0 * a * ((a - 1.0) - (a + 1.0) * cs);
                b2 = a * ((a + 1.0) - (a - 1.0) * cs - s);
                a0 = (a + 1.0) + (a - 1.0) * cs + s;
                a1 = -2.0 * ((a - 1.0) + (a + 1.0) * cs);
                a2 = (a + 1.0) + (a - 1.0) * cs - s;
            }
            FilterKind::HighShelf => {
                let s = 2.0 * a.sqrt() * alpha;
                b0 = a * ((a + 1.0) + (a - 1.0) * cs + s);
                b1 = -2.0 * a * ((a - 1.0) + (a + 1.0) * cs);
                b2 = a * ((a + 1.0) + (a - 1.0) * cs - s);
                a0 = (a + 1.0) - (a - 1.0) * cs + s;
                a1 = 2.0 * ((a - 1.0) - (a + 1.0) * cs);
                a2 = (a + 1.0) - (a - 1.0) * cs - s;
            }
            FilterKind::HighPass => {
                b0 = (1.0 + cs) * 0.5;
                b1 = -(1.0 + cs);
                b2 = (1.0 + cs) * 0.5;
                a0 = 1.0 + alpha;
                a1 = -2.0 * cs;
                a2 = 1.0 - alpha;
            }
            FilterKind::LowPass => {
                b0 = (1.0 - cs) * 0.5;
                b1 = 1.0 - cs;
                b2 = (1.0 - cs) * 0.5;
                a0 = 1.0 + alpha;
                a1 = -2.0 * cs;
                a2 = 1.0 - alpha;
            }
            FilterKind::Notch => {
                b0 = 1.0;
                b1 = -2.0 * cs;
                b2 = 1.0;
                a0 = 1.0 + alpha;
                a1 = -2.0 * cs;
                a2 = 1.0 - alpha;
            }
            FilterKind::BandPass => {
                b0 = alpha;
                b1 = 0.0;
                b2 = -alpha;
                a0 = 1.0 + alpha;
                a1 = -2.0 * cs;
                a2 = 1.0 - alpha;
            }
            FilterKind::AllPass => {
                b0 = 1.0 - alpha;
                b1 = -2.0 * cs;
                b2 = 1.0 + alpha;
                a0 = 1.0 + alpha;
                a1 = -2.0 * cs;
                a2 = 1.0 - alpha;
            }
        }
        let inv = 1.0 / a0;
        Coeffs { b0: b0 * inv, b1: b1 * inv, b2: b2 * inv, a1: a1 * inv, a2: a2 * inv }
    }

    /// Linear magnitude response at `freq`.
    pub fn magnitude(&self, sr: f32, freq: f32) -> f32 {
        let w = TAU * freq / sr;
        let (s1, c1) = w.sin_cos();
        let (s2, c2) = (2.0 * w).sin_cos();
        let nr = self.b0 + self.b1 * c1 + self.b2 * c2;
        let ni = -(self.b1 * s1 + self.b2 * s2);
        let dr = 1.0 + self.a1 * c1 + self.a2 * c2;
        let di = -(self.a1 * s1 + self.a2 * s2);
        ((nr * nr + ni * ni) / (dr * dr + di * di).max(1e-20)).sqrt()
    }
}

/// Transposed direct form II biquad state for one channel.
#[derive(Clone, Copy, Debug, Default)]
pub struct Biquad {
    pub c: Coeffs,
    z1: f32,
    z2: f32,
}

impl Default for Coeffs {
    fn default() -> Self {
        Coeffs::IDENTITY
    }
}

impl Biquad {
    pub fn new(c: Coeffs) -> Self {
        Self { c, z1: 0.0, z2: 0.0 }
    }
    pub fn set(&mut self, c: Coeffs) {
        self.c = c;
    }
    pub fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }
    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let y = self.c.b0 * x + self.z1;
        self.z1 = self.c.b1 * x - self.c.a1 * y + self.z2 + DENORM;
        self.z2 = self.c.b2 * x - self.c.a2 * y - DENORM;
        y
    }
}

/// Butterworth Q values for an even order filter.
pub fn butterworth_qs(order: usize) -> Vec<f32> {
    (1..=order / 2)
        .map(|k| 1.0 / (2.0 * ((2 * k - 1) as f32 * PI / (2 * order) as f32).cos()))
        .collect()
}

/// Cascade of up to four biquads forming a Butterworth high/low pass (12-48 dB/oct).
#[derive(Clone, Debug)]
pub struct ButterCascade {
    pub stages: [Biquad; 4],
    pub n: usize,
}

impl Default for ButterCascade {
    fn default() -> Self {
        Self { stages: [Biquad::default(); 4], n: 1 }
    }
}

impl ButterCascade {
    /// `order` is the filter order (2, 4, 6 or 8).
    pub fn design(&mut self, high_pass: bool, sr: f32, freq: f32, order: usize) {
        let order = order.clamp(2, 8) & !1;
        let qs = butterworth_qs(order);
        self.n = qs.len();
        let kind = if high_pass { FilterKind::HighPass } else { FilterKind::LowPass };
        for (i, q) in qs.iter().enumerate() {
            self.stages[i].set(Coeffs::design(kind, sr, freq, *q, 0.0));
        }
    }
    pub fn reset(&mut self) {
        self.stages.iter_mut().for_each(|s| s.reset());
    }
    #[inline]
    pub fn process(&mut self, mut x: f32) -> f32 {
        for s in self.stages.iter_mut().take(self.n) {
            x = s.process(x);
        }
        x
    }
    pub fn magnitude(&self, sr: f32, f: f32) -> f32 {
        self.stages.iter().take(self.n).map(|s| s.c.magnitude(sr, f)).product()
    }
}

/// Linkwitz-Riley 4th order low/high split (two cascaded Butterworth Q=0.7071 biquads).
#[derive(Clone, Copy, Debug, Default)]
pub struct Lr4 {
    lp: [Biquad; 2],
    hp: [Biquad; 2],
}

impl Lr4 {
    pub fn set_freq(&mut self, sr: f32, freq: f32) {
        let lp = Coeffs::design(FilterKind::LowPass, sr, freq, std::f32::consts::FRAC_1_SQRT_2, 0.0);
        let hp = Coeffs::design(FilterKind::HighPass, sr, freq, std::f32::consts::FRAC_1_SQRT_2, 0.0);
        for b in &mut self.lp {
            b.set(lp);
        }
        for b in &mut self.hp {
            b.set(hp);
        }
    }
    pub fn reset(&mut self) {
        self.lp.iter_mut().chain(self.hp.iter_mut()).for_each(|b| b.reset());
    }
    /// Returns (low, high).
    #[inline]
    pub fn split(&mut self, x: f32) -> (f32, f32) {
        let t = self.lp[0].process(x);
        let l = self.lp[1].process(t);
        let t = self.hp[0].process(x);
        let h = self.hp[1].process(t);
        (l, h)
    }
    /// Phase-matching all-pass (sum of the two branches).
    #[inline]
    pub fn allpass(&mut self, x: f32) -> f32 {
        let (l, h) = self.split(x);
        l + h
    }
}

/// Multi-band crossover (2-4 bands) built from LR4 splits with all-pass phase alignment.
#[derive(Clone, Debug)]
pub struct Crossover {
    pub bands: usize,
    split_mid: Lr4,
    split_lo: Lr4,
    split_hi: Lr4,
    ap_lo: Lr4, // compensates low-side bands for the upper split
    ap_hi: Lr4, // compensates the high side for the lower split
    ap_lo2: Lr4,
    ap_hi2: Lr4,
}

impl Crossover {
    pub fn new(bands: usize) -> Self {
        Self {
            bands: bands.clamp(2, 4),
            split_mid: Lr4::default(),
            split_lo: Lr4::default(),
            split_hi: Lr4::default(),
            ap_lo: Lr4::default(),
            ap_hi: Lr4::default(),
            ap_lo2: Lr4::default(),
            ap_hi2: Lr4::default(),
        }
    }

    /// `freqs` holds `bands - 1` ascending crossover frequencies.
    pub fn set_freqs(&mut self, sr: f32, freqs: &[f32]) {
        match self.bands {
            2 => self.split_mid.set_freq(sr, freqs[0]),
            3 => {
                self.split_lo.set_freq(sr, freqs[0]);
                self.split_mid.set_freq(sr, freqs[1]);
                self.ap_lo.set_freq(sr, freqs[1]);
            }
            _ => {
                self.split_mid.set_freq(sr, freqs[1]);
                self.split_lo.set_freq(sr, freqs[0]);
                self.split_hi.set_freq(sr, freqs[2]);
                self.ap_lo.set_freq(sr, freqs[2]);
                self.ap_lo2.set_freq(sr, freqs[2]);
                self.ap_hi.set_freq(sr, freqs[0]);
                self.ap_hi2.set_freq(sr, freqs[0]);
            }
        }
    }

    pub fn reset(&mut self) {
        for s in [
            &mut self.split_mid,
            &mut self.split_lo,
            &mut self.split_hi,
            &mut self.ap_lo,
            &mut self.ap_hi,
            &mut self.ap_lo2,
            &mut self.ap_hi2,
        ] {
            s.reset();
        }
    }

    /// Split one sample into band outputs; unused entries are zero.
    #[inline]
    pub fn process(&mut self, x: f32) -> [f32; 4] {
        match self.bands {
            2 => {
                let (l, h) = self.split_mid.split(x);
                [l, h, 0.0, 0.0]
            }
            3 => {
                let (l, rest) = self.split_lo.split(x);
                let (m, h) = self.split_mid.split(rest);
                [self.ap_lo.allpass(l), m, h, 0.0]
            }
            _ => {
                let (low_side, high_side) = self.split_mid.split(x);
                let (b0, b1) = self.split_lo.split(low_side);
                let (b2, b3) = self.split_hi.split(high_side);
                [self.ap_lo.allpass(b0), self.ap_lo2.allpass(b1), self.ap_hi.allpass(b2), self.ap_hi2.allpass(b3)]
            }
        }
    }
}

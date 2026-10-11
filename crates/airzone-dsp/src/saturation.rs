//! Anti-aliased (first-order ADAA) waveshapers and the multiband Exciter.

use crate::biquad::Crossover;
use crate::util::db_to_lin;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ShaperKind {
    /// Symmetric tanh: tape-like, odd harmonics.
    #[default]
    Tape,
    /// Biased tanh: even + odd harmonics.
    Tube,
    /// Strongly biased tanh: second-harmonic heavy.
    Warm,
    /// Arctangent: brighter and edgier.
    Retro,
}

#[inline]
fn ln_cosh(x: f32) -> f32 {
    let a = x.abs();
    a + (1.0 + (-2.0 * a).exp()).ln() - std::f32::consts::LN_2
}

#[inline]
fn shape(kind: ShaperKind, x: f32) -> f32 {
    match kind {
        ShaperKind::Tape => x.tanh(),
        ShaperKind::Tube => (x + 0.25).tanh() - 0.25f32.tanh(),
        ShaperKind::Warm => (x + 0.5).tanh() - 0.5f32.tanh(),
        ShaperKind::Retro => (2.0 / std::f32::consts::PI) * (x * std::f32::consts::FRAC_PI_2).atan(),
    }
}

#[inline]
fn antiderivative(kind: ShaperKind, x: f32) -> f32 {
    match kind {
        ShaperKind::Tape => ln_cosh(x),
        ShaperKind::Tube => ln_cosh(x + 0.25) - 0.25f32.tanh() * x,
        ShaperKind::Warm => ln_cosh(x + 0.5) - 0.5f32.tanh() * x,
        ShaperKind::Retro => {
            let k = std::f32::consts::FRAC_PI_2;
            (2.0 / std::f32::consts::PI) * (x * (k * x).atan() - (1.0 + k * k * x * x).ln() / (2.0 * k))
        }
    }
}

/// Stateful ADAA waveshaper (one per channel).
#[derive(Clone, Copy, Debug, Default)]
pub struct Shaper {
    x1: f32,
    pub kind: ShaperKind,
}

impl Shaper {
    pub fn new(kind: ShaperKind) -> Self {
        Self { x1: 0.0, kind }
    }
    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let d = x - self.x1;
        let y = if d.abs() < 1.0e-4 {
            shape(self.kind, 0.5 * (x + self.x1))
        } else {
            (antiderivative(self.kind, x) - antiderivative(self.kind, self.x1)) / d
        };
        self.x1 = x;
        y
    }
}

pub const EXC_BANDS: usize = 4;

#[derive(Clone, Copy, Debug)]
pub struct ExciterBand {
    pub enabled: bool,
    pub drive_db: f32,
    /// 0..1 blend of the saturated signal.
    pub amount: f32,
    pub gain_db: f32,
}

impl Default for ExciterBand {
    fn default() -> Self {
        Self { enabled: true, drive_db: 6.0, amount: 0.0, gain_db: 0.0 }
    }
}

#[derive(Clone, Debug)]
pub struct Exciter {
    pub crossovers: [f32; 3],
    pub kind: ShaperKind,
    pub bands: [ExciterBand; EXC_BANDS],
    pub mix: f32,
    xo: [Crossover; 2],
    sh: [[Shaper; EXC_BANDS]; 2],
    last_xo: [f32; 3],
    last_kind: ShaperKind,
    sr: f32,
}

impl Default for Exciter {
    fn default() -> Self {
        Self {
            crossovers: [150.0, 1200.0, 6000.0],
            kind: ShaperKind::Tape,
            bands: [ExciterBand::default(); EXC_BANDS],
            mix: 1.0,
            xo: [Crossover::new(4), Crossover::new(4)],
            sh: [[Shaper::default(); EXC_BANDS]; 2],
            last_xo: [0.0; 3],
            last_kind: ShaperKind::Tape,
            sr: 44100.0,
        }
    }
}

impl Exciter {
    pub fn prepare(&mut self, sr: f32) {
        self.sr = sr;
        self.last_xo = [0.0; 3];
        self.reset();
    }
    pub fn reset(&mut self) {
        self.xo.iter_mut().for_each(|x| x.reset());
        self.sh = [[Shaper::new(self.kind); EXC_BANDS]; 2];
    }
    pub fn latency(&self) -> usize {
        0
    }

    pub fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        if self.last_xo != self.crossovers {
            let c = self.crossovers;
            let f = [c[0], c[1].max(c[0] * 1.2), c[2].max(c[1] * 1.2)];
            for x in &mut self.xo {
                x.set_freqs(self.sr, &f);
            }
            self.last_xo = self.crossovers;
        }
        if self.last_kind != self.kind {
            self.last_kind = self.kind;
            for ch in &mut self.sh {
                for s in ch.iter_mut() {
                    s.kind = self.kind;
                }
            }
        }
        let mut drive = [1.0f32; EXC_BANDS];
        let mut gain = [1.0f32; EXC_BANDS];
        for k in 0..EXC_BANDS {
            drive[k] = db_to_lin(self.bands[k].drive_db);
            gain[k] = db_to_lin(self.bands[k].gain_db);
        }
        for i in 0..l.len() {
            let dry = [l[i], r[i]];
            let mut out = [0.0f32; 2];
            let bands = [self.xo[0].process(dry[0]), self.xo[1].process(dry[1])];
            for (ch, b) in bands.iter().enumerate() {
                for k in 0..EXC_BANDS {
                    let s = self.bands[k];
                    let x = b[k];
                    if !s.enabled {
                        out[ch] += x;
                        continue;
                    }
                    let wet = self.sh[ch][k].process(x * drive[k]) / drive[k];
                    out[ch] += (x + (wet - x) * s.amount) * gain[k];
                }
            }
            l[i] = dry[0] + (out[0] - dry[0]) * self.mix;
            r[i] = dry[1] + (out[1] - dry[1]) * self.mix;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shaper_is_bounded_and_odd_for_tape() {
        let mut a = Shaper::new(ShaperKind::Tape);
        let mut b = Shaper::new(ShaperKind::Tape);
        for i in 0..1000 {
            let x = 4.0 * (i as f32 * 0.07).sin();
            let ya = a.process(x);
            let yb = b.process(-x);
            assert!(ya.abs() <= 1.0001);
            assert!((ya + yb).abs() < 1e-3);
        }
    }

    #[test]
    fn exciter_off_is_transparent_in_energy() {
        let mut e = Exciter::default();
        e.prepare(48000.0);
        let mut l: Vec<f32> = (0..8192).map(|i| 0.3 * (i as f32 * 0.03).sin()).collect();
        let mut r = l.clone();
        let src = l.clone();
        e.process(&mut l, &mut r);
        let e0: f32 = src[2048..].iter().map(|x| x * x).sum();
        let e1: f32 = l[2048..].iter().map(|x| x * x).sum();
        assert!((e1 / e0 - 1.0).abs() < 0.05);
    }
}

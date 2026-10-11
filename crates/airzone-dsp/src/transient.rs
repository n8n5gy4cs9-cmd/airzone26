//! Transient shaping: Impact (3-band attack/sustain) and Low End Focus.

use crate::biquad::{Crossover, Lr4};
use crate::dynamics::comp_gain_db;
use crate::util::{db_to_lin, lin_to_db, time_coeff};

/// Linked-stereo attack/sustain gain computer.
#[derive(Clone, Copy, Debug)]
pub struct TransientShaper {
    fast: f32,
    slow: f32,
    gain_db: f32,
    fa: f32,
    fr: f32,
    sa: f32,
    sr_: f32,
    smooth: f32,
}

impl Default for TransientShaper {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl TransientShaper {
    pub fn new(sr: f32) -> Self {
        Self {
            fast: 0.0,
            slow: 0.0,
            gain_db: 0.0,
            fa: time_coeff(0.5, sr),
            fr: time_coeff(15.0, sr),
            sa: time_coeff(25.0, sr),
            sr_: time_coeff(130.0, sr),
            smooth: time_coeff(2.0, sr),
        }
    }
    pub fn reset(&mut self) {
        self.fast = 0.0;
        self.slow = 0.0;
        self.gain_db = 0.0;
    }
    /// Returns a linear gain for the given detector level.
    #[inline]
    pub fn gain(&mut self, level: f32, attack_db: f32, sustain_db: f32) -> f32 {
        let cf = if level > self.fast { self.fa } else { self.fr };
        self.fast = level + (self.fast - level) * cf;
        let cs = if level > self.slow { self.sa } else { self.sr_ };
        self.slow = level + (self.slow - level) * cs;
        let diff = lin_to_db(self.fast) - lin_to_db(self.slow);
        let wt = (diff / 8.0).clamp(0.0, 1.0);
        let present = ((lin_to_db(self.slow) + 60.0) / 20.0).clamp(0.0, 1.0);
        let ws = (1.0 - wt) * present;
        let target = wt * attack_db + ws * sustain_db;
        self.gain_db = target + (self.gain_db - target) * self.smooth;
        db_to_lin(self.gain_db)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ImpactBand {
    pub attack_db: f32,
    pub sustain_db: f32,
}

#[derive(Clone, Debug)]
pub struct Impact {
    pub crossovers: [f32; 2],
    pub bands: [ImpactBand; 3],
    pub mix: f32,
    xo: [Crossover; 2],
    ts: [TransientShaper; 3],
    sr: f32,
    last_xo: [f32; 2],
}

impl Default for Impact {
    fn default() -> Self {
        Self {
            crossovers: [200.0, 3000.0],
            bands: [ImpactBand::default(); 3],
            mix: 1.0,
            xo: [Crossover::new(3), Crossover::new(3)],
            ts: [TransientShaper::default(); 3],
            sr: 44100.0,
            last_xo: [0.0; 2],
        }
    }
}

impl Impact {
    pub fn prepare(&mut self, sr: f32) {
        self.sr = sr;
        self.ts = [TransientShaper::new(sr); 3];
        self.last_xo = [0.0; 2];
        self.reset();
    }
    pub fn reset(&mut self) {
        self.xo.iter_mut().for_each(|x| x.reset());
        self.ts.iter_mut().for_each(|t| t.reset());
    }
    pub fn latency(&self) -> usize {
        0
    }
    pub fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        if self.last_xo != self.crossovers {
            let f = [self.crossovers[0], self.crossovers[1].max(self.crossovers[0] * 1.5)];
            for x in &mut self.xo {
                x.set_freqs(self.sr, &f);
            }
            self.last_xo = self.crossovers;
        }
        for i in 0..l.len() {
            let (dl, dr) = (l[i], r[i]);
            let bl = self.xo[0].process(dl);
            let br = self.xo[1].process(dr);
            let (mut ol, mut or) = (0.0, 0.0);
            for k in 0..3 {
                let det = bl[k].abs().max(br[k].abs());
                let g = self.ts[k].gain(det, self.bands[k].attack_db, self.bands[k].sustain_db);
                ol += bl[k] * g;
                or += br[k] * g;
            }
            l[i] = dl + (ol - dl) * self.mix;
            r[i] = dr + (or - dr) * self.mix;
        }
    }
}

#[derive(Clone, Debug)]
pub struct LowEndFocus {
    pub freq: f32,
    /// Attack emphasis of the low band in dB.
    pub punch_db: f32,
    /// Sustain trim of the low band in dB.
    pub sustain_db: f32,
    /// 0..1: dynamic tightening of the low band.
    pub contrast: f32,
    pub mix: f32,
    split: [Lr4; 2],
    ts: TransientShaper,
    env: f32,
    gr: f32,
    last_freq: f32,
    sr: f32,
}

impl Default for LowEndFocus {
    fn default() -> Self {
        Self {
            freq: 120.0,
            punch_db: 0.0,
            sustain_db: 0.0,
            contrast: 0.0,
            mix: 1.0,
            split: [Lr4::default(); 2],
            ts: TransientShaper::default(),
            env: 0.0,
            gr: 0.0,
            last_freq: 0.0,
            sr: 44100.0,
        }
    }
}

impl LowEndFocus {
    pub fn prepare(&mut self, sr: f32) {
        self.sr = sr;
        self.ts = TransientShaper::new(sr);
        self.last_freq = 0.0;
        self.reset();
    }
    pub fn reset(&mut self) {
        self.split.iter_mut().for_each(|s| s.reset());
        self.ts.reset();
        self.env = 0.0;
        self.gr = 0.0;
    }
    pub fn latency(&self) -> usize {
        0
    }
    pub fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        if self.last_freq != self.freq {
            for s in &mut self.split {
                s.set_freq(self.sr, self.freq);
            }
            self.last_freq = self.freq;
        }
        let rel = time_coeff(80.0, self.sr);
        let thr = -18.0 - 12.0 * self.contrast;
        let ratio = 1.0 + 5.0 * self.contrast;
        for i in 0..l.len() {
            let (dl, dr) = (l[i], r[i]);
            let (ll, hl) = self.split[0].split(dl);
            let (lr, hr) = self.split[1].split(dr);
            let det = ll.abs().max(lr.abs());
            let mut g = self.ts.gain(det, self.punch_db, self.sustain_db);
            if self.contrast > 0.0 {
                self.env = if det > self.env { det } else { det + (self.env - det) * rel };
                let target = comp_gain_db(lin_to_db(self.env), thr, ratio, 6.0);
                self.gr = if target < self.gr { target } else { target + (self.gr - target) * rel };
                g *= db_to_lin(self.gr);
            }
            let (pl, pr) = (ll * g, lr * g);
            let (ol, or) = (pl + hl, pr + hr);
            l[i] = dl + (ol - dl) * self.mix;
            r[i] = dr + (or - dr) * self.mix;
        }
    }
}

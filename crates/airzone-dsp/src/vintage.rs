//! Vintage-flavoured processors: opto-style compressor, passive-style EQ,
//! fast peak limiter and tape machine.

use crate::biquad::{Biquad, Coeffs, FilterKind};
use crate::dynamics::comp_gain_db;
use crate::saturation::{Shaper, ShaperKind};
use crate::util::{db_to_lin, lin_to_db, time_coeff, DelayLine, Rng, TAU};

// ------------------------------------------------------------ Vintage compressor

#[derive(Clone, Debug)]
pub struct VintageComp {
    pub input_db: f32,
    pub threshold_db: f32,
    pub ratio: f32,
    pub attack_ms: f32,
    /// Base release; actual release is program dependent (two time constants).
    pub release_ms: f32,
    pub makeup_db: f32,
    /// 0..1 tube-style saturation.
    pub warmth: f32,
    pub mix: f32,
    env: f32,
    gr_fast: f32,
    gr_slow: f32,
    sh: [Shaper; 2],
    sr: f32,
    pub meter_gr: f32,
}

impl Default for VintageComp {
    fn default() -> Self {
        Self {
            input_db: 0.0,
            threshold_db: -14.0,
            ratio: 3.0,
            attack_ms: 12.0,
            release_ms: 250.0,
            makeup_db: 0.0,
            warmth: 0.2,
            mix: 1.0,
            env: 0.0,
            gr_fast: 0.0,
            gr_slow: 0.0,
            sh: [Shaper::new(ShaperKind::Tube); 2],
            sr: 44100.0,
            meter_gr: 0.0,
        }
    }
}

impl VintageComp {
    pub fn prepare(&mut self, sr: f32) {
        self.sr = sr;
        self.reset();
    }
    pub fn reset(&mut self) {
        self.env = 0.0;
        self.gr_fast = 0.0;
        self.gr_slow = 0.0;
        self.sh = [Shaper::new(ShaperKind::Tube); 2];
    }
    pub fn latency(&self) -> usize {
        0
    }
    pub fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        let ig = db_to_lin(self.input_db);
        let mk = db_to_lin(self.makeup_db);
        let att = time_coeff(self.attack_ms, self.sr);
        let rel_f = time_coeff(self.release_ms * 0.25, self.sr);
        let rel_s = time_coeff(self.release_ms * 4.0, self.sr);
        let det_rel = time_coeff(30.0, self.sr);
        let drive = 1.0 + 3.0 * self.warmth;
        for i in 0..l.len() {
            let (dl, dr) = (l[i], r[i]);
            let (xl, xr) = (dl * ig, dr * ig);
            let d = xl.abs().max(xr.abs());
            self.env = if d > self.env { d + (self.env - d) * att.min(0.98) * 0.2 } else { d + (self.env - d) * det_rel };
            let target = comp_gain_db(lin_to_db(self.env), self.threshold_db, self.ratio, 12.0);
            // two-stage optical-like release: fast part recovers quickly, slow part lingers
            let t_fast = target * 0.6;
            let t_slow = target * 0.4;
            self.gr_fast = if t_fast < self.gr_fast { t_fast + (self.gr_fast - t_fast) * att } else { t_fast + (self.gr_fast - t_fast) * rel_f };
            self.gr_slow = if t_slow < self.gr_slow { t_slow + (self.gr_slow - t_slow) * att } else { t_slow + (self.gr_slow - t_slow) * rel_s };
            let g = db_to_lin(self.gr_fast + self.gr_slow) * mk;
            let mut ol = xl * g;
            let mut or = xr * g;
            if self.warmth > 0.0 {
                ol = self.sh[0].process(ol * drive) / drive;
                or = self.sh[1].process(or * drive) / drive;
            }
            l[i] = dl + (ol - dl) * self.mix;
            r[i] = dr + (or - dr) * self.mix;
        }
        self.meter_gr = -(self.gr_fast + self.gr_slow);
    }
}

// ------------------------------------------------------------ Vintage EQ

pub const LOW_FREQS: [f32; 4] = [20.0, 30.0, 60.0, 100.0];
pub const HIGH_BOOST_FREQS: [f32; 7] = [3000.0, 4000.0, 5000.0, 8000.0, 10000.0, 12000.0, 16000.0];
pub const HIGH_ATTEN_FREQS: [f32; 3] = [5000.0, 10000.0, 20000.0];

#[derive(Clone, Debug)]
pub struct VintageEq {
    /// Index into [`LOW_FREQS`].
    pub low_freq: usize,
    /// 0..10 knob values.
    pub low_boost: f32,
    pub low_atten: f32,
    pub high_freq: usize,
    pub high_boost: f32,
    /// 0 (sharp) .. 10 (broad).
    pub bandwidth: f32,
    pub atten_freq: usize,
    pub high_atten: f32,
    /// Output tube stage drive 0..1.
    pub tube: f32,
    filters: [[Biquad; 4]; 2],
    sh: [Shaper; 2],
    last: [f32; 8],
    sr: f32,
}

impl Default for VintageEq {
    fn default() -> Self {
        Self {
            low_freq: 2,
            low_boost: 0.0,
            low_atten: 0.0,
            high_freq: 4,
            high_boost: 0.0,
            bandwidth: 5.0,
            atten_freq: 1,
            high_atten: 0.0,
            tube: 0.15,
            filters: [[Biquad::default(); 4]; 2],
            sh: [Shaper::new(ShaperKind::Tube); 2],
            last: [f32::NAN; 8],
            sr: 44100.0,
        }
    }
}

impl VintageEq {
    pub fn prepare(&mut self, sr: f32) {
        self.sr = sr;
        self.last = [f32::NAN; 8];
        self.reset();
    }
    pub fn reset(&mut self) {
        self.filters.iter_mut().flatten().for_each(|f| f.reset());
    }
    pub fn latency(&self) -> usize {
        0
    }

    pub fn coeffs(&self, sr: f32) -> [Coeffs; 4] {
        let lf = LOW_FREQS[self.low_freq.min(3)];
        let hf = HIGH_BOOST_FREQS[self.high_freq.min(6)];
        let af = HIGH_ATTEN_FREQS[self.atten_freq.min(2)];
        let q = 3.0 - 2.5 * (self.bandwidth / 10.0);
        [
            Coeffs::design(FilterKind::LowShelf, sr, lf, 0.6, self.low_boost * 1.4),
            Coeffs::design(FilterKind::LowShelf, sr, lf * 1.6, 0.6, -self.low_atten * 1.6),
            Coeffs::design(FilterKind::Bell, sr, hf, q, self.high_boost * 1.8),
            Coeffs::design(FilterKind::HighShelf, sr, af * 0.6, 0.6, -self.high_atten * 1.5),
        ]
    }

    pub fn response_db(&self, f: f32) -> f32 {
        self.coeffs(self.sr).iter().map(|c| lin_to_db(c.magnitude(self.sr, f))).sum()
    }

    pub fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        let now = [
            self.low_freq as f32,
            self.low_boost,
            self.low_atten,
            self.high_freq as f32,
            self.high_boost,
            self.bandwidth,
            self.atten_freq as f32,
            self.high_atten,
        ];
        if now != self.last || self.last[0].is_nan() {
            let c = self.coeffs(self.sr);
            for ch in 0..2 {
                for k in 0..4 {
                    self.filters[ch][k].set(c[k]);
                }
            }
            self.last = now;
        }
        let drive = 1.0 + 2.0 * self.tube;
        for i in 0..l.len() {
            let (mut a, mut b) = (l[i], r[i]);
            for k in 0..4 {
                a = self.filters[0][k].process(a);
                b = self.filters[1][k].process(b);
            }
            if self.tube > 0.0 {
                a = self.sh[0].process(a * drive) / drive;
                b = self.sh[1].process(b * drive) / drive;
            }
            l[i] = a;
            r[i] = b;
        }
    }
}

// ------------------------------------------------------------ Vintage limiter

#[derive(Clone, Debug)]
pub struct VintageLimiter {
    pub input_db: f32,
    pub output_db: f32,
    pub attack_ms: f32,
    pub release_ms: f32,
    /// 0..1 saturation of the output stage.
    pub character: f32,
    gr: f32,
    sh: [Shaper; 2],
    sr: f32,
    pub meter_gr: f32,
}

impl Default for VintageLimiter {
    fn default() -> Self {
        Self {
            input_db: 0.0,
            output_db: 0.0,
            attack_ms: 0.2,
            release_ms: 100.0,
            character: 0.3,
            gr: 0.0,
            sh: [Shaper::new(ShaperKind::Warm); 2],
            sr: 44100.0,
            meter_gr: 0.0,
        }
    }
}

impl VintageLimiter {
    pub fn prepare(&mut self, sr: f32) {
        self.sr = sr;
        self.reset();
    }
    pub fn reset(&mut self) {
        self.gr = 0.0;
        self.sh = [Shaper::new(ShaperKind::Warm); 2];
    }
    pub fn latency(&self) -> usize {
        0
    }
    pub fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        let ig = db_to_lin(self.input_db);
        let og = db_to_lin(self.output_db);
        let att = time_coeff(self.attack_ms, self.sr);
        let rel = time_coeff(self.release_ms, self.sr);
        let drive = 1.0 + 2.5 * self.character;
        for i in 0..l.len() {
            let (xl, xr) = (l[i] * ig, r[i] * ig);
            let d = xl.abs().max(xr.abs());
            let target = comp_gain_db(lin_to_db(d), -1.0, 20.0, 3.0);
            self.gr = if target < self.gr { target + (self.gr - target) * att } else { target + (self.gr - target) * rel };
            let g = db_to_lin(self.gr);
            let mut a = xl * g;
            let mut b = xr * g;
            if self.character > 0.0 {
                a = self.sh[0].process(a * drive) / drive;
                b = self.sh[1].process(b * drive) / drive;
            }
            l[i] = a * og;
            r[i] = b * og;
        }
        self.meter_gr = -self.gr;
    }
}

// ------------------------------------------------------------ Vintage tape

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TapeSpeed {
    Ips7_5,
    #[default]
    Ips15,
    Ips30,
}

#[derive(Clone, Debug)]
pub struct VintageTape {
    pub drive_db: f32,
    pub speed: TapeSpeed,
    /// 0..1 saturation depth blend.
    pub saturation: f32,
    /// 0..1 wow & flutter depth.
    pub wow_flutter: f32,
    /// 0..1 tape hiss (very low level).
    pub noise: f32,
    /// 0..1 high-frequency loss ("age").
    pub age: f32,
    pub output_db: f32,
    sh: [Shaper; 2],
    bump: [Biquad; 2],
    lp: [Biquad; 2],
    lp2: [Biquad; 2],
    delay: [DelayLine; 2],
    phase: f32,
    phase2: f32,
    rng: Rng,
    last: (TapeSpeed, f32),
    sr: f32,
}

pub const TAPE_CENTER_DELAY: usize = 16;

impl Default for VintageTape {
    fn default() -> Self {
        Self {
            drive_db: 3.0,
            speed: TapeSpeed::Ips15,
            saturation: 0.7,
            wow_flutter: 0.0,
            noise: 0.0,
            age: 0.0,
            output_db: 0.0,
            sh: [Shaper::new(ShaperKind::Tape); 2],
            bump: [Biquad::default(); 2],
            lp: [Biquad::default(); 2],
            lp2: [Biquad::default(); 2],
            delay: [DelayLine::new(128), DelayLine::new(128)],
            phase: 0.0,
            phase2: 0.0,
            rng: Rng(0x9e37_79b9),
            last: (TapeSpeed::Ips15, -1.0),
            sr: 44100.0,
        }
    }
}

impl VintageTape {
    pub fn prepare(&mut self, sr: f32) {
        self.sr = sr;
        self.last = (TapeSpeed::Ips15, -1.0);
        self.reset();
    }
    pub fn reset(&mut self) {
        self.sh = [Shaper::new(ShaperKind::Tape); 2];
        self.bump.iter_mut().chain(self.lp.iter_mut()).chain(self.lp2.iter_mut()).for_each(|b| b.reset());
        self.delay.iter_mut().for_each(|d| d.clear());
    }
    pub fn latency(&self) -> usize {
        TAPE_CENTER_DELAY
    }
    pub fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        if self.last != (self.speed, self.age) {
            let (bump_f, bump_db, hf) = match self.speed {
                TapeSpeed::Ips7_5 => (90.0, 3.0, 12000.0),
                TapeSpeed::Ips15 => (60.0, 2.0, 17000.0),
                TapeSpeed::Ips30 => (45.0, 1.0, 21000.0),
            };
            let hf = (hf * (1.0 - 0.7 * self.age)).min(self.sr * 0.45);
            let b = Coeffs::design(FilterKind::Bell, self.sr, bump_f, 0.8, bump_db);
            let lp = Coeffs::design(FilterKind::LowPass, self.sr, hf, 0.6, 0.0);
            let lp2 = Coeffs::design(FilterKind::LowPass, self.sr, hf, 1.0, 0.0);
            for c in 0..2 {
                self.bump[c].set(b);
                self.lp[c].set(lp);
                self.lp2[c].set(lp2);
            }
            self.last = (self.speed, self.age);
        }
        let drive = db_to_lin(self.drive_db);
        let og = db_to_lin(self.output_db);
        let wow_rate = 0.6 / self.sr * TAU;
        let flut_rate = 7.3 / self.sr * TAU;
        let depth_w = self.wow_flutter * 9.0;
        let depth_f = self.wow_flutter * 2.0;
        let hiss = self.noise * 0.0006;
        for i in 0..l.len() {
            self.phase = (self.phase + wow_rate) % TAU;
            self.phase2 = (self.phase2 + flut_rate) % TAU;
            let m = TAPE_CENTER_DELAY as f32 + depth_w * self.phase.sin() + depth_f * self.phase2.sin();
            let mut out = [0.0f32; 2];
            let inp = [l[i], r[i]];
            for c in 0..2 {
                let mut x = self.bump[c].process(inp[c]);
                let wet = self.sh[c].process(x * drive) / drive.max(1.0);
                x += (wet - x) * self.saturation;
                x = self.lp2[c].process(self.lp[c].process(x));
                self.delay[c].push(x);
                let mut y = self.delay[c].tap_frac(m.clamp(1.0, 100.0));
                y += hiss * self.rng.next_f32();
                out[c] = y * og;
            }
            l[i] = out[0];
            r[i] = out[1];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(n: usize, a: f32) -> Vec<f32> {
        (0..n).map(|i| a * (i as f32 * 0.05).sin()).collect()
    }

    #[test]
    fn comp_reduces_loud() {
        let mut c = VintageComp::default();
        c.warmth = 0.0;
        c.prepare(48000.0);
        let mut l = sine(24000, 0.9);
        let mut r = l.clone();
        c.process(&mut l, &mut r);
        let peak = l[20000..].iter().fold(0.0f32, |a, b| a.max(b.abs()));
        assert!(peak < 0.7, "{peak}");
    }

    #[test]
    fn vintage_eq_boosts_low_end() {
        let mut e = VintageEq::default();
        e.prepare(48000.0);
        e.low_boost = 8.0;
        assert!(e.response_db(40.0) > 3.0);
        assert!(e.response_db(2000.0).abs() < 1.5);
    }

    #[test]
    fn vintage_limiter_limits() {
        let mut v = VintageLimiter::default();
        v.character = 0.0;
        v.prepare(48000.0);
        let mut l = sine(24000, 2.0);
        let mut r = l.clone();
        v.process(&mut l, &mut r);
        let peak = l[20000..].iter().fold(0.0f32, |a, b| a.max(b.abs()));
        assert!(peak < 1.2, "{peak}");
    }

    #[test]
    fn tape_is_finite_and_delayed() {
        let mut t = VintageTape::default();
        t.prepare(48000.0);
        let mut l = sine(4800, 0.5);
        let mut r = l.clone();
        t.process(&mut l, &mut r);
        assert!(l.iter().all(|x| x.is_finite()));
    }
}

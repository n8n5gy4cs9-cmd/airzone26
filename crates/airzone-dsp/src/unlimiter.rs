//! Unlimiter: restores crest factor / transient peaks on over-compressed material
//! by applying program-dependent upward expansion on peaks that stand out from the
//! short-term level.

use crate::util::{db_to_lin, lin_to_db, time_coeff};

#[derive(Clone, Debug)]
pub struct Unlimiter {
    /// 0..1 overall effect.
    pub amount: f32,
    /// Maximum peak boost in dB.
    pub max_boost_db: f32,
    /// How far a peak must stand above the body (dB) before it is boosted.
    pub threshold_db: f32,
    pub release_ms: f32,
    /// Gently pull the body down to keep loudness roughly constant.
    pub compensate: bool,
    fast: f32,
    slow: f32,
    gain_db: f32,
    body_db: f32,
    sr: f32,
    pub meter_boost_db: f32,
}

impl Default for Unlimiter {
    fn default() -> Self {
        Self {
            amount: 0.5,
            max_boost_db: 6.0,
            threshold_db: 1.0,
            release_ms: 60.0,
            compensate: true,
            fast: 0.0,
            slow: 0.0,
            gain_db: 0.0,
            body_db: 0.0,
            sr: 44100.0,
            meter_boost_db: 0.0,
        }
    }
}

impl Unlimiter {
    pub fn prepare(&mut self, sr: f32) {
        self.sr = sr;
        self.reset();
    }
    pub fn reset(&mut self) {
        self.fast = 0.0;
        self.slow = 0.0;
        self.gain_db = 0.0;
        self.body_db = 0.0;
    }
    pub fn latency(&self) -> usize {
        0
    }
    pub fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        let fa = time_coeff(0.3, self.sr);
        let fr = time_coeff(8.0, self.sr);
        let sa = time_coeff(60.0, self.sr);
        let sr_ = time_coeff(250.0, self.sr);
        let gr = time_coeff(self.release_ms, self.sr);
        let ga = time_coeff(0.5, self.sr);
        let comp_c = time_coeff(800.0, self.sr);
        for i in 0..l.len() {
            let det = l[i].abs().max(r[i].abs());
            self.fast = det + (self.fast - det) * if det > self.fast { fa } else { fr };
            self.slow = det + (self.slow - det) * if det > self.slow { sa } else { sr_ };
            let stand = lin_to_db(self.fast) - lin_to_db(self.slow) - self.threshold_db;
            let weight = (stand / 6.0).clamp(0.0, 1.0);
            let level_ok = ((lin_to_db(self.slow) + 50.0) / 15.0).clamp(0.0, 1.0);
            let target = weight * self.max_boost_db * self.amount * level_ok;
            let c = if target > self.gain_db { ga } else { gr };
            self.gain_db = target + (self.gain_db - target) * c;
            self.body_db = self.gain_db + (self.body_db - self.gain_db) * comp_c;
            let comp = if self.compensate { -0.6 * self.body_db } else { 0.0 };
            let g = db_to_lin(self.gain_db + comp);
            l[i] *= g;
            r[i] *= g;
        }
        self.meter_boost_db = self.gain_db;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boosts_peaks_over_body() {
        let mut u = Unlimiter::default();
        u.amount = 1.0;
        u.compensate = false;
        u.prepare(48000.0);
        let mut l = vec![0.1f32; 48000];
        for k in 0..10 {
            l[20000 + k] = 0.8;
        }
        let mut r = l.clone();
        u.process(&mut l, &mut r);
        let peak = l[20000..20100].iter().fold(0.0f32, |a, b| a.max(b.abs()));
        assert!(peak > 0.9, "{peak}");
        assert!((l[40000] - 0.1).abs() < 0.01);
    }
}

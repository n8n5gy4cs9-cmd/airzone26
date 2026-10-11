//! Clarity: brings out quiet detail and "air" in the upper mids and highs.
//!
//! The band above `freq` is analysed with a stereo-linked level detector. Quiet material in that
//! band is lifted by up to `amount * 8 dB` (upward compression of detail, loud passages are left
//! alone), optional harmonics are generated with an anti-aliased saturator, and a low shelf
//! (`warmth`) can balance the result.

use crate::biquad::{Biquad, ButterCascade, Coeffs, FilterKind};
use crate::saturation::{Shaper, ShaperKind};
use crate::util::{db_to_lin, lin_to_db, time_coeff};

#[derive(Clone, Debug)]
pub struct Clarity {
    /// 0..1: how much quiet detail is lifted.
    pub amount: f32,
    /// Lower edge of the processed band in Hz.
    pub freq: f32,
    /// 0..1: amount of generated harmonics.
    pub harmonics: f32,
    /// -1..1: low-shelf warmth (-3..+3 dB around 200 Hz).
    pub warmth: f32,
    pub mix: f32,
    hp: [ButterCascade; 2],
    shelf: [Biquad; 2],
    sat: [Shaper; 2],
    env: f32,
    gain_db: f32,
    last: (f32, f32),
    sr: f32,
    /// Current detail boost in dB for metering.
    pub meter_boost_db: f32,
}

impl Default for Clarity {
    fn default() -> Self {
        Self {
            amount: 0.4,
            freq: 2000.0,
            harmonics: 0.2,
            warmth: 0.0,
            mix: 1.0,
            hp: [ButterCascade::default(), ButterCascade::default()],
            shelf: [Biquad::default(); 2],
            sat: [Shaper::new(ShaperKind::Tape); 2],
            env: 0.0,
            gain_db: 0.0,
            last: (0.0, 0.0),
            sr: 44100.0,
            meter_boost_db: 0.0,
        }
    }
}

impl Clarity {
    pub fn prepare(&mut self, sr: f32) {
        self.sr = sr;
        self.last = (0.0, 0.0);
        self.reset();
    }

    pub fn reset(&mut self) {
        self.hp.iter_mut().for_each(|h| h.reset());
        self.shelf.iter_mut().for_each(|s| s.reset());
        self.sat = [Shaper::new(ShaperKind::Tape); 2];
        self.env = 0.0;
        self.gain_db = 0.0;
    }

    pub fn latency(&self) -> usize {
        0
    }

    pub fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        if self.last != (self.freq, self.warmth) {
            for c in 0..2 {
                self.hp[c].design(true, self.sr, self.freq.clamp(300.0, 12000.0), 4);
                self.shelf[c].set(Coeffs::design(FilterKind::LowShelf, self.sr, 200.0, 0.7, self.warmth.clamp(-1.0, 1.0) * 3.0));
            }
            self.last = (self.freq, self.warmth);
        }
        let att = time_coeff(3.0, self.sr);
        let rel = time_coeff(80.0, self.sr);
        let smooth = time_coeff(30.0, self.sr);
        let drive = db_to_lin(6.0);
        let use_shelf = self.warmth.abs() > 0.01;
        for i in 0..l.len() {
            let (dl, dr) = (l[i], r[i]);
            let bl = self.hp[0].process(dl);
            let br = self.hp[1].process(dr);
            let det = bl.abs().max(br.abs());
            self.env = if det > self.env { det + (self.env - det) * att } else { det + (self.env - det) * rel };
            let level = lin_to_db(self.env);
            // quiet detail (band level below about -28 dBFS) is lifted, loud parts are not
            let want = self.amount * 8.0 * ((-28.0 - level) / 20.0).clamp(0.0, 1.0);
            self.gain_db = want + (self.gain_db - want) * smooth;
            let g = db_to_lin(self.gain_db) - 1.0;
            let hl = self.sat[0].process(bl * drive) / drive - bl;
            let hr = self.sat[1].process(br * drive) / drive - br;
            let mut ol = dl + (bl * g + hl * self.harmonics * 0.6) * self.mix;
            let mut or = dr + (br * g + hr * self.harmonics * 0.6) * self.mix;
            if use_shelf {
                ol = self.shelf[0].process(ol);
                or = self.shelf[1].process(or);
            }
            l[i] = ol;
            r[i] = or;
        }
        self.meter_boost_db = self.gain_db;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neutral_settings_are_transparent() {
        let mut c = Clarity::default();
        c.amount = 0.0;
        c.harmonics = 0.0;
        c.warmth = 0.0;
        c.prepare(48000.0);
        let src: Vec<f32> = (0..4800).map(|i| 0.3 * (i as f32 * 0.4).sin()).collect();
        let mut l = src.clone();
        let mut r = src.clone();
        c.process(&mut l, &mut r);
        for i in 0..src.len() {
            assert!((l[i] - src[i]).abs() < 1e-5);
        }
    }

    #[test]
    fn lifts_quiet_high_frequency_detail() {
        let mut c = Clarity::default();
        c.amount = 1.0;
        c.harmonics = 0.0;
        c.prepare(48000.0);
        // very quiet 6 kHz tone
        let src: Vec<f32> = (0..48000).map(|i| 0.002 * (std::f32::consts::TAU * 6000.0 * i as f32 / 48000.0).sin()).collect();
        let mut l = src.clone();
        let mut r = src.clone();
        c.process(&mut l, &mut r);
        let e0: f32 = src[24000..].iter().map(|x| x * x).sum();
        let e1: f32 = l[24000..].iter().map(|x| x * x).sum();
        assert!(e1 > e0 * 2.0, "{} vs {}", e1, e0);
    }
}

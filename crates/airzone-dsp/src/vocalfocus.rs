//! Pitch correction of the lead vocal *inside a full mix*.
//!
//! The vocal is estimated with the stem separator (harmonic + centre + voice-band masks), the rest
//! of the mix is obtained by subtraction, the vocal estimate alone is pitch-corrected with the
//! PSOLA engine, and the two are summed again. Separation is approximate, so keep correction
//! amounts moderate; for a clean acapella use the plain Vocal mode instead.

use crate::pitch::PitchCorrector;
use crate::spectral::StemEq;
use crate::stft::{Stft, FFT_SIZE};
use crate::util::DelayLine;

pub struct VocalFocus {
    stft: Stft,
    stem: StemEq,
    pub pitch: PitchCorrector,
    dry: [DelayLine; 2],
    rest: [DelayLine; 2],
    vl: Vec<f32>,
    vr: Vec<f32>,
    sr: f32,
}

impl VocalFocus {
    pub fn new(sr: f32) -> Self {
        let pitch = PitchCorrector::new(sr);
        let lat = pitch.latency();
        let mut stem = StemEq::default();
        stem.solo = Some(2);
        stem.focus = 2.5;
        Self {
            stft: Stft::new(),
            stem,
            pitch,
            dry: [DelayLine::new(FFT_SIZE + 8), DelayLine::new(FFT_SIZE + 8)],
            rest: [DelayLine::new(lat + 8), DelayLine::new(lat + 8)],
            vl: vec![0.0; 4096],
            vr: vec![0.0; 4096],
            sr,
        }
    }

    pub fn prepare(&mut self, sr: f32) {
        let settings = self.pitch.settings;
        *self = Self::new(sr);
        self.pitch.settings = settings;
    }

    pub fn reset(&mut self) {
        let sr = self.sr;
        self.prepare(sr);
    }

    pub fn latency(&self) -> usize {
        FFT_SIZE + self.pitch.latency()
    }

    pub fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        let n = l.len();
        if self.vl.len() < n {
            self.vl.resize(n, 0.0);
            self.vr.resize(n, 0.0);
        }
        self.vl[..n].copy_from_slice(l);
        self.vr[..n].copy_from_slice(r);
        let sr = self.sr;
        {
            let Self { stft, stem, vl, vr, .. } = &mut *self;
            stft.process(&mut vl[..n], &mut vr[..n], |a, b| stem.apply(sr, a, b));
        }
        // rest = delayed dry - vocal estimate (both now FFT_SIZE samples late)
        for i in 0..n {
            self.dry[0].push(l[i]);
            self.dry[1].push(r[i]);
            l[i] = self.dry[0].tap(FFT_SIZE) - self.vl[i];
            r[i] = self.dry[1].tap(FFT_SIZE) - self.vr[i];
        }
        self.pitch.process(&mut self.vl[..n], &mut self.vr[..n]);
        let lat = self.pitch.latency();
        for i in 0..n {
            self.rest[0].push(l[i]);
            self.rest[1].push(r[i]);
            l[i] = self.rest[0].tap(lat) + self.vl[i];
            r[i] = self.rest[1].tap(lat) + self.vr[i];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_finite_and_reports_latency() {
        let mut v = VocalFocus::new(44100.0);
        assert!(v.latency() > FFT_SIZE);
        let mut l: Vec<f32> = (0..44100).map(|i| 0.3 * (i as f32 * 0.04).sin() + 0.1 * (i as f32 * 0.5).sin()).collect();
        let mut r = l.clone();
        for c in 0..(44100 / 441) {
            v.process(&mut l[c * 441..(c + 1) * 441], &mut r[c * 441..(c + 1) * 441]);
        }
        assert!(l.iter().chain(r.iter()).all(|x| x.is_finite() && x.abs() < 3.0));
    }
}

//! Running tonal-balance analysis used by the AirZone 26 Tonal Balance plug-in.

use std::sync::Arc;

use realfft::num_complex::Complex32;
use realfft::{RealFftPlanner, RealToComplex};

use crate::spectral::{SpectrumProfile, MATCH_BANDS};
use crate::util::TAU;

const FFT: usize = 2048;

/// The five broad regions shown by the Tonal Balance display: (name, low Hz, high Hz).
pub const REGIONS: [(&str, f32, f32); 5] = [
    ("Low", 40.0, 200.0),
    ("Low-Mid", 200.0, 800.0),
    ("Mid", 800.0, 3000.0),
    ("High-Mid", 3000.0, 8000.0),
    ("High", 8000.0, 16000.0),
];

fn band_of(f: f32) -> usize {
    let t = (f / 20.0).ln() / 1000f32.ln();
    (t * (MATCH_BANDS - 1) as f32).round().clamp(0.0, (MATCH_BANDS - 1) as f32) as usize
}

/// Average level (dB) of a zero-mean tonal shape inside each region.
pub fn region_levels(shape: &[f32; MATCH_BANDS]) -> [f32; 5] {
    let mut out = [0.0f32; 5];
    for (i, (_, lo, hi)) in REGIONS.iter().enumerate() {
        let (a, b) = (band_of(*lo), band_of(*hi));
        let mut acc = 0.0;
        let mut n = 0.0;
        for k in a..=b.max(a) {
            acc += shape[k];
            n += 1.0;
        }
        out[i] = if n > 0.0 { acc / n } else { 0.0 };
    }
    out
}

pub struct TonalMeter {
    fft: Arc<dyn RealToComplex<f32>>,
    window: Vec<f32>,
    ring: Vec<f32>,
    pos: usize,
    hop: usize,
    buf: Vec<f32>,
    spec: Vec<Complex32>,
    scratch: Vec<Complex32>,
    power: [f32; MATCH_BANDS],
    have: bool,
    sr: f32,
    /// Blend factor per analysis frame (about 5 seconds of memory at 48 kHz).
    pub blend: f32,
}

impl TonalMeter {
    pub fn new(sr: f32) -> Self {
        let mut planner = RealFftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(FFT);
        let scratch = fft.make_scratch_vec();
        Self {
            fft,
            window: (0..FFT).map(|i| 0.5 - 0.5 * (TAU * i as f32 / FFT as f32).cos()).collect(),
            ring: vec![0.0; FFT],
            pos: 0,
            hop: 0,
            buf: vec![0.0; FFT],
            spec: vec![Complex32::new(0.0, 0.0); FFT / 2 + 1],
            scratch,
            power: [0.0; MATCH_BANDS],
            have: false,
            sr,
            blend: 0.01,
        }
    }

    pub fn reset(&mut self) {
        self.power = [0.0; MATCH_BANDS];
        self.have = false;
    }

    pub fn is_ready(&self) -> bool {
        self.have
    }

    pub fn process(&mut self, l: &[f32], r: &[f32]) {
        for i in 0..l.len() {
            self.ring[self.pos] = 0.5 * (l[i] + r[i]);
            self.pos = (self.pos + 1) % FFT;
            self.hop += 1;
            if self.hop >= FFT / 2 {
                self.hop = 0;
                self.frame();
            }
        }
    }

    fn frame(&mut self) {
        for k in 0..FFT {
            self.buf[k] = self.ring[(self.pos + k) % FFT] * self.window[k];
        }
        let _ = self.fft.process_with_scratch(&mut self.buf, &mut self.spec, &mut self.scratch);
        let norm = 1.0 / FFT as f32;
        let scaled: Vec<Complex32> = self.spec.iter().map(|c| *c * norm).collect();
        let mut frame = SpectrumProfile::default();
        frame.accumulate(self.sr, &scaled, &scaled);
        let total: f32 = frame.power.iter().sum();
        if total < 1.0e-14 {
            return; // silence: keep the previous picture
        }
        if !self.have {
            self.power = frame.power;
            self.have = true;
        } else {
            for b in 0..MATCH_BANDS {
                self.power[b] += (frame.power[b] - self.power[b]) * self.blend;
            }
        }
    }

    /// Zero-mean tonal shape in dB per log band.
    pub fn shape_db(&self) -> [f32; MATCH_BANDS] {
        SpectrumProfile { power: self.power, frames: 1 }.shape_db()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noise(n: usize, seed: u32) -> Vec<f32> {
        let mut r = crate::util::Rng(seed);
        (0..n).map(|_| r.next_f32() * 0.3).collect()
    }

    #[test]
    fn white_noise_is_brighter_than_lowpassed_noise() {
        let sr = 48000.0;
        let white = noise(sr as usize * 4, 3);
        let mut lp = crate::biquad::ButterCascade::default();
        lp.design(false, sr, 1000.0, 4);
        let dark: Vec<f32> = white.iter().map(|x| lp.process(*x)).collect();

        let mut a = TonalMeter::new(sr);
        for c in white.chunks(480) {
            a.process(c, c);
        }
        let mut b = TonalMeter::new(sr);
        for c in dark.chunks(480) {
            b.process(c, c);
        }
        assert!(a.is_ready() && b.is_ready());
        let (ra, rb) = (region_levels(&a.shape_db()), region_levels(&b.shape_db()));
        assert!(ra[4] > rb[4] + 10.0, "high region: white {} vs dark {}", ra[4], rb[4]);
        assert!(rb[0] > ra[0], "low region: dark {} vs white {}", rb[0], ra[0]);
    }

    #[test]
    fn silence_keeps_not_ready() {
        let mut m = TonalMeter::new(48000.0);
        let z = vec![0.0f32; 48000];
        m.process(&z, &z);
        assert!(!m.is_ready());
    }
}

//! Streaming stereo STFT engine (2048-point, 75 % overlap, sqrt-Hann windows).
//! Latency is exactly [`FFT_SIZE`] samples.

use std::sync::Arc;

use realfft::num_complex::Complex32;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};

pub const FFT_SIZE: usize = 2048;
pub const HOP: usize = FFT_SIZE / 4;
pub const BINS: usize = FFT_SIZE / 2 + 1;

pub struct Stft {
    r2c: Arc<dyn RealToComplex<f32>>,
    c2r: Arc<dyn ComplexToReal<f32>>,
    window: Vec<f32>,
    in_ring: [Vec<f32>; 2],
    out_ring: [Vec<f32>; 2],
    pos: usize,
    hop_count: usize,
    time_buf: Vec<f32>,
    spec: [Vec<Complex32>; 2],
    scratch_f: Vec<Complex32>,
    scratch_i: Vec<Complex32>,
}

impl Stft {
    pub fn new() -> Self {
        let mut planner = RealFftPlanner::<f32>::new();
        let r2c = planner.plan_fft_forward(FFT_SIZE);
        let c2r = planner.plan_fft_inverse(FFT_SIZE);
        let window: Vec<f32> = (0..FFT_SIZE)
            .map(|i| {
                let h = 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / FFT_SIZE as f32).cos();
                h.sqrt()
            })
            .collect();
        let scratch_f = r2c.make_scratch_vec();
        let scratch_i = c2r.make_scratch_vec();
        Self {
            r2c,
            c2r,
            window,
            in_ring: [vec![0.0; FFT_SIZE], vec![0.0; FFT_SIZE]],
            out_ring: [vec![0.0; FFT_SIZE], vec![0.0; FFT_SIZE]],
            pos: 0,
            hop_count: 0,
            time_buf: vec![0.0; FFT_SIZE],
            spec: [vec![Complex32::new(0.0, 0.0); BINS], vec![Complex32::new(0.0, 0.0); BINS]],
            scratch_f,
            scratch_i,
        }
    }

    pub fn reset(&mut self) {
        for c in 0..2 {
            self.in_ring[c].iter_mut().for_each(|x| *x = 0.0);
            self.out_ring[c].iter_mut().for_each(|x| *x = 0.0);
        }
        self.pos = 0;
        self.hop_count = 0;
    }

    pub const fn latency() -> usize {
        FFT_SIZE
    }

    /// Process audio; `f` is called once per frame with the L and R spectra and
    /// may modify them in place.
    pub fn process<F>(&mut self, l: &mut [f32], r: &mut [f32], mut f: F)
    where
        F: FnMut(&mut [Complex32], &mut [Complex32]),
    {
        let norm = 1.0 / (2.0 * FFT_SIZE as f32);
        for i in 0..l.len() {
            let w = self.pos;
            // emit the completed sample for time t-N, then reuse the slot
            let (ol, or) = (self.out_ring[0][w], self.out_ring[1][w]);
            self.out_ring[0][w] = 0.0;
            self.out_ring[1][w] = 0.0;
            self.in_ring[0][w] = l[i];
            self.in_ring[1][w] = r[i];
            l[i] = ol;
            r[i] = or;
            self.pos = (w + 1) % FFT_SIZE;
            self.hop_count += 1;
            if self.hop_count == HOP {
                self.hop_count = 0;
                let start = self.pos; // oldest sample
                for c in 0..2 {
                    for k in 0..FFT_SIZE {
                        self.time_buf[k] = self.in_ring[c][(start + k) % FFT_SIZE] * self.window[k];
                    }
                    let _ = self.r2c.process_with_scratch(&mut self.time_buf, &mut self.spec[c], &mut self.scratch_f);
                }
                let (a, b) = self.spec.split_at_mut(1);
                f(&mut a[0], &mut b[0]);
                for c in 0..2 {
                    // DC and Nyquist bins must be real for the inverse transform
                    self.spec[c][0].im = 0.0;
                    self.spec[c][BINS - 1].im = 0.0;
                    let _ = self.c2r.process_with_scratch(&mut self.spec[c], &mut self.time_buf, &mut self.scratch_i);
                    for k in 0..FFT_SIZE {
                        self.out_ring[c][(start + k) % FFT_SIZE] += self.time_buf[k] * self.window[k] * norm;
                    }
                }
            }
        }
    }
}

impl Default for Stft {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for Stft {
    fn clone(&self) -> Self {
        Self::new()
    }
}

/// Frequency of an FFT bin.
#[inline]
pub fn bin_freq(k: usize, sr: f32) -> f32 {
    k as f32 * sr / FFT_SIZE as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stft_is_a_delayed_identity() {
        let mut s = Stft::new();
        let n = 16384;
        let src: Vec<f32> = (0..n).map(|i| (i as f32 * 0.031).sin() * 0.5 + (i as f32 * 0.2).sin() * 0.2).collect();
        let mut l = src.clone();
        let mut r = src.clone();
        for c in 0..(n / 256) {
            s.process(&mut l[c * 256..(c + 1) * 256], &mut r[c * 256..(c + 1) * 256], |_, _| {});
        }
        for i in (FFT_SIZE + 64)..n {
            assert!((l[i] - src[i - FFT_SIZE]).abs() < 1e-3, "i={i} {} {}", l[i], src[i - FFT_SIZE]);
        }
    }
}

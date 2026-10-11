//! Time-based effects: stereo delay and a Freeverb-style reverb.

use crate::biquad::{Biquad, Coeffs, FilterKind};
use crate::util::{db_to_lin, DelayLine};

#[derive(Clone, Debug)]
pub struct StereoDelay {
    pub time_l_ms: f32,
    pub time_r_ms: f32,
    pub feedback: f32,
    pub ping_pong: bool,
    pub lowcut_hz: f32,
    pub highcut_hz: f32,
    pub mix: f32,
    /// Output only the echoes (no dry signal) so they can be auditioned on their own.
    pub wet_only: bool,
    lines: [DelayLine; 2],
    hp: [Biquad; 2],
    lp: [Biquad; 2],
    last: (f32, f32),
    sr: f32,
}

impl Default for StereoDelay {
    fn default() -> Self {
        Self {
            time_l_ms: 250.0,
            time_r_ms: 375.0,
            feedback: 0.3,
            ping_pong: false,
            lowcut_hz: 200.0,
            highcut_hz: 6000.0,
            mix: 0.0,
            wet_only: false,
            lines: [DelayLine::new(4), DelayLine::new(4)],
            hp: [Biquad::default(); 2],
            lp: [Biquad::default(); 2],
            last: (0.0, 0.0),
            sr: 44100.0,
        }
    }
}

impl StereoDelay {
    pub fn prepare(&mut self, sr: f32) {
        self.sr = sr;
        let max = (sr * 2.1) as usize;
        self.lines = [DelayLine::new(max), DelayLine::new(max)];
        self.last = (0.0, 0.0);
        self.reset();
    }
    pub fn reset(&mut self) {
        self.lines.iter_mut().for_each(|l| l.clear());
        self.hp.iter_mut().chain(self.lp.iter_mut()).for_each(|b| b.reset());
    }
    pub fn latency(&self) -> usize {
        0
    }
    pub fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        if self.last != (self.lowcut_hz, self.highcut_hz) {
            let h = Coeffs::design(FilterKind::HighPass, self.sr, self.lowcut_hz, 0.707, 0.0);
            let lo = Coeffs::design(FilterKind::LowPass, self.sr, self.highcut_hz, 0.707, 0.0);
            for c in 0..2 {
                self.hp[c].set(h);
                self.lp[c].set(lo);
            }
            self.last = (self.lowcut_hz, self.highcut_hz);
        }
        let dl = (self.time_l_ms * 0.001 * self.sr).clamp(1.0, self.lines[0].capacity() as f32 - 2.0);
        let dr = (self.time_r_ms * 0.001 * self.sr).clamp(1.0, self.lines[1].capacity() as f32 - 2.0);
        let fb = self.feedback.clamp(0.0, 0.95);
        for i in 0..l.len() {
            let wl = self.lines[0].tap_frac(dl);
            let wr = self.lines[1].tap_frac(dr);
            let (in_l, in_r) = if self.ping_pong { (l[i] + fb * wr, fb * wl) } else { (l[i] + fb * wl, r[i] + fb * wr) };
            let fl = self.lp[0].process(self.hp[0].process(in_l));
            let fr = self.lp[1].process(self.hp[1].process(in_r));
            self.lines[0].push(fl);
            self.lines[1].push(fr);
            if self.wet_only {
                // solo the echoes: dry signal removed, level independent of a tiny Mix setting
                let g = self.mix.max(0.25);
                l[i] = wl * g;
                r[i] = wr * g;
            } else {
                l[i] += wl * self.mix;
                r[i] += wr * self.mix;
            }
        }
    }
}

// ------------------------------------------------------------ Reverb

const COMB_TUNING: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const ALLPASS_TUNING: [usize; 4] = [556, 441, 341, 225];
const STEREO_SPREAD: usize = 23;

#[derive(Clone, Debug)]
struct Comb {
    line: DelayLine,
    len: usize,
    store: f32,
}
impl Comb {
    fn new(len: usize) -> Self {
        Self { line: DelayLine::new(len + 1), len, store: 0.0 }
    }
    #[inline]
    fn process(&mut self, x: f32, feedback: f32, damp: f32) -> f32 {
        let out = self.line.tap(self.len - 1);
        self.store = out * (1.0 - damp) + self.store * damp;
        self.line.push(x + self.store * feedback);
        out
    }
}

#[derive(Clone, Debug)]
struct Allpass {
    line: DelayLine,
    len: usize,
}
impl Allpass {
    fn new(len: usize) -> Self {
        Self { line: DelayLine::new(len + 1), len }
    }
    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        let b = self.line.tap(self.len - 1);
        let out = -x + b;
        self.line.push(x + b * 0.5);
        out
    }
}

#[derive(Clone, Debug)]
pub struct Reverb {
    /// 0..1
    pub room_size: f32,
    pub damping: f32,
    pub width: f32,
    pub predelay_ms: f32,
    pub mix: f32,
    pub lowcut_hz: f32,
    /// Solo the reverb: output only the wet signal (no dry) so the tail can be heard on its own.
    pub wet_only: bool,
    combs: [Vec<Comb>; 2],
    allpasses: [Vec<Allpass>; 2],
    pre: [DelayLine; 2],
    hp: [Biquad; 2],
    last_lc: f32,
    sr: f32,
}

impl Default for Reverb {
    fn default() -> Self {
        Self {
            room_size: 0.5,
            damping: 0.5,
            width: 1.0,
            predelay_ms: 10.0,
            mix: 0.0,
            lowcut_hz: 150.0,
            wet_only: false,
            combs: [Vec::new(), Vec::new()],
            allpasses: [Vec::new(), Vec::new()],
            pre: [DelayLine::new(4), DelayLine::new(4)],
            hp: [Biquad::default(); 2],
            last_lc: 0.0,
            sr: 44100.0,
        }
    }
}

impl Reverb {
    pub fn prepare(&mut self, sr: f32) {
        self.sr = sr;
        let s = sr / 44100.0;
        for c in 0..2 {
            let spread = if c == 1 { STEREO_SPREAD } else { 0 };
            self.combs[c] = COMB_TUNING.iter().map(|t| Comb::new((((*t + spread) as f32) * s) as usize)).collect();
            self.allpasses[c] = ALLPASS_TUNING.iter().map(|t| Allpass::new((((*t + spread) as f32) * s) as usize)).collect();
        }
        let pd = (sr * 0.25) as usize;
        self.pre = [DelayLine::new(pd), DelayLine::new(pd)];
        self.last_lc = 0.0;
    }
    pub fn reset(&mut self) {
        let sr = self.sr;
        self.prepare(sr);
    }
    pub fn latency(&self) -> usize {
        0
    }
    pub fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        if self.mix <= 0.0 && !self.wet_only {
            return;
        }
        if self.last_lc != self.lowcut_hz {
            let h = Coeffs::design(FilterKind::HighPass, self.sr, self.lowcut_hz, 0.707, 0.0);
            self.hp[0].set(h);
            self.hp[1].set(h);
            self.last_lc = self.lowcut_hz;
        }
        let fb = 0.7 + 0.28 * self.room_size.clamp(0.0, 1.0);
        let damp = 0.1 + 0.8 * self.damping.clamp(0.0, 1.0);
        let pd = (self.predelay_ms * 0.001 * self.sr).clamp(0.0, self.pre[0].capacity() as f32 - 2.0) as usize;
        let wet1 = 0.5 * (1.0 + self.width);
        let wet2 = 0.5 * (1.0 - self.width);
        for i in 0..l.len() {
            let input = (l[i] + r[i]) * 0.015;
            let mut out = [0.0f32; 2];
            for c in 0..2 {
                self.pre[c].push(input);
                let x = self.hp[c].process(self.pre[c].tap(pd));
                let mut acc = 0.0;
                for comb in &mut self.combs[c] {
                    acc += comb.process(x, fb, damp);
                }
                for ap in &mut self.allpasses[c] {
                    acc = ap.process(acc);
                }
                out[c] = acc;
            }
            let wl = out[0] * wet1 + out[1] * wet2;
            let wr = out[1] * wet1 + out[0] * wet2;
            if self.wet_only {
                // level of the solo'd tail does not collapse when Mix is set very low
                let g = self.mix.max(0.25) * db_to_lin(6.0) * 3.0;
                l[i] = wl * g;
                r[i] = wr * g;
            } else {
                let g = self.mix * db_to_lin(6.0) * 3.0;
                l[i] += wl * g;
                r[i] += wr * g;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delay_echoes() {
        let mut d = StereoDelay::default();
        d.mix = 1.0;
        d.time_l_ms = 10.0;
        d.time_r_ms = 10.0;
        d.lowcut_hz = 20.0;
        d.highcut_hz = 20000.0;
        d.prepare(48000.0);
        let mut l = vec![0.0f32; 2000];
        l[0] = 1.0;
        let mut r = l.clone();
        d.process(&mut l, &mut r);
        let peak = l[470..500].iter().fold(0.0f32, |a, b| a.max(b.abs()));
        assert!(peak > 0.3, "{peak}");
    }

    #[test]
    fn reverb_tail_decays_and_is_finite() {
        let mut v = Reverb::default();
        v.mix = 0.5;
        v.prepare(48000.0);
        let mut l = vec![0.0f32; 96000];
        l[0] = 1.0;
        let mut r = l.clone();
        for c in 0..(96000 / 480) {
            v.process(&mut l[c * 480..(c + 1) * 480], &mut r[c * 480..(c + 1) * 480]);
        }
        assert!(l.iter().all(|x| x.is_finite()));
        let early: f32 = l[1000..5000].iter().map(|x| x * x).sum();
        let late: f32 = l[80000..96000].iter().map(|x| x * x).sum();
        assert!(early > 0.0 && late < early);
    }
}

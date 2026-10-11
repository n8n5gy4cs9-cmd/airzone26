//! Maximizer: lookahead true-peak limiter with five release/"character" algorithms
//! (AZ-1 .. AZ-5) and a noise-shaped dither stage.

use std::collections::VecDeque;

use crate::biquad::Crossover;
use crate::util::{db_to_lin, lin_to_db, time_coeff, DelayLine, Rng};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum LimiterMode {
    /// Transparent, short lookahead, fixed release.
    Clean,
    /// Program-dependent release.
    #[default]
    Balanced,
    /// 3-band limiter + safety stage; keeps lows from pumping highs.
    Multiband,
    /// 4-band, saturating pre-stage, loudest.
    Aggressive,
    /// Crest-factor adaptive release.
    Adaptive,
    /// Soft clipper in front of a short limiter: keeps drum transients, adds density.
    Clipper,
    /// Long lookahead and slow, very smooth release: gentle and "invisible".
    Smooth,
}

impl LimiterMode {
    pub const ALL: [LimiterMode; 7] = [
        LimiterMode::Clean,
        LimiterMode::Balanced,
        LimiterMode::Multiband,
        LimiterMode::Aggressive,
        LimiterMode::Adaptive,
        LimiterMode::Clipper,
        LimiterMode::Smooth,
    ];
    pub fn name(&self) -> &'static str {
        match self {
            LimiterMode::Clean => "AZ-1 Clean",
            LimiterMode::Balanced => "AZ-2 Balanced",
            LimiterMode::Multiband => "AZ-3 Multiband",
            LimiterMode::Aggressive => "AZ-4 Aggressive",
            LimiterMode::Adaptive => "AZ-5 Adaptive",
            LimiterMode::Clipper => "AZ-6 Clip + Limit",
            LimiterMode::Smooth => "AZ-7 Smooth",
        }
    }
    fn lookahead_ms(&self) -> f32 {
        match self {
            LimiterMode::Clean => 1.0,
            LimiterMode::Balanced => 2.0,
            LimiterMode::Multiband => 2.5,
            LimiterMode::Aggressive => 3.0,
            LimiterMode::Adaptive => 2.0,
            LimiterMode::Clipper => 1.0,
            LimiterMode::Smooth => 5.0,
        }
    }
    fn bands(&self) -> usize {
        match self {
            LimiterMode::Multiband => 3,
            LimiterMode::Aggressive => 4,
            _ => 1,
        }
    }
}

/// Lookahead gain computer: sliding minimum -> release smoothing -> moving average.
#[derive(Clone, Debug)]
struct LimiterCore {
    n: usize,
    counter: u64,
    deque: VecDeque<(u64, f32)>,
    g_s: f32,
    ring: Vec<f32>,
    pos: usize,
    sum: f64,
}

impl LimiterCore {
    fn new(n: usize) -> Self {
        let n = n.max(2);
        Self {
            n,
            counter: 0,
            deque: VecDeque::with_capacity(n + 4),
            g_s: 1.0,
            ring: vec![1.0; n],
            pos: 0,
            sum: n as f64,
        }
    }
    fn reset(&mut self) {
        self.counter = 0;
        self.deque.clear();
        self.g_s = 1.0;
        self.ring.iter_mut().for_each(|x| *x = 1.0);
        self.sum = self.n as f64;
    }
    /// Returns (smoothed gain, release-smoother state).
    #[inline]
    fn push(&mut self, req: f32, rel_coef: f32) -> f32 {
        self.counter += 1;
        while let Some(&(_, v)) = self.deque.back() {
            if v >= req {
                self.deque.pop_back();
            } else {
                break;
            }
        }
        self.deque.push_back((self.counter, req));
        while let Some(&(c, _)) = self.deque.front() {
            if c + (self.n as u64) <= self.counter {
                self.deque.pop_front();
            } else {
                break;
            }
        }
        let m = self.deque.front().map(|x| x.1).unwrap_or(1.0);
        self.g_s = if m < self.g_s { m } else { m - (m - self.g_s) * rel_coef };
        // moving average over n samples
        self.sum += (self.g_s - self.ring[self.pos]) as f64;
        self.ring[self.pos] = self.g_s;
        self.pos = (self.pos + 1) % self.n;
        (self.sum / self.n as f64) as f32
    }
    fn delay(&self) -> usize {
        self.n - 1
    }
}

/// 4-point Hermite inter-sample peak estimate between s1 and s2.
#[inline]
fn true_peak_between(s0: f32, s1: f32, s2: f32, s3: f32) -> f32 {
    let c0 = s1;
    let c1 = 0.5 * (s2 - s0);
    let c2 = s0 - 2.5 * s1 + 2.0 * s2 - 0.5 * s3;
    let c3 = 0.5 * (s3 - s0) + 1.5 * (s1 - s2);
    let mut p: f32 = 0.0;
    for t in [0.25f32, 0.5, 0.75] {
        p = p.max((((c3 * t + c2) * t + c1) * t + c0).abs());
    }
    p
}

#[derive(Clone, Debug, Default)]
struct Hist([f32; 4]);

impl Hist {
    #[inline]
    fn push(&mut self, x: f32) -> f32 {
        self.0 = [self.0[1], self.0[2], self.0[3], x];
        true_peak_between(self.0[0], self.0[1], self.0[2], self.0[3]).max(x.abs())
    }
}

#[derive(Clone, Debug)]
struct Stage {
    cores: [LimiterCore; 2],
    delays: [DelayLine; 2],
    hist: [Hist; 2],
}

impl Stage {
    fn new(n: usize) -> Self {
        let c = LimiterCore::new(n);
        let d = c.delay();
        Self {
            cores: [c.clone(), c],
            delays: [DelayLine::new(d + 2), DelayLine::new(d + 2)],
            hist: [Hist::default(), Hist::default()],
        }
    }
    fn reset(&mut self) {
        self.cores.iter_mut().for_each(|c| c.reset());
        self.delays.iter_mut().for_each(|d| d.clear());
        self.hist = [Hist::default(), Hist::default()];
    }
    fn delay(&self) -> usize {
        self.cores[0].delay()
    }
    /// Process one stereo sample against `ceiling` (linear).
    #[inline]
    fn process(
        &mut self,
        x: [f32; 2],
        ceiling: f32,
        rel_coef: f32,
        independence: f32,
        true_peak: bool,
    ) -> ([f32; 2], f32) {
        let mut req = [1.0f32; 2];
        for c in 0..2 {
            let p = if true_peak { self.hist[c].push(x[c]) } else { x[c].abs() };
            req[c] = if p > ceiling { ceiling / p } else { 1.0 };
        }
        let linked = req[0].min(req[1]);
        let mut out = [0.0; 2];
        let mut gmin: f32 = 1.0;
        for c in 0..2 {
            let r = linked + (req[c] - linked) * independence;
            let g = self.cores[c].push(r, rel_coef);
            self.delays[c].push(x[c]);
            out[c] = self.delays[c].tap(self.cores[c].delay()) * g;
            gmin = gmin.min(g);
        }
        (out, gmin)
    }
}

#[derive(Clone, Debug)]
pub struct Maximizer {
    pub mode: LimiterMode,
    /// Input drive in dB (pushes the signal into the ceiling).
    pub gain_db: f32,
    pub ceiling_db: f32,
    pub release_ms: f32,
    /// 0..1 soft-saturation blended before the limiter.
    pub character: f32,
    /// 0 = fully linked, 1 = independent channels.
    pub independence: f32,
    pub true_peak: bool,
    /// Gain reduction in dB (positive), for metering.
    pub meter_gr: f32,
    sr: f32,
    built_mode: Option<LimiterMode>,
    built_sr: f32,
    stages: Vec<Stage>,
    safety: Stage,
    xo: [Crossover; 2],
    band_delay: Vec<[DelayLine; 2]>,
    last_xo_sr: f32,
    crest_env: f32,
    rms_env: f32,
    latency: usize,
}

impl Default for Maximizer {
    fn default() -> Self {
        Self {
            mode: LimiterMode::Balanced,
            gain_db: 0.0,
            ceiling_db: -0.3,
            release_ms: 120.0,
            character: 0.0,
            independence: 0.0,
            true_peak: true,
            meter_gr: 0.0,
            sr: 44100.0,
            built_mode: None,
            built_sr: 0.0,
            stages: Vec::new(),
            safety: Stage::new(32),
            xo: [Crossover::new(3), Crossover::new(3)],
            band_delay: Vec::new(),
            last_xo_sr: 0.0,
            crest_env: 0.0,
            rms_env: 0.0,
            latency: 0,
        }
    }
}

const SAFETY_MS: f32 = 0.8;

impl Maximizer {
    pub fn prepare(&mut self, sr: f32) {
        self.sr = sr;
        self.built_mode = None;
        self.rebuild();
    }

    fn ms_to_n(&self, ms: f32) -> usize {
        ((ms * 0.001 * self.sr).round() as usize).max(2)
    }

    fn rebuild(&mut self) {
        let bands = self.mode.bands();
        let n = self.ms_to_n(self.mode.lookahead_ms());
        self.stages = (0..bands).map(|_| Stage::new(n)).collect();
        self.safety = Stage::new(self.ms_to_n(SAFETY_MS));
        self.xo = [Crossover::new(bands.max(2)), Crossover::new(bands.max(2))];
        let freqs = [180.0f32, 2000.0, 7000.0];
        let f: Vec<f32> = freqs.iter().take(bands.max(2) - 1).cloned().collect();
        let f4 = if bands == 3 { vec![180.0, 2200.0] } else { f };
        for x in &mut self.xo {
            x.set_freqs(self.sr, &f4);
        }
        self.band_delay = Vec::new();
        self.built_mode = Some(self.mode);
        self.latency = self.stages[0].delay() + self.safety.delay();
        self.crest_env = 0.0;
        self.rms_env = 0.0;
    }

    pub fn latency(&self) -> usize {
        self.latency
    }

    pub fn reset(&mut self) {
        self.stages.iter_mut().for_each(|s| s.reset());
        self.safety.reset();
        self.xo.iter_mut().for_each(|x| x.reset());
        self.meter_gr = 0.0;
    }

    pub fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        if self.built_mode != Some(self.mode) {
            self.rebuild();
        }
        let ceiling = db_to_lin(self.ceiling_db);
        let drive = db_to_lin(self.gain_db);
        let bands = self.mode.bands();
        let mut gr_min: f32 = 1.0;
        let character = self.character;
        let sat = |x: f32, c: f32| -> f32 {
            let y = ceiling * (x / ceiling).tanh();
            x + (y - x) * c
        };
        for i in 0..l.len() {
            let mut x = [l[i] * drive, r[i] * drive];
            if self.mode == LimiterMode::Aggressive || self.mode == LimiterMode::Clipper {
                // saturating pre-stage; more drive -> more saturation (the clipper always clips hard)
                let c = if self.mode == LimiterMode::Clipper { (0.7 + 0.3 * self.character).min(1.0) } else { (0.3 + 0.7 * self.character).min(1.0) };
                for v in &mut x {
                    *v = sat(*v, c);
                }
            } else if character > 0.0 {
                for v in &mut x {
                    *v = sat(*v, character);
                }
            }
            // program-dependent release coefficient
            let depth_db = -lin_to_db(self.stages[0].cores[0].g_s);
            let rel_ms_scale = match self.mode {
                LimiterMode::Clean | LimiterMode::Multiband | LimiterMode::Aggressive | LimiterMode::Clipper => 1.0,
                LimiterMode::Balanced => 0.3 + 0.7 * (depth_db / 6.0).clamp(0.0, 1.0),
                LimiterMode::Smooth => 0.8 + 1.0 * (depth_db / 6.0).clamp(0.0, 1.0),
                LimiterMode::Adaptive => {
                    let a = x[0].abs().max(x[1].abs());
                    self.crest_env = a + (self.crest_env - a) * time_coeff(300.0, self.sr).max(0.0);
                    let sq = 0.5 * (x[0] * x[0] + x[1] * x[1]);
                    self.rms_env = sq + (self.rms_env - sq) * time_coeff(300.0, self.sr);
                    let crest_db = lin_to_db(self.crest_env) - 10.0 * (self.rms_env + 1e-12).log10();
                    // peaky material (high crest) -> faster release, dense -> slower
                    0.35 + 0.9 * (1.0 - ((crest_db - 6.0) / 12.0).clamp(0.0, 1.0))
                }
            };
            let rel = time_coeff(self.release_ms * rel_ms_scale, self.sr);

            let y = if bands == 1 {
                let (o, g) = self.stages[0].process(x, ceiling, rel, self.independence, self.true_peak);
                gr_min = gr_min.min(g);
                o
            } else {
                let bl = self.xo[0].process(x[0]);
                let br = self.xo[1].process(x[1]);
                let mut sum = [0.0f32; 2];
                for b in 0..bands {
                    // each band limited to a share of the ceiling so that sums stay near it
                    let band_ceiling = ceiling * if bands == 3 { 0.55 } else { 0.5 };
                    let band_rel = match b {
                        0 => rel_scale(self.release_ms, 1.6, self.sr),
                        1 => rel_scale(self.release_ms, 1.0, self.sr),
                        2 => rel_scale(self.release_ms, 0.6, self.sr),
                        _ => rel_scale(self.release_ms, 0.35, self.sr),
                    };
                    let (o, g) = self.stages[b].process(
                        [bl[b], br[b]],
                        band_ceiling,
                        band_rel,
                        self.independence,
                        false,
                    );
                    gr_min = gr_min.min(g);
                    sum[0] += o[0];
                    sum[1] += o[1];
                }
                // multiband output is expected to be near the ceiling; the safety stage catches the rest
                sum
            };
            let (o, _g) = self.safety.process(y, ceiling, time_coeff(30.0, self.sr), self.independence, self.true_peak);
            l[i] = o[0];
            r[i] = o[1];
        }
        let target = -lin_to_db(gr_min);
        self.meter_gr += (target - self.meter_gr) * 0.4;
    }
}

fn rel_scale(ms: f32, k: f32, sr: f32) -> f32 {
    time_coeff(ms * k, sr)
}

// ------------------------------------------------------------------- Dither

#[derive(Clone, Debug)]
pub struct Dither {
    /// 0 = off, otherwise target word length.
    pub bits: u32,
    pub shaped: bool,
    rng: Rng,
    e1: [f32; 2],
}

impl Default for Dither {
    fn default() -> Self {
        Self { bits: 0, shaped: true, rng: Rng(0x1234_5678), e1: [0.0; 2] }
    }
}

impl Dither {
    pub fn reset(&mut self) {
        self.e1 = [0.0; 2];
    }
    pub fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        if self.bits == 0 {
            return;
        }
        let steps = (1u64 << (self.bits - 1)) as f32;
        let lsb = 1.0 / steps;
        for i in 0..l.len() {
            for (ch, buf) in [&mut *l, &mut *r].into_iter().enumerate() {
                let x = buf[i];
                let tpdf = (self.rng.next_f32() + self.rng.next_f32()) * 0.5 * lsb;
                let v = if self.shaped { x - self.e1[ch] } else { x };
                let q = ((v + tpdf) * steps).round() / steps;
                self.e1[ch] = if self.shaped { (q - v).clamp(-lsb * 4.0, lsb * 4.0) } else { 0.0 };
                buf[i] = q;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loud_signal(n: usize) -> (Vec<f32>, Vec<f32>) {
        let l: Vec<f32> = (0..n).map(|i| 1.8 * (i as f32 * 0.021).sin() + 0.5 * (i as f32 * 0.37).sin()).collect();
        let r: Vec<f32> = (0..n).map(|i| 1.5 * (i as f32 * 0.017).sin() + 0.4 * (i as f32 * 0.29).sin()).collect();
        (l, r)
    }

    #[test]
    fn never_exceeds_ceiling() {
        for mode in LimiterMode::ALL {
            let mut m = Maximizer::default();
            m.mode = mode;
            m.ceiling_db = -1.0;
            m.true_peak = false;
            m.prepare(48000.0);
            let (mut l, mut r) = loud_signal(49152);
            for c in 0..(l.len() / 512) {
                let a = c * 512;
                m.process(&mut l[a..a + 512], &mut r[a..a + 512]);
            }
            let lim = db_to_lin(-1.0) * 1.001;
            let peak = l.iter().chain(r.iter()).skip(2000).fold(0.0f32, |a, b| a.max(b.abs()));
            assert!(peak <= lim, "{:?} peak {}", mode, peak);
            assert!(peak > lim * 0.5, "{:?} too quiet {}", mode, peak);
        }
    }

    #[test]
    fn transparent_below_ceiling() {
        let mut m = Maximizer::default();
        m.mode = LimiterMode::Clean;
        m.prepare(48000.0);
        let src: Vec<f32> = (0..9600).map(|i| 0.2 * (i as f32 * 0.05).sin()).collect();
        let mut l = src.clone();
        let mut r = src.clone();
        m.process(&mut l, &mut r);
        let d = m.latency();
        for i in 1000..9000 {
            assert!((l[i] - src[i - d]).abs() < 1e-4, "i={} {} {}", i, l[i], src[i - d]);
        }
    }

    #[test]
    fn dither_quantizes() {
        let mut d = Dither::default();
        d.bits = 16;
        let mut l = vec![0.1234567f32; 1000];
        let mut r = l.clone();
        d.process(&mut l, &mut r);
        let step = 1.0 / 32768.0;
        for x in &l {
            let k = x / step;
            assert!((k - k.round()).abs() < 1e-2);
        }
    }
}


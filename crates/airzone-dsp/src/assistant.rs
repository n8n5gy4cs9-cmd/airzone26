//! Master Assistant: listens to the programme, measures it, and *suggests* settings
//! for a genre / loudness target. It guides, it does not decide: every suggestion
//! carries an explanation and only modules the user allowed are touched.

use std::sync::Arc;

use realfft::num_complex::Complex32;
use realfft::{RealFftPlanner, RealToComplex};

use crate::biquad::FilterKind;
use crate::chain::{ModuleId, MODULE_COUNT};
use crate::maximizer::LimiterMode;
use crate::meter::{LoudnessMeter, StereoMeter};
use crate::spectral::{SpectrumProfile, MATCH_BANDS};
use crate::util::TAU;

const AN_FFT: usize = 2048;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Genre {
    #[default]
    Universal,
    Pop,
    Rock,
    HipHop,
    Electronic,
    Acoustic,
    Metal,
    Classical,
}

impl Genre {
    pub const ALL: [Genre; 8] = [
        Genre::Universal,
        Genre::Pop,
        Genre::Rock,
        Genre::HipHop,
        Genre::Electronic,
        Genre::Acoustic,
        Genre::Metal,
        Genre::Classical,
    ];
    pub fn name(&self) -> &'static str {
        match self {
            Genre::Universal => "Universal",
            Genre::Pop => "Pop",
            Genre::Rock => "Rock",
            Genre::HipHop => "Hip-Hop / R&B",
            Genre::Electronic => "Electronic / EDM",
            Genre::Acoustic => "Acoustic / Folk / Jazz",
            Genre::Metal => "Metal / Hard Rock",
            Genre::Classical => "Classical / Cinematic",
        }
    }
    /// (slope dB/oct, bass shelf dB, air shelf dB, preferred limiter, comp ratio, min width)
    fn profile(&self) -> (f32, f32, f32, LimiterMode, f32, f32) {
        match self {
            Genre::Universal => (-4.5, 0.0, 0.0, LimiterMode::Balanced, 2.0, 0.15),
            Genre::Pop => (-4.2, 1.0, 1.5, LimiterMode::Balanced, 2.2, 0.2),
            Genre::Rock => (-4.0, 0.5, 0.5, LimiterMode::Adaptive, 2.5, 0.15),
            Genre::HipHop => (-5.0, 3.0, 0.5, LimiterMode::Multiband, 2.5, 0.12),
            Genre::Electronic => (-4.6, 2.0, 1.5, LimiterMode::Aggressive, 2.0, 0.25),
            Genre::Acoustic => (-4.8, 0.0, 0.5, LimiterMode::Clean, 1.6, 0.18),
            Genre::Metal => (-3.8, 0.5, 0.0, LimiterMode::Aggressive, 3.0, 0.15),
            Genre::Classical => (-5.2, -0.5, 0.5, LimiterMode::Clean, 1.3, 0.25),
        }
    }
    /// Target tonal shape: dB per log-band, zero-mean.
    pub fn target_shape(&self) -> [f32; MATCH_BANDS] {
        let (slope, bass, air, ..) = self.profile();
        let mut out = [0.0f32; MATCH_BANDS];
        for b in 0..MATCH_BANDS {
            let f = 20.0 * 1000f32.powf(b as f32 / (MATCH_BANDS - 1) as f32);
            let ss = |e0: f32, e1: f32, x: f32| {
                let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
                t * t * (3.0 - 2.0 * t)
            };
            out[b] = slope * (f / 1000.0).log2() + bass * (1.0 - ss(60.0, 250.0, f)) + air * ss(5000.0, 14000.0, f);
        }
        let mean = out.iter().sum::<f32>() / MATCH_BANDS as f32;
        out.iter_mut().for_each(|x| *x -= mean);
        out
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LoudnessTarget {
    /// Streaming normalisation friendly: -14 LUFS, -1 dBTP
    Streaming,
    /// Competitive release: -9 LUFS, -0.3 dBTP
    Loud,
    /// Broadcast (EBU R128): -23 LUFS, -1 dBTP
    Broadcast,
    Custom { lufs: f32, ceiling_db: f32 },
}

impl LoudnessTarget {
    pub fn values(&self) -> (f32, f32) {
        match *self {
            LoudnessTarget::Streaming => (-14.0, -1.0),
            LoudnessTarget::Loud => (-9.0, -0.3),
            LoudnessTarget::Broadcast => (-23.0, -1.0),
            LoudnessTarget::Custom { lufs, ceiling_db } => (lufs, ceiling_db),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Analysis {
    pub lufs: f32,
    pub lra: f32,
    pub true_peak_db: f32,
    pub crest_db: f32,
    /// side / (mid+side) energy ratio, 0 (mono) .. 1
    pub width: f32,
    pub correlation: f32,
    pub shape: [f32; MATCH_BANDS],
    /// sub (<80 Hz) minus bass (80-250 Hz) in dB
    pub sub_minus_bass_db: f32,
    pub seconds: f32,
}

pub struct Analyzer {
    sr: f32,
    fft: Arc<dyn RealToComplex<f32>>,
    window: Vec<f32>,
    ring: [Vec<f32>; 2],
    pos: usize,
    hop: usize,
    buf: Vec<f32>,
    spec: [Vec<Complex32>; 2],
    scratch: Vec<Complex32>,
    profile: SpectrumProfile,
    loud: LoudnessMeter,
    stereo: StereoMeter,
    mid_e: f64,
    side_e: f64,
    sq_sum: f64,
    samples: u64,
    peak: f32,
    sub_e: f64,
    bass_e: f64,
}

impl Analyzer {
    pub fn new(sr: f32) -> Self {
        let mut planner = RealFftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(AN_FFT);
        let scratch = fft.make_scratch_vec();
        Self {
            sr,
            fft,
            window: (0..AN_FFT).map(|i| 0.5 - 0.5 * (TAU * i as f32 / AN_FFT as f32).cos()).collect(),
            ring: [vec![0.0; AN_FFT], vec![0.0; AN_FFT]],
            pos: 0,
            hop: 0,
            buf: vec![0.0; AN_FFT],
            spec: [vec![Complex32::new(0.0, 0.0); AN_FFT / 2 + 1], vec![Complex32::new(0.0, 0.0); AN_FFT / 2 + 1]],
            scratch,
            profile: SpectrumProfile::default(),
            loud: LoudnessMeter::new(sr),
            stereo: StereoMeter::default(),
            mid_e: 0.0,
            side_e: 0.0,
            sq_sum: 0.0,
            samples: 0,
            peak: 0.0,
            sub_e: 0.0,
            bass_e: 0.0,
        }
    }

    pub fn reset(&mut self) {
        *self = Self::new(self.sr);
    }

    pub fn seconds(&self) -> f32 {
        self.samples as f32 / self.sr
    }

    pub fn process(&mut self, l: &[f32], r: &[f32]) {
        self.loud.process(l, r);
        self.stereo.process(l, r, self.sr);
        for i in 0..l.len() {
            let (a, b) = (l[i], r[i]);
            let m = 0.5 * (a + b);
            let s = 0.5 * (a - b);
            self.mid_e += (m * m) as f64;
            self.side_e += (s * s) as f64;
            self.sq_sum += (0.5 * (a * a + b * b)) as f64;
            self.peak = self.peak.max(a.abs()).max(b.abs());
            self.ring[0][self.pos] = a;
            self.ring[1][self.pos] = b;
            self.pos = (self.pos + 1) % AN_FFT;
            self.samples += 1;
            self.hop += 1;
            if self.hop >= AN_FFT / 2 {
                self.hop = 0;
                self.frame();
            }
        }
    }

    fn frame(&mut self) {
        for c in 0..2 {
            for k in 0..AN_FFT {
                self.buf[k] = self.ring[c][(self.pos + k) % AN_FFT] * self.window[k];
            }
            let _ = self.fft.process_with_scratch(&mut self.buf, &mut self.spec[c], &mut self.scratch);
        }
        let norm = 1.0 / AN_FFT as f32;
        for s in self.spec.iter_mut() {
            s.iter_mut().for_each(|v| *v *= norm);
        }
        self.profile.accumulate(self.sr, &self.spec[0], &self.spec[1]);
        let bin_hz = self.sr / AN_FFT as f32;
        for k in 1..self.spec[0].len() {
            let f = k as f32 * bin_hz;
            let e = 0.5 * (self.spec[0][k].norm_sqr() + self.spec[1][k].norm_sqr()) as f64;
            if f < 80.0 {
                self.sub_e += e;
            } else if f < 250.0 {
                self.bass_e += e;
            }
        }
    }

    pub fn result(&self) -> Option<Analysis> {
        if !self.profile.is_valid() || self.samples == 0 {
            return None;
        }
        let rms = (self.sq_sum / self.samples as f64).sqrt() as f32;
        let crest = 20.0 * (self.peak.max(1e-6) / rms.max(1e-6)).log10();
        let width = (self.side_e / (self.mid_e + self.side_e + 1e-12)).sqrt() as f32;
        Some(Analysis {
            lufs: if self.loud.integrated > -69.0 { self.loud.integrated } else { self.loud.momentary },
            lra: self.loud.lra,
            true_peak_db: self.loud.true_peak_db,
            crest_db: crest,
            width,
            correlation: self.stereo.correlation,
            shape: self.profile.shape_db(),
            sub_minus_bass_db: (10.0 * ((self.sub_e + 1e-12) / (self.bass_e + 1e-12)).log10()) as f32,
            seconds: self.seconds(),
        })
    }
}

#[derive(Clone, Debug)]
pub struct EqSuggestion {
    pub kind: FilterKind,
    pub freq: f32,
    pub gain_db: f32,
    pub q: f32,
}

#[derive(Clone, Debug, Default)]
pub struct Suggestion {
    pub eq: Vec<EqSuggestion>,
    pub highpass_hz: Option<f32>,
    /// (threshold dB, ratio) for the Dynamics module
    pub dynamics: Option<(f32, f32)>,
    pub exciter_high_amount: Option<f32>,
    pub imager_high_width: Option<f32>,
    pub bass_sub_mono: bool,
    pub bass_sub_gain_db: Option<f32>,
    pub stabilizer: bool,
    pub unlimiter_amount: Option<f32>,
    pub maximizer_gain_db: Option<f32>,
    pub ceiling_db: Option<f32>,
    pub limiter_mode: Option<LimiterMode>,
    pub notes: Vec<String>,
}

pub struct AssistantOptions {
    pub genre: Genre,
    pub target: LoudnessTarget,
    /// Which modules the assistant may adjust (the "custom flow").
    pub allow: [bool; MODULE_COUNT],
}

impl Default for AssistantOptions {
    fn default() -> Self {
        let mut allow = [false; MODULE_COUNT];
        for m in [
            ModuleId::Equalizer,
            ModuleId::Dynamics,
            ModuleId::Exciter,
            ModuleId::Imager,
            ModuleId::BassControl,
            ModuleId::Maximizer,
            ModuleId::Unlimiter,
        ] {
            allow[m.index()] = true;
        }
        Self { genre: Genre::Universal, target: LoudnessTarget::Streaming, allow }
    }
}

fn band_freq(b: usize) -> f32 {
    20.0 * 1000f32.powf(b as f32 / (MATCH_BANDS - 1) as f32)
}

pub fn suggest(a: &Analysis, o: &AssistantOptions) -> Suggestion {
    let (_slope, _bass, _air, limiter, comp_ratio, min_width) = o.genre.profile();
    let (target_lufs, ceiling) = o.target.values();
    let allow = |m: ModuleId| o.allow[m.index()];
    let mut s = Suggestion::default();

    // --- tonal balance
    if allow(ModuleId::Equalizer) {
        let tgt = o.genre.target_shape();
        let mut diff: Vec<f32> = (0..MATCH_BANDS).map(|b| a.shape[b] - tgt[b]).collect();
        let mean = diff.iter().sum::<f32>() / MATCH_BANDS as f32;
        diff.iter_mut().for_each(|d| *d -= mean);
        let src = diff.clone();
        for b in 0..MATCH_BANDS {
            let (mut acc, mut n) = (0.0, 0.0);
            for j in -3i32..=3 {
                let idx = (b as i32 + j).clamp(0, MATCH_BANDS as i32 - 1) as usize;
                acc += src[idx];
                n += 1.0;
            }
            diff[b] = acc / n;
        }
        let mut used = [false; MATCH_BANDS];
        for _ in 0..5 {
            let mut best = None;
            for b in 3..MATCH_BANDS - 1 {
                if used[b] || !(30.0..=18000.0).contains(&band_freq(b)) {
                    continue;
                }
                if best.map_or(true, |(_, v): (usize, f32)| diff[b].abs() > v.abs()) {
                    best = Some((b, diff[b]));
                }
            }
            if let Some((b, d)) = best {
                if d.abs() < 1.5 {
                    break;
                }
                for j in b.saturating_sub(6)..(b + 7).min(MATCH_BANDS) {
                    used[j] = true;
                }
                let f = band_freq(b);
                let gain = (-d * 0.7).clamp(-4.0, 4.0);
                let kind = if f < 70.0 {
                    FilterKind::LowShelf
                } else if f > 11000.0 {
                    FilterKind::HighShelf
                } else {
                    FilterKind::Bell
                };
                s.eq.push(EqSuggestion { kind, freq: f, gain_db: gain, q: 0.8 });
                s.notes.push(format!(
                    "EQ: {:+.1} dB at {:.0} Hz ({} {:.1} dB vs {} target)",
                    gain,
                    f,
                    if d > 0.0 { "excess" } else { "lack" },
                    d.abs(),
                    o.genre.name()
                ));
            } else {
                break;
            }
        }
        s.highpass_hz = Some(24.0);
    }

    // --- dynamics / transients
    let mut makeup = 0.0;
    if allow(ModuleId::Dynamics) {
        if a.crest_db > 15.0 {
            let thr = -20.0;
            s.dynamics = Some((thr, comp_ratio));
            makeup = 1.0;
            s.notes.push(format!("Dynamics: crest factor {:.1} dB is high, gentle {:.1}:1 control", a.crest_db, comp_ratio));
        } else if a.crest_db > 11.0 {
            s.dynamics = Some((-16.0, (comp_ratio * 0.75).max(1.2)));
            makeup = 0.5;
            s.notes.push(format!("Dynamics: crest {:.1} dB is healthy, light glue only", a.crest_db));
        } else {
            s.notes.push(format!("Dynamics: crest {:.1} dB is already dense, compression skipped", a.crest_db));
        }
    }
    if allow(ModuleId::Unlimiter) && a.crest_db < 9.0 && a.lufs > -10.5 {
        s.unlimiter_amount = Some(((9.0 - a.crest_db) / 5.0).clamp(0.2, 0.8));
        s.notes.push("Unlimiter: material looks over-limited, restoring some peak dynamics before re-limiting".into());
    }

    // --- stereo / low end
    if allow(ModuleId::BassControl) {
        s.bass_sub_mono = true;
        if a.sub_minus_bass_db > 4.0 {
            s.bass_sub_gain_db = Some(-(a.sub_minus_bass_db - 3.0).min(4.0));
            s.notes.push(format!("Bass Control: sub is {:.1} dB above the bass region, trimming it", a.sub_minus_bass_db));
        } else if a.sub_minus_bass_db < -9.0 {
            s.bass_sub_gain_db = Some(((-9.0 - a.sub_minus_bass_db) * 0.5).min(3.0));
            s.notes.push("Bass Control: sub region is weak, small lift".into());
        }
        s.notes.push("Bass Control: sub band summed to mono for translation".into());
    }
    if allow(ModuleId::Imager) {
        if a.width < min_width {
            s.imager_high_width = Some(1.25);
            s.notes.push(format!("Imager: width {:.2} is narrow for {}, widening highs", a.width, o.genre.name()));
        } else if a.correlation < 0.1 {
            s.imager_high_width = Some(0.85);
            s.notes.push("Imager: low correlation, narrowing highs for mono compatibility".into());
        }
    }
    if allow(ModuleId::Exciter) {
        let hf: f32 = a.shape[MATCH_BANDS - 14..MATCH_BANDS - 4].iter().sum::<f32>() / 10.0
            - o.genre.target_shape()[MATCH_BANDS - 14..MATCH_BANDS - 4].iter().sum::<f32>() / 10.0;
        if hf < -2.0 {
            s.exciter_high_amount = Some(0.35);
            s.notes.push("Exciter: top end is dull relative to target, adding gentle harmonics".into());
        }
    }
    if allow(ModuleId::Stabilizer) {
        let dev = a.shape.iter().zip(o.genre.target_shape().iter()).map(|(x, t)| (x - t).abs()).sum::<f32>() / MATCH_BANDS as f32;
        if dev > 4.0 {
            s.stabilizer = true;
            s.notes.push(format!("Stabilizer: average tonal deviation {:.1} dB, enabling adaptive balance", dev));
        }
    }

    // --- loudness
    if allow(ModuleId::Maximizer) {
        let need = target_lufs - a.lufs - makeup;
        s.maximizer_gain_db = Some(need.clamp(0.0, 12.0));
        s.ceiling_db = Some(ceiling);
        s.limiter_mode = Some(limiter);
        if need < 0.0 {
            s.notes.push(format!("Maximizer: programme ({:.1} LUFS) is already louder than target ({:.1}), no extra gain", a.lufs, target_lufs));
        } else {
            s.notes.push(format!(
                "Maximizer: +{:.1} dB to move from {:.1} to {:.1} LUFS, ceiling {:.1} dBTP, mode {}",
                need.min(12.0),
                a.lufs,
                target_lufs,
                ceiling,
                limiter.name()
            ));
        }
        if need > 8.0 {
            s.notes.push("Warning: more than 8 dB of limiting needed; consider a lower loudness target to preserve dynamics".into());
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn white(n: usize, seed: u32, amp: f32) -> Vec<f32> {
        let mut r = crate::util::Rng(seed);
        (0..n).map(|_| r.next_f32() * amp).collect()
    }

    #[test]
    fn bright_signal_gets_high_cut_and_gain() {
        let sr = 48000.0;
        let mut an = Analyzer::new(sr);
        let l = white(sr as usize * 6, 1, 0.1);
        let r = white(sr as usize * 6, 2, 0.1);
        for (a, b) in l.chunks(480).zip(r.chunks(480)) {
            an.process(a, b);
        }
        let res = an.result().expect("analysis");
        let sug = suggest(&res, &AssistantOptions::default());
        assert!(sug.eq.iter().any(|e| e.freq > 2000.0 && e.gain_db < 0.0), "{:?}", sug.eq);
        assert!(sug.maximizer_gain_db.unwrap() > 0.0);
        assert!(!sug.notes.is_empty());
    }

    #[test]
    fn target_shapes_are_zero_mean() {
        for g in Genre::ALL {
            let s = g.target_shape();
            assert!((s.iter().sum::<f32>() / MATCH_BANDS as f32).abs() < 1e-3);
        }
    }
}

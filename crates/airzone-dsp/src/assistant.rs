//! Master Assistant: listens to the programme, measures it, and *suggests* settings
//! for a genre / loudness target. It guides, it does not decide: every suggestion
//! carries an explanation and only modules the user allowed are touched.
//!
//! Design rules (to avoid "strange" advice):
//! * tonal advice is only derived from the musically reliable 90 Hz - 12 kHz range,
//! * the overall level offset is removed with a median, so a loud or quiet mix does not look tonal,
//! * a global tilt is corrected with one broad shelf instead of several fighting bells,
//! * narrow EQ moves are *cuts* of prominent excesses only, never narrow boosts,
//! * dynamics decisions use the peak-to-loudness ratio (PLR), which silence/intro sections do not skew,
//! * nothing is suggested when too little signal was heard.

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
    /// Peak-to-loudness ratio in dB (true peak minus integrated loudness).
    pub crest_db: f32,
    /// side / (mid+side) energy ratio, 0 (mono) .. 1
    pub width: f32,
    /// Long-term stereo correlation over the whole capture.
    pub correlation: f32,
    pub shape: [f32; MATCH_BANDS],
    /// sub (30-80 Hz) minus bass (80-250 Hz) in dB, per octave
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
    samples: u64,
    sub_e: f64,
    bass_e: f64,
    lr_sum: f64,
    ll_sum: f64,
    rr_sum: f64,
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
            samples: 0,
            sub_e: 0.0,
            bass_e: 0.0,
            lr_sum: 0.0,
            ll_sum: 0.0,
            rr_sum: 0.0,
        }
    }

    pub fn reset(&mut self) {
        *self = Self::new(self.sr);
    }

    pub fn profile(&self) -> &SpectrumProfile {
        &self.profile
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
            self.lr_sum += (a * b) as f64;
            self.ll_sum += (a * a) as f64;
            self.rr_sum += (b * b) as f64;
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
            // sub: 30-80 Hz, bass: 80-250 Hz (below 30 Hz there is rarely anything musical)
            if (30.0..80.0).contains(&f) {
                self.sub_e += e;
            } else if (80.0..250.0).contains(&f) {
                self.bass_e += e;
            }
        }
    }

    pub fn result(&self) -> Option<Analysis> {
        if !self.profile.is_valid() || self.samples == 0 {
            return None;
        }
        let lufs = if self.loud.integrated > -69.0 { self.loud.integrated } else { self.loud.momentary };
        // Peak-to-loudness ratio: robust against quiet intros / silence, unlike peak-to-RMS.
        let plr = (self.loud.true_peak_db - lufs).max(0.0);
        let width = (self.side_e / (self.mid_e + self.side_e + 1e-12)).sqrt() as f32;
        // long-term correlation over the whole capture (not the instantaneous meter value)
        let denom = (self.ll_sum * self.rr_sum).sqrt();
        let correlation = if denom > 1e-12 { (self.lr_sum / denom) as f32 } else { 1.0 };
        // energy per octave so that the wider bass region is not favoured over the sub region
        let sub_oct = (80.0f64 / 30.0).log2();
        let bass_oct = (250.0f64 / 80.0).log2();
        let sub_minus_bass = 10.0 * ((self.sub_e / sub_oct + 1e-12) / (self.bass_e / bass_oct + 1e-12)).log10();
        Some(Analysis {
            lufs,
            lra: self.loud.lra,
            true_peak_db: self.loud.true_peak_db,
            crest_db: plr,
            width,
            correlation,
            shape: self.profile.shape_db(),
            sub_minus_bass_db: sub_minus_bass as f32,
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

fn band_index(f: f32) -> usize {
    let t = (f / 20.0).ln() / 1000f32.ln();
    (t * (MATCH_BANDS - 1) as f32).round().clamp(0.0, (MATCH_BANDS - 1) as f32) as usize
}

/// Result of comparing the measured spectrum with the genre target in the reliable range.
pub struct Tonal {
    /// Excess tilt of the measurement over the target, dB per octave (positive = too bright).
    pub tilt_db_oct: f32,
    /// Remaining deviation (band index, dB) after the level offset and tilt were removed.
    pub residual: Vec<(usize, f32)>,
    /// RMS deviation from the target after removing the level offset only.
    pub rms_dev: f32,
}

pub fn tonal_analysis(a: &Analysis, genre: Genre) -> Tonal {
    let tgt = genre.target_shape();
    let lo = band_index(90.0);
    let hi = band_index(12000.0);
    let idx: Vec<usize> = (lo..=hi).collect();
    let mut d: Vec<f32> = idx.iter().map(|&b| a.shape[b] - tgt[b]).collect();
    // remove the overall level offset with a median (robust against a few odd bands)
    let mut sorted = d.clone();
    sorted.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
    let med = sorted[sorted.len() / 2];
    d.iter_mut().for_each(|x| *x -= med);
    let rms_dev = (d.iter().map(|x| x * x).sum::<f32>() / d.len() as f32).sqrt();
    // least-squares tilt versus log2(frequency)
    let xs: Vec<f32> = idx.iter().map(|&b| (band_freq(b) / 1000.0).log2()).collect();
    let n = d.len() as f32;
    let mx = xs.iter().sum::<f32>() / n;
    let my = d.iter().sum::<f32>() / n;
    let (mut sxy, mut sxx) = (0.0f32, 0.0f32);
    for i in 0..d.len() {
        sxy += (xs[i] - mx) * (d[i] - my);
        sxx += (xs[i] - mx) * (xs[i] - mx);
    }
    let slope = if sxx > 1.0e-6 { sxy / sxx } else { 0.0 };
    let icpt = my - slope * mx;
    let res0: Vec<f32> = (0..d.len()).map(|i| d[i] - (icpt + slope * xs[i])).collect();
    let mut res = res0.clone();
    for i in 0..res0.len() {
        let a0 = res0[i.saturating_sub(1)];
        let c0 = res0[(i + 1).min(res0.len() - 1)];
        res[i] = 0.25 * a0 + 0.5 * res0[i] + 0.25 * c0;
    }
    Tonal { tilt_db_oct: slope, residual: idx.into_iter().zip(res).collect(), rms_dev }
}

pub fn suggest(a: &Analysis, o: &AssistantOptions) -> Suggestion {
    let (_slope, _bass, _air, limiter, comp_ratio, min_width) = o.genre.profile();
    let (target_lufs, ceiling) = o.target.values();
    let allow = |m: ModuleId| o.allow[m.index()];
    let mut s = Suggestion::default();

    if a.seconds < 5.0 || a.lufs < -55.0 {
        s.notes.push("Not enough signal was heard. Play at least 5 seconds of the loudest, most representative section and analyse again.".into());
        return s;
    }
    let plr = a.crest_db;
    let tonal = tonal_analysis(a, o.genre);

    // --- tonal balance: one broad tilt shelf plus cuts of prominent excesses
    if allow(ModuleId::Equalizer) {
        s.highpass_hz = Some(24.0);
        s.notes.push("EQ: 24 Hz high-pass removes inaudible rumble that wastes limiter headroom".into());
        if tonal.tilt_db_oct.abs() > 0.7 {
            let gain = (-tonal.tilt_db_oct * 2.2).clamp(-3.5, 3.5);
            s.eq.push(EqSuggestion { kind: FilterKind::HighShelf, freq: 3000.0, gain_db: gain, q: 0.7 });
            s.notes.push(format!(
                "EQ: overall balance is {:.1} dB/oct {} than a typical {} master, {:+.1} dB high shelf at 3 kHz",
                tonal.tilt_db_oct.abs(),
                if tonal.tilt_db_oct > 0.0 { "brighter" } else { "darker" },
                o.genre.name(),
                gain
            ));
        }
        let mut peaks = tonal.residual.clone();
        peaks.sort_by(|x, y| y.1.partial_cmp(&x.1).unwrap_or(std::cmp::Ordering::Equal));
        let mut taken: Vec<usize> = Vec::new();
        for (b, r) in peaks {
            if r < 3.0 || s.eq.len() >= 4 {
                break;
            }
            if taken.iter().any(|t| (*t as i32 - b as i32).abs() < 8) {
                continue;
            }
            taken.push(b);
            let f = band_freq(b);
            let gain = (-r * 0.6).clamp(-3.0, -1.0);
            s.eq.push(EqSuggestion { kind: FilterKind::Bell, freq: f, gain_db: gain, q: 1.2 });
            s.notes.push(format!("EQ: {:.0} Hz stands out by {:.1} dB above the surrounding balance, {:+.1} dB bell", f, r, gain));
        }
        if s.eq.is_empty() {
            s.notes.push("EQ: tonal balance is already close to the target - no corrective EQ suggested".into());
        }
    }

    // --- dynamics / transients (decided from peak-to-loudness ratio)
    let mut makeup = 0.0;
    if allow(ModuleId::Dynamics) {
        let thr = (a.lufs - 4.0).clamp(-40.0, -10.0);
        if plr > 17.0 {
            s.dynamics = Some((thr, comp_ratio));
            makeup = 1.0;
            s.notes.push(format!("Dynamics: PLR {:.1} dB is very dynamic, gentle {:.1}:1 control below {:.0} dB", plr, comp_ratio, thr));
        } else if plr > 12.0 {
            s.dynamics = Some((thr + 2.0, (comp_ratio * 0.75).max(1.2)));
            makeup = 0.5;
            s.notes.push(format!("Dynamics: PLR {:.1} dB is healthy, light glue compression only", plr));
        } else {
            s.notes.push(format!("Dynamics: PLR {:.1} dB is already dense, compression skipped", plr));
        }
    }
    if allow(ModuleId::Unlimiter) && plr < 9.0 && a.lufs > -10.5 {
        s.unlimiter_amount = Some(((9.0 - plr) / 5.0).clamp(0.2, 0.8));
        s.notes.push(format!("Unlimiter: PLR {:.1} dB at {:.1} LUFS looks over-limited, restoring some peak dynamics before re-limiting", plr, a.lufs));
    }

    // --- stereo / low end
    if allow(ModuleId::BassControl) {
        s.bass_sub_mono = true;
        if a.sub_minus_bass_db > 6.0 {
            let g = -(a.sub_minus_bass_db - 4.0).min(4.0);
            s.bass_sub_gain_db = Some(g);
            s.notes.push(format!("Bass Control: sub region is {:.1} dB stronger than the bass region (per octave), {:+.1} dB on the sub", a.sub_minus_bass_db, g));
        } else if a.sub_minus_bass_db < -9.0 {
            let g = ((-9.0 - a.sub_minus_bass_db) * 0.5).min(3.0);
            s.bass_sub_gain_db = Some(g);
            s.notes.push(format!("Bass Control: sub region is {:.1} dB below the bass region (per octave), {:+.1} dB on the sub", a.sub_minus_bass_db, g));
        }
        s.notes.push("Bass Control: sub band summed to mono for translation".into());
    }
    if allow(ModuleId::Imager) {
        if a.correlation < 0.0 {
            s.imager_high_width = Some(0.85);
            s.notes.push(format!("Imager: long-term correlation {:+.2} is negative, narrowing the highs for mono compatibility", a.correlation));
        } else if a.width < min_width {
            s.imager_high_width = Some(1.25);
            s.notes.push(format!("Imager: width {:.2} is narrow for {}, widening the highs", a.width, o.genre.name()));
        }
    }
    if allow(ModuleId::Exciter) && tonal.tilt_db_oct < -1.3 {
        s.exciter_high_amount = Some(0.25);
        s.notes.push("Exciter: top end is darker than the target, adding gentle harmonics".into());
    }
    if allow(ModuleId::Stabilizer) && tonal.rms_dev > 4.5 {
        s.stabilizer = true;
        s.notes.push(format!("Stabilizer: tonal balance wanders {:.1} dB RMS from the target, enabling adaptive balance", tonal.rms_dev));
    }

    // --- loudness
    if allow(ModuleId::Maximizer) {
        let need = target_lufs - a.lufs - makeup;
        s.maximizer_gain_db = Some(need.clamp(0.0, 12.0));
        s.ceiling_db = Some(ceiling);
        s.limiter_mode = Some(limiter);
        if need <= 0.0 {
            s.notes.push(format!("Maximizer: programme ({:.1} LUFS) is already as loud as the target ({:.1}), no extra gain", a.lufs, target_lufs));
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

    fn analyse(l: &[f32], r: &[f32], sr: f32) -> Analysis {
        let mut an = Analyzer::new(sr);
        for (a, b) in l.chunks(480).zip(r.chunks(480)) {
            an.process(a, b);
        }
        an.result().expect("analysis")
    }

    #[test]
    fn bright_signal_gets_high_cut_and_gain() {
        let sr = 48000.0;
        let l = white(sr as usize * 6, 1, 0.1);
        let r = white(sr as usize * 6, 2, 0.1);
        let res = analyse(&l, &r, sr);
        let sug = suggest(&res, &AssistantOptions::default());
        assert!(sug.eq.iter().any(|e| e.freq > 2000.0 && e.gain_db < 0.0), "{:?}", sug.eq);
        assert!(sug.maximizer_gain_db.unwrap() > 0.0);
        assert!(!sug.notes.is_empty());
    }

    #[test]
    fn low_end_gaps_do_not_cause_bass_boosts() {
        // white noise high-passed at ~150 Hz (like a thin master): must never yield a low boost
        let sr = 48000.0;
        let mut hp = crate::biquad::ButterCascade::default();
        hp.design(true, sr, 150.0, 4);
        let raw = white(sr as usize * 6, 5, 0.2);
        let l: Vec<f32> = raw.iter().map(|x| hp.process(*x)).collect();
        let res = analyse(&l, &l, sr);
        let sug = suggest(&res, &AssistantOptions::default());
        assert!(!sug.eq.iter().any(|e| e.freq < 200.0 && e.gain_db > 0.0), "{:?}", sug.eq);
        assert!(sug.eq.iter().all(|e| e.gain_db.abs() <= 3.5));
    }

    #[test]
    fn silence_gives_no_suggestions() {
        let sr = 48000.0;
        let l = vec![0.0001f32; sr as usize * 6];
        let mut an = Analyzer::new(sr);
        for c in l.chunks(480) {
            an.process(c, c);
        }
        if let Some(res) = an.result() {
            let sug = suggest(&res, &AssistantOptions::default());
            assert!(sug.eq.is_empty() && sug.maximizer_gain_db.is_none());
        }
    }

    #[test]
    fn target_shapes_are_zero_mean() {
        for g in Genre::ALL {
            let s = g.target_shape();
            assert!((s.iter().sum::<f32>() / MATCH_BANDS as f32).abs() < 1e-3);
        }
    }
}

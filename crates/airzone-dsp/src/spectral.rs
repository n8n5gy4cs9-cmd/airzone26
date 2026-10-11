//! FFT-domain modules sharing one STFT pass: Stem EQ, Match EQ, Stabilizer, Spectral Shaper.

use realfft::num_complex::Complex32;

use crate::stft::{bin_freq, Stft, BINS, FFT_SIZE};
use crate::util::{db_to_lin, lin_to_db};

pub const MATCH_BANDS: usize = 64;
const HIST: usize = 9;

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn median<const N: usize>(v: &mut [f32; N]) -> f32 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    v[N / 2]
}

/// Map a bin to a fractional log-band position in `0..bands-1` (20 Hz .. 20 kHz).
fn band_pos(f: f32, bands: usize) -> f32 {
    let t = ((f.max(20.0) / 20.0).ln() / (1000.0f32).ln()).clamp(0.0, 1.0);
    t * (bands - 1) as f32
}

fn interp(curve: &[f32], pos: f32) -> f32 {
    let i = pos.floor() as usize;
    let f = pos - i as f32;
    let a = curve[i.min(curve.len() - 1)];
    let b = curve[(i + 1).min(curve.len() - 1)];
    a + (b - a) * f
}

// ---------------------------------------------------------------- Stem EQ

#[derive(Clone, Copy, Debug, Default)]
pub struct StemSettings {
    pub gain_db: f32,
    /// Positive brightens, negative darkens this stem.
    pub tilt_db: f32,
}

#[derive(Clone, Debug)]
pub struct StemEq {
    pub enabled: bool,
    /// 0 = bass, 1 = drums, 2 = vocals, 3 = other
    pub stems: [StemSettings; 4],
    /// Mask sharpness 1 (soft) .. 4 (hard).
    pub focus: f32,
    hist: Vec<Vec<f32>>,
    hist_pos: usize,
    prev: Vec<[f32; 4]>,
    /// Soloed stem for auditioning (None = off).
    pub solo: Option<usize>,
    /// Smoothed stem energy share for meters.
    pub meter: [f32; 4],
}

impl Default for StemEq {
    fn default() -> Self {
        Self {
            enabled: false,
            stems: [StemSettings::default(); 4],
            focus: 2.0,
            hist: vec![vec![0.0; BINS]; HIST],
            hist_pos: 0,
            prev: vec![[0.25; 4]; BINS],
            solo: None,
            meter: [0.25; 4],
        }
    }
}

impl StemEq {
    pub fn reset(&mut self) {
        self.hist.iter_mut().for_each(|h| h.iter_mut().for_each(|x| *x = 0.0));
        self.prev.iter_mut().for_each(|p| *p = [0.25; 4]);
    }

    fn apply(&mut self, sr: f32, l: &mut [Complex32], r: &mut [Complex32]) {
        let mut mm = vec![0.0f32; BINS];
        let mut ms = vec![0.0f32; BINS];
        for k in 0..BINS {
            mm[k] = ((l[k] + r[k]) * 0.5).norm();
            ms[k] = ((l[k] - r[k]) * 0.5).norm();
        }
        self.hist[self.hist_pos].copy_from_slice(&mm);
        self.hist_pos = (self.hist_pos + 1) % HIST;
        let p = self.focus.clamp(1.0, 4.0);
        let mut energy = [0.0f32; 4];
        for k in 0..BINS {
            let f = bin_freq(k, sr);
            let mut tv = [0.0f32; HIST];
            for (i, h) in self.hist.iter().enumerate() {
                tv[i] = h[k];
            }
            let hm = median(&mut tv);
            let mut fv = [0.0f32; 17];
            for (j, v) in fv.iter_mut().enumerate() {
                let idx = (k + j).saturating_sub(8).min(BINS - 1);
                *v = mm[idx];
            }
            let pm = median(&mut fv);
            let (h2, p2) = (hm * hm, pm * pm);
            let mh = h2 / (h2 + p2 + 1e-12);
            let mp = 1.0 - mh;
            let cen = mm[k] * mm[k] / (mm[k] * mm[k] + ms[k] * ms[k] + 1e-12);
            let low_w = 1.0 - smoothstep(110.0, 320.0, f);
            let voc_w = smoothstep(120.0, 400.0, f) * (1.0 - smoothstep(6000.0, 12000.0, f));
            let mut raw = [
                mh.powf(p) * low_w,
                mp.powf(p),
                mh.powf(p) * cen.powf(p) * voc_w * (1.0 - low_w),
                mh.powf(p) * (1.0 - cen * voc_w) * (1.0 - low_w) + 1e-4,
            ];
            let sum: f32 = raw.iter().sum::<f32>() + 1e-9;
            for (i, v) in raw.iter_mut().enumerate() {
                *v /= sum;
                *v = 0.5 * *v + 0.5 * self.prev[k][i];
                self.prev[k][i] = *v;
                energy[i] += *v * mm[k] * mm[k];
            }
            let octaves = ((f.max(40.0) / 1000.0).log2() / 3.0).clamp(-1.0, 1.0);
            let mut g = 0.0;
            for i in 0..4 {
                let db = self.stems[i].gain_db + self.stems[i].tilt_db * octaves;
                g += match self.solo {
                    Some(s) => {
                        if s == i {
                            raw[i]
                        } else {
                            0.0
                        }
                    }
                    None => raw[i] * db_to_lin(db),
                };
            }
            l[k] *= g;
            r[k] *= g;
        }
        let tot: f32 = energy.iter().sum::<f32>() + 1e-12;
        for i in 0..4 {
            self.meter[i] += (energy[i] / tot - self.meter[i]) * 0.2;
        }
    }
}

// ---------------------------------------------------------------- Match EQ

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Capture {
    #[default]
    Off,
    Source,
    Reference,
}

/// Averaged log-band spectrum accumulator.
#[derive(Clone, Debug)]
pub struct SpectrumProfile {
    pub power: [f32; MATCH_BANDS],
    pub frames: u32,
}

impl Default for SpectrumProfile {
    fn default() -> Self {
        Self { power: [0.0; MATCH_BANDS], frames: 0 }
    }
}

impl SpectrumProfile {
    pub fn accumulate(&mut self, sr: f32, l: &[Complex32], r: &[Complex32]) {
        let mut acc = [0.0f32; MATCH_BANDS];
        let mut cnt = [0.0f32; MATCH_BANDS];
        for k in 1..BINS {
            let f = bin_freq(k, sr);
            if !(20.0..=20000.0).contains(&f) {
                continue;
            }
            let b = band_pos(f, MATCH_BANDS).round() as usize;
            acc[b] += 0.5 * (l[k].norm_sqr() + r[k].norm_sqr());
            cnt[b] += 1.0;
        }
        for b in 0..MATCH_BANDS {
            if cnt[b] > 0.0 {
                self.power[b] += acc[b] / cnt[b];
            }
        }
        self.frames += 1;
    }
    pub fn is_valid(&self) -> bool {
        self.frames > 8
    }
    /// Average level per band in dB, normalised to zero mean (tonal shape only).
    pub fn shape_db(&self) -> [f32; MATCH_BANDS] {
        let mut out = [0.0f32; MATCH_BANDS];
        if self.frames == 0 {
            return out;
        }
        let mut mean = 0.0;
        for b in 0..MATCH_BANDS {
            out[b] = 10.0 * (self.power[b] / self.frames as f32 + 1e-14).log10();
            mean += out[b];
        }
        mean /= MATCH_BANDS as f32;
        out.iter_mut().for_each(|x| *x -= mean);
        out
    }
}

#[derive(Clone, Debug)]
pub struct MatchEq {
    pub enabled: bool,
    pub amount: f32,
    /// Smoothing in bands (1..16).
    pub smoothing: f32,
    pub max_gain_db: f32,
    pub capture: Capture,
    pub source: SpectrumProfile,
    pub reference: SpectrumProfile,
    /// Cached bin gains in dB, rebuilt when profiles change.
    curve: Vec<f32>,
    curve_dirty: bool,
}

impl Default for MatchEq {
    fn default() -> Self {
        Self {
            enabled: false,
            amount: 1.0,
            smoothing: 4.0,
            max_gain_db: 9.0,
            capture: Capture::Off,
            source: SpectrumProfile::default(),
            reference: SpectrumProfile::default(),
            curve: vec![0.0; MATCH_BANDS],
            curve_dirty: true,
        }
    }
}

impl MatchEq {
    pub fn clear(&mut self, which: Capture) {
        match which {
            Capture::Source => self.source = SpectrumProfile::default(),
            Capture::Reference => self.reference = SpectrumProfile::default(),
            Capture::Off => {}
        }
        self.curve_dirty = true;
    }

    pub fn set_reference(&mut self, p: SpectrumProfile) {
        self.reference = p;
        self.curve_dirty = true;
    }

    pub fn mark_dirty(&mut self) {
        self.curve_dirty = true;
    }

    /// The correction curve in dB per log band (for display).
    pub fn band_curve(&mut self) -> Vec<f32> {
        self.rebuild();
        self.curve.clone()
    }

    fn rebuild(&mut self) {
        if !self.curve_dirty {
            return;
        }
        self.curve_dirty = false;
        if !(self.source.is_valid() && self.reference.is_valid()) {
            self.curve.iter_mut().for_each(|x| *x = 0.0);
            return;
        }
        let s = self.source.shape_db();
        let r = self.reference.shape_db();
        let mut diff: Vec<f32> = (0..MATCH_BANDS).map(|b| r[b] - s[b]).collect();
        let w = self.smoothing.round().clamp(1.0, 16.0) as i32;
        let src = diff.clone();
        for b in 0..MATCH_BANDS as i32 {
            let mut acc = 0.0;
            let mut n = 0.0;
            for j in -w..=w {
                let idx = (b + j).clamp(0, MATCH_BANDS as i32 - 1) as usize;
                acc += src[idx];
                n += 1.0;
            }
            diff[b as usize] = acc / n;
        }
        let mean = diff.iter().sum::<f32>() / MATCH_BANDS as f32;
        self.curve = diff.iter().map(|d| (d - mean).clamp(-self.max_gain_db, self.max_gain_db)).collect();
    }

    fn apply(&mut self, sr: f32, l: &mut [Complex32], r: &mut [Complex32]) {
        match self.capture {
            Capture::Source => {
                self.source.accumulate(sr, l, r);
                self.curve_dirty = true;
            }
            Capture::Reference => {
                self.reference.accumulate(sr, l, r);
                self.curve_dirty = true;
            }
            Capture::Off => {}
        }
        if !self.enabled {
            return;
        }
        self.rebuild();
        for k in 1..BINS {
            let f = bin_freq(k, sr);
            let db = interp(&self.curve, band_pos(f, MATCH_BANDS)) * self.amount;
            let g = db_to_lin(db);
            l[k] *= g;
            r[k] *= g;
        }
    }
}

// ---------------------------------------------------------------- Stabilizer

pub const STAB_BANDS: usize = 32;

#[derive(Clone, Debug)]
pub struct Stabilizer {
    pub enabled: bool,
    pub amount: f32,
    pub range_db: f32,
    /// Target spectral slope in dB per octave (e.g. -3 pink-ish master).
    pub slope_db_oct: f32,
    /// Adaptation speed 0 (slow) .. 1 (fast).
    pub speed: f32,
    /// Use the Match EQ reference as the target shape when available.
    pub use_reference: bool,
    level: [f32; STAB_BANDS],
    gain_db: [f32; STAB_BANDS],
    /// For metering: current correction per band.
    pub meter_gain_db: [f32; STAB_BANDS],
}

impl Default for Stabilizer {
    fn default() -> Self {
        Self {
            enabled: false,
            amount: 0.5,
            range_db: 4.0,
            slope_db_oct: -4.5,
            speed: 0.3,
            use_reference: false,
            level: [-90.0; STAB_BANDS],
            gain_db: [0.0; STAB_BANDS],
            meter_gain_db: [0.0; STAB_BANDS],
        }
    }
}

impl Stabilizer {
    pub fn reset(&mut self) {
        self.level = [-90.0; STAB_BANDS];
        self.gain_db = [0.0; STAB_BANDS];
    }

    fn apply(&mut self, sr: f32, ref_shape: Option<[f32; MATCH_BANDS]>, l: &mut [Complex32], r: &mut [Complex32]) {
        let mut acc = [0.0f32; STAB_BANDS];
        let mut cnt = [0.0f32; STAB_BANDS];
        for k in 1..BINS {
            let f = bin_freq(k, sr);
            if !(30.0..=18000.0).contains(&f) {
                continue;
            }
            let b = band_pos(f, STAB_BANDS).round() as usize;
            acc[b] += 0.5 * (l[k].norm_sqr() + r[k].norm_sqr());
            cnt[b] += 1.0;
        }
        let alpha = 0.02 + 0.25 * self.speed;
        let mut cur = [0.0f32; STAB_BANDS];
        for b in 0..STAB_BANDS {
            let db = if cnt[b] > 0.0 { 10.0 * (acc[b] / cnt[b] + 1e-14).log10() } else { -120.0 };
            // slow when level rises, a bit slower when it falls
            self.level[b] += (db - self.level[b]) * alpha;
            cur[b] = self.level[b];
        }
        // target shape
        let mut tgt = [0.0f32; STAB_BANDS];
        for b in 0..STAB_BANDS {
            let f = 20.0 * 1000f32.powf(b as f32 / (STAB_BANDS - 1) as f32);
            tgt[b] = match (&ref_shape, self.use_reference) {
                (Some(s), true) => interp(s, band_pos(f, MATCH_BANDS)),
                _ => self.slope_db_oct * (f / 1000.0).log2(),
            };
        }
        // compare the shapes after removing the mean level difference (weighted over 100Hz-10kHz)
        let (mut m, mut n) = (0.0, 0.0);
        for b in 0..STAB_BANDS {
            let f = 20.0 * 1000f32.powf(b as f32 / (STAB_BANDS - 1) as f32);
            if (100.0..=10000.0).contains(&f) {
                m += cur[b] - tgt[b];
                n += 1.0;
            }
        }
        let off = if n > 0.0 { m / n } else { 0.0 };
        let mut want = [0.0f32; STAB_BANDS];
        for b in 0..STAB_BANDS {
            let silent = cur[b] < -100.0;
            want[b] = if silent { 0.0 } else { (tgt[b] + off - cur[b]).clamp(-self.range_db, self.range_db) * self.amount };
        }
        // smooth across bands
        for b in 0..STAB_BANDS {
            let a = want[b.saturating_sub(1)];
            let c = want[(b + 1).min(STAB_BANDS - 1)];
            let sm = 0.25 * a + 0.5 * want[b] + 0.25 * c;
            self.gain_db[b] += (sm - self.gain_db[b]) * 0.1;
        }
        self.meter_gain_db = self.gain_db;
        for k in 1..BINS {
            let f = bin_freq(k, sr);
            let g = db_to_lin(interp(&self.gain_db, band_pos(f, STAB_BANDS)));
            l[k] *= g;
            r[k] *= g;
        }
    }
}

// ---------------------------------------------------------------- Spectral Shaper

#[derive(Clone, Debug)]
pub struct SpectralShaper {
    pub enabled: bool,
    /// Resonance suppression depth 0..1.
    pub depth: f32,
    /// Threshold above local average in dB before suppression starts.
    pub threshold_db: f32,
    /// Selectivity: bins on each side used for the local average.
    pub width: f32,
    pub low_hz: f32,
    pub high_hz: f32,
    /// Spectral tilt in dB per octave around 1 kHz (post).
    pub tilt_db_oct: f32,
    /// Output only the removed signal.
    pub delta: bool,
    gains: Vec<f32>,
    pub meter_reduction_db: f32,
}

impl Default for SpectralShaper {
    fn default() -> Self {
        Self {
            enabled: false,
            depth: 0.5,
            threshold_db: 4.0,
            width: 12.0,
            low_hz: 200.0,
            high_hz: 14000.0,
            tilt_db_oct: 0.0,
            delta: false,
            gains: vec![1.0; BINS],
            meter_reduction_db: 0.0,
        }
    }
}

impl SpectralShaper {
    fn apply(&mut self, sr: f32, l: &mut [Complex32], r: &mut [Complex32]) {
        let mag_db: Vec<f32> = (0..BINS).map(|k| lin_to_db((l[k].norm_sqr() + r[k].norm_sqr()).sqrt() * 0.7071)).collect();
        // local average via prefix sums
        let mut prefix = vec![0.0f32; BINS + 1];
        for k in 0..BINS {
            prefix[k + 1] = prefix[k] + mag_db[k];
        }
        let w = self.width.round().clamp(3.0, 40.0) as usize;
        let mut total_red = 0.0;
        let mut count = 0.0;
        for k in 1..BINS {
            let f = bin_freq(k, sr);
            let lo = k.saturating_sub(w);
            let hi = (k + w).min(BINS - 1);
            let avg = (prefix[hi + 1] - prefix[lo]) / (hi - lo + 1) as f32;
            let excess = mag_db[k] - avg - self.threshold_db;
            let mut target = 1.0;
            if excess > 0.0 && f >= self.low_hz && f <= self.high_hz {
                let red = (excess * self.depth).min(18.0);
                target = db_to_lin(-red);
                total_red += red;
            }
            count += 1.0;
            // fast attack, slower release over frames
            let g = &mut self.gains[k];
            *g = if target < *g { target } else { *g + (target - *g) * 0.3 };
        }
        self.meter_reduction_db += (total_red / count - self.meter_reduction_db) * 0.2;
        for k in 1..BINS {
            let f = bin_freq(k, sr);
            let tilt = db_to_lin(self.tilt_db_oct * (f.max(30.0) / 1000.0).log2());
            if self.delta {
                let d = 1.0 - self.gains[k];
                l[k] *= d;
                r[k] *= d;
            } else {
                l[k] *= self.gains[k] * tilt;
                r[k] *= self.gains[k] * tilt;
            }
        }
    }
}

// ---------------------------------------------------------------- Tuning fix (full-song pitch correction)

const TUNE_HIST: usize = 100;

/// Global tuning correction for full mixes: finds how far the programme sits from the 12-TET grid
/// (or applies a manual offset) and moves the whole spectrum with a phase vocoder.
#[derive(Clone, Debug)]
pub struct TuningFix {
    pub enabled: bool,
    /// Detect the offset automatically; otherwise `manual_cents` is used.
    pub auto: bool,
    /// Pitch the song's A4 should end up at.
    pub target_a4: f32,
    pub manual_cents: f32,
    /// Strength of the automatic correction (0..1).
    pub amount: f32,
    prev_phase: [Vec<f32>; 2],
    synth_phase: [Vec<f32>; 2],
    have_prev: bool,
    hist: [f32; TUNE_HIST],
    pub detected_cents: f32,
    pub confidence: f32,
    pub applied_cents: f32,
    freq_bins: Vec<f32>,
    mag_in: Vec<f32>,
    mag_out: Vec<f32>,
    tf_out: Vec<f32>,
}

impl Default for TuningFix {
    fn default() -> Self {
        Self {
            enabled: false,
            auto: true,
            target_a4: 440.0,
            manual_cents: 0.0,
            amount: 1.0,
            prev_phase: [vec![0.0; BINS], vec![0.0; BINS]],
            synth_phase: [vec![0.0; BINS], vec![0.0; BINS]],
            have_prev: false,
            hist: [0.0; TUNE_HIST],
            detected_cents: 0.0,
            confidence: 0.0,
            applied_cents: 0.0,
            freq_bins: vec![0.0; BINS],
            mag_in: vec![0.0; BINS],
            mag_out: vec![0.0; BINS],
            tf_out: vec![0.0; BINS],
        }
    }
}

fn wrap_pi(x: f32) -> f32 {
    let t = std::f32::consts::TAU;
    x - t * (x / t).round()
}

impl TuningFix {
    pub fn reset(&mut self) {
        self.have_prev = false;
        self.hist = [0.0; TUNE_HIST];
        self.applied_cents = 0.0;
        self.confidence = 0.0;
        for c in 0..2 {
            self.prev_phase[c].iter_mut().for_each(|x| *x = 0.0);
            self.synth_phase[c].iter_mut().for_each(|x| *x = 0.0);
        }
    }

    fn apply(&mut self, sr: f32, l: &mut [Complex32], r: &mut [Complex32]) {
        use crate::stft::{FFT_SIZE, HOP};
        let two_pi = std::f32::consts::TAU;
        let hop = HOP as f32;
        let n = FFT_SIZE as f32;
        // instantaneous bin frequency (in bins) from the left channel phase advance
        for k in 0..BINS {
            let ph = l[k].arg();
            if self.have_prev {
                let expected = two_pi * k as f32 * hop / n;
                let d = wrap_pi(ph - self.prev_phase[0][k] - expected);
                self.freq_bins[k] = k as f32 + d * n / (two_pi * hop);
            } else {
                self.freq_bins[k] = k as f32;
            }
            self.mag_in[k] = ((l[k].norm_sqr() + r[k].norm_sqr()) * 0.5).sqrt();
        }
        // ---- tuning detection
        if self.have_prev {
            let maxm = self.mag_in.iter().cloned().fold(0.0f32, f32::max);
            for h in self.hist.iter_mut() {
                *h *= 0.998;
            }
            if maxm > 1.0e-5 {
                for k in 3..BINS - 3 {
                    let f = self.freq_bins[k] * sr / n;
                    if !(150.0..=3000.0).contains(&f) || (self.freq_bins[k] - k as f32).abs() > 0.7 {
                        continue;
                    }
                    let m = self.mag_in[k];
                    if m > 0.03 * maxm && m >= self.mag_in[k - 1] && m >= self.mag_in[k + 1] && m > self.mag_in[k - 2] && m > self.mag_in[k + 2] {
                        let dev = 12.0 * (f / 440.0).log2();
                        let frac = dev - dev.round();
                        let idx = (((frac * 100.0) + 50.0).floor() as isize).clamp(0, TUNE_HIST as isize - 1) as usize;
                        self.hist[idx] += m;
                    }
                }
            }
            // smoothed histogram peak
            let total: f32 = self.hist.iter().sum();
            if total > 1.0e-3 {
                let mut best = (0usize, 0.0f32);
                for i in 0..TUNE_HIST {
                    let mut acc = 0.0;
                    for j in -3i32..=3 {
                        acc += self.hist[(i as i32 + j).rem_euclid(TUNE_HIST as i32) as usize];
                    }
                    if acc > best.1 {
                        best = (i, acc);
                    }
                }
                self.detected_cents = best.0 as f32 - 49.5;
                self.confidence = (best.1 / total * (TUNE_HIST as f32 / 7.0) / 6.0).clamp(0.0, 1.0);
            }
        }
        // ---- desired shift
        let desired = if self.auto {
            if self.confidence > 0.35 {
                let want = 1200.0 * (self.target_a4 / 440.0).log2() - self.detected_cents;
                (want * self.amount).clamp(-100.0, 100.0)
            } else {
                0.0
            }
        } else {
            self.manual_cents.clamp(-100.0, 100.0)
        };
        let slew = if self.auto { 0.15 } else { 1.0 };
        let diff = desired - self.applied_cents;
        self.applied_cents += diff.clamp(-slew, slew);

        // ---- phase-vocoder shift
        let ratio = 2f32.powf(self.applied_cents / 1200.0);
        if !self.have_prev || (ratio - 1.0).abs() < 2.0e-4 {
            for (c, spec) in [&*l, &*r].iter().enumerate() {
                for k in 0..BINS {
                    let ph = spec[k].arg();
                    self.prev_phase[c][k] = ph;
                    self.synth_phase[c][k] = ph;
                }
            }
            self.have_prev = true;
            return;
        }
        for (c, spec) in [l, r].into_iter().enumerate() {
            // true frequency of every analysis bin from this channel's phase advance
            for k in 0..BINS {
                let ph = spec[k].arg();
                let expected = two_pi * k as f32 * hop / n;
                let d = wrap_pi(ph - self.prev_phase[c][k] - expected);
                self.freq_bins[k] = k as f32 + d * n / (two_pi * hop);
                self.mag_in[k] = spec[k].norm();
                self.prev_phase[c][k] = ph;
            }
            for k2 in 0..BINS {
                let src = k2 as f32 / ratio;
                let k0 = src.floor() as usize;
                if k0 + 1 >= BINS {
                    self.mag_out[k2] = 0.0;
                    self.tf_out[k2] = k2 as f32;
                    continue;
                }
                let f = src - k0 as f32;
                self.mag_out[k2] = self.mag_in[k0] * (1.0 - f) + self.mag_in[k0 + 1] * f;
                let kn = if f < 0.5 { k0 } else { k0 + 1 };
                self.tf_out[k2] = self.freq_bins[kn] * ratio;
            }
            for k2 in 0..BINS {
                self.synth_phase[c][k2] = wrap_pi(self.synth_phase[c][k2] + two_pi * hop * self.tf_out[k2] / n);
                spec[k2] = Complex32::from_polar(self.mag_out[k2], self.synth_phase[c][k2]);
            }
        }
    }
}

// ---------------------------------------------------------------- Hub

/// One FFT pass shared by all four spectral modules (so latency is constant).
pub struct SpectralHub {
    stft: Stft,
    pub stem: StemEq,
    pub match_eq: MatchEq,
    pub stabilizer: Stabilizer,
    pub shaper: SpectralShaper,
    pub tuning: TuningFix,
    sr: f32,
}

impl Default for SpectralHub {
    fn default() -> Self {
        Self {
            stft: Stft::new(),
            stem: StemEq::default(),
            match_eq: MatchEq::default(),
            stabilizer: Stabilizer::default(),
            shaper: SpectralShaper::default(),
            tuning: TuningFix::default(),
            sr: 44100.0,
        }
    }
}

impl SpectralHub {
    pub fn prepare(&mut self, sr: f32) {
        self.sr = sr;
        self.reset();
    }
    pub fn reset(&mut self) {
        self.stft.reset();
        self.stem.reset();
        self.stabilizer.reset();
        self.tuning.reset();
    }
    pub fn latency(&self) -> usize {
        FFT_SIZE
    }
    pub fn any_enabled(&self) -> bool {
        self.stem.enabled
            || self.match_eq.enabled
            || self.match_eq.capture != Capture::Off
            || self.stabilizer.enabled
            || self.shaper.enabled
            || self.tuning.enabled
    }

    pub fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        let sr = self.sr;
        let Self { stft, stem, match_eq, stabilizer, shaper, tuning, .. } = self;
        stft.process(l, r, |sl, sr_spec| {
            if match_eq.capture != Capture::Off || match_eq.enabled {
                match_eq.apply(sr, sl, sr_spec);
            }
            if stem.enabled {
                stem.apply(sr, sl, sr_spec);
            }
            if stabilizer.enabled {
                let shape = if match_eq.reference.is_valid() { Some(match_eq.reference.shape_db()) } else { None };
                stabilizer.apply(sr, shape, sl, sr_spec);
            }
            if shaper.enabled {
                shaper.apply(sr, sl, sr_spec);
            }
            if tuning.enabled {
                tuning.apply(sr, sl, sr_spec);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(hub: &mut SpectralHub, n: usize) -> (Vec<f32>, Vec<f32>) {
        let src: Vec<f32> = (0..n).map(|i| 0.3 * (i as f32 * 0.05).sin() + 0.1 * (i as f32 * 0.8).sin()).collect();
        let mut l = src.clone();
        let mut r = src.clone();
        for c in 0..(n / 512) {
            hub.process(&mut l[c * 512..(c + 1) * 512], &mut r[c * 512..(c + 1) * 512]);
        }
        (src, l)
    }

    #[test]
    fn disabled_hub_is_delay_only() {
        let mut hub = SpectralHub::default();
        hub.prepare(48000.0);
        let (src, out) = run(&mut hub, 16384);
        for i in (FFT_SIZE + 64)..16384 {
            assert!((out[i] - src[i - FFT_SIZE]).abs() < 1e-3);
        }
    }

    #[test]
    fn modules_stay_finite() {
        let mut hub = SpectralHub::default();
        hub.prepare(48000.0);
        hub.stem.enabled = true;
        hub.stem.stems[2].gain_db = 6.0;
        hub.stabilizer.enabled = true;
        hub.shaper.enabled = true;
        hub.match_eq.enabled = true;
        let (_s, out) = run(&mut hub, 32768);
        assert!(out.iter().all(|x| x.is_finite() && x.abs() < 4.0));
    }

    #[test]
    fn match_eq_learns_difference() {
        let mut hub = SpectralHub::default();
        hub.prepare(48000.0);
        hub.match_eq.capture = Capture::Source;
        let _ = run(&mut hub, 32768);
        hub.match_eq.capture = Capture::Reference;
        // brighter reference: high sine stronger
        let n = 32768;
        let mut l: Vec<f32> = (0..n).map(|i| 0.05 * (i as f32 * 0.05).sin() + 0.3 * (i as f32 * 0.8).sin()).collect();
        let mut r = l.clone();
        for c in 0..(n / 512) {
            hub.process(&mut l[c * 512..(c + 1) * 512], &mut r[c * 512..(c + 1) * 512]);
        }
        hub.match_eq.capture = Capture::Off;
        let curve = hub.match_eq.band_curve();
        let hi = curve[MATCH_BANDS - 20..].iter().cloned().fold(f32::MIN, f32::max);
        let lo = curve[..MATCH_BANDS / 2].iter().cloned().fold(f32::MAX, f32::min);
        assert!(hi > 1.0 || lo < -1.0, "hi {hi} lo {lo}");
    }

    #[test]
    fn tuning_fix_detects_and_corrects_detune() {
        let sr = 48000.0;
        // chord of A, C#, E partials, tuned 35 cents sharp of 440 grid
        let detune = 2f32.powf(35.0 / 1200.0);
        let freqs = [220.0 * detune, 277.18 * detune, 329.63 * detune, 440.0 * detune, 554.37 * detune, 659.25 * detune];
        let n = 48000 * 8;
        let src: Vec<f32> = (0..n)
            .map(|i| {
                let t = i as f32 / sr;
                freqs.iter().map(|f| 0.1 * (std::f32::consts::TAU * f * t).sin()).sum()
            })
            .collect();
        let mut hub = SpectralHub::default();
        hub.prepare(sr);
        hub.tuning.enabled = true;
        hub.tuning.auto = true;
        let mut l = src.clone();
        let mut r = src.clone();
        for c in 0..(n / 512) {
            hub.process(&mut l[c * 512..(c + 1) * 512], &mut r[c * 512..(c + 1) * 512]);
        }
        assert!((hub.tuning.detected_cents - 35.0).abs() < 4.0, "detected {}", hub.tuning.detected_cents);
        assert!((hub.tuning.applied_cents + 35.0).abs() < 6.0, "applied {}", hub.tuning.applied_cents);
        assert!(l.iter().all(|x| x.is_finite()));
    }

    #[test]
    fn tuning_fix_manual_shift_moves_a_sine() {
        let sr = 48000.0;
        let n = 48000 * 3;
        let f_in = 1000.0f32;
        let src: Vec<f32> = (0..n).map(|i| 0.4 * (std::f32::consts::TAU * f_in * i as f32 / sr).sin()).collect();
        let mut hub = SpectralHub::default();
        hub.prepare(sr);
        hub.tuning.enabled = true;
        hub.tuning.auto = false;
        hub.tuning.manual_cents = 50.0;
        let mut l = src.clone();
        let mut r = src.clone();
        for c in 0..(n / 512) {
            hub.process(&mut l[c * 512..(c + 1) * 512], &mut r[c * 512..(c + 1) * 512]);
        }
        // estimate output frequency by zero crossings in the last second
        let tail = &l[n - 24000..];
        let mut crossings = 0;
        for w in tail.windows(2) {
            if w[0] <= 0.0 && w[1] > 0.0 {
                crossings += 1;
            }
        }
        let f_out = crossings as f32 / (24000.0 / sr);
        let cents = 1200.0 * (f_out / f_in).log2();
        assert!((cents - 50.0).abs() < 8.0, "measured {cents} cents ({f_out} Hz)");
        let peak = tail.iter().fold(0.0f32, |a, b| a.max(b.abs()));
        assert!(peak > 0.25 && peak < 0.6, "peak {peak}");
    }
}

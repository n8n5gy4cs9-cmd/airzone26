//! Natural-sounding pitch correction.
//!
//! * [`PitchDetector`] – YIN fundamental-frequency tracker.
//! * [`Key`], [`ScaleKind`], [`KeyDetector`] – scale model and Krumhansl-Schmuckler key finding.
//! * [`PitchCorrector`] – monophonic corrector (vocals, solo instruments): a slow "centre" pitch is
//!   tracked so vibrato survives, a hysteretic note chooser picks the target, and the shift is
//!   applied by formant-preserving time-domain PSOLA.

use std::collections::VecDeque;

use crate::util::TAU;

// ------------------------------------------------------------------------------ YIN

const WIN_DEC: usize = 1024;
const HOP_DEC: usize = 128;

#[derive(Clone, Copy, Debug, Default)]
pub struct Estimate {
    pub f0: f32,
    pub confidence: f32,
}

/// Streaming YIN detector working at roughly 22 kHz internally.
#[derive(Clone, Debug)]
pub struct PitchDetector {
    sr: f32,
    dec: usize,
    sr_int: f32,
    acc: f32,
    acc_n: usize,
    ring: Vec<f32>,
    pos: usize,
    hop: usize,
    scratch: Vec<f32>,
    d: Vec<f32>,
    pub min_hz: f32,
    pub max_hz: f32,
    pub threshold: f32,
}

impl PitchDetector {
    pub fn new(sr: f32) -> Self {
        let dec = ((sr / 22050.0).round() as usize).max(1);
        Self {
            sr,
            dec,
            sr_int: sr / dec as f32,
            acc: 0.0,
            acc_n: 0,
            ring: vec![0.0; WIN_DEC],
            pos: 0,
            hop: 0,
            scratch: vec![0.0; WIN_DEC],
            d: vec![0.0; WIN_DEC / 2 + 1],
            min_hz: 50.0,
            max_hz: 1500.0,
            threshold: 0.2,
        }
    }

    pub fn dec(&self) -> usize {
        self.dec
    }
    /// Hop between estimates in full-rate samples.
    pub fn hop_samples(&self) -> usize {
        HOP_DEC * self.dec
    }
    /// Analysis window length in full-rate samples.
    pub fn window_samples(&self) -> usize {
        WIN_DEC * self.dec
    }
    pub fn sample_rate(&self) -> f32 {
        self.sr
    }

    pub fn reset(&mut self) {
        self.ring.iter_mut().for_each(|x| *x = 0.0);
        self.acc = 0.0;
        self.acc_n = 0;
        self.hop = 0;
    }

    /// Feed one full-rate sample; returns an estimate once per hop.
    pub fn push(&mut self, x: f32) -> Option<Estimate> {
        self.acc += x;
        self.acc_n += 1;
        if self.acc_n < self.dec {
            return None;
        }
        let v = self.acc / self.dec as f32;
        self.acc = 0.0;
        self.acc_n = 0;
        self.ring[self.pos] = v;
        self.pos = (self.pos + 1) % WIN_DEC;
        self.hop += 1;
        if self.hop >= HOP_DEC {
            self.hop = 0;
            Some(self.estimate())
        } else {
            None
        }
    }

    fn estimate(&mut self) -> Estimate {
        for k in 0..WIN_DEC {
            self.scratch[k] = self.ring[(self.pos + k) % WIN_DEC];
        }
        let x = &self.scratch;
        let energy: f32 = x.iter().map(|v| v * v).sum::<f32>() / WIN_DEC as f32;
        if energy < 1.0e-8 {
            return Estimate::default();
        }
        let tau_max = ((self.sr_int / self.min_hz) as usize).min(WIN_DEC / 2 - 1).max(4);
        let tau_min = ((self.sr_int / self.max_hz) as usize).max(2).min(tau_max - 1);
        let n = WIN_DEC - tau_max;
        for tau in 1..=tau_max {
            let mut s = 0.0f32;
            for j in 0..n {
                let diff = x[j] - x[j + tau];
                s += diff * diff;
            }
            self.d[tau] = s;
        }
        // cumulative mean normalised difference
        let mut cm = vec![1.0f32; tau_max + 2];
        let mut run = 0.0f32;
        for tau in 1..=tau_max {
            run += self.d[tau];
            cm[tau] = if run > 0.0 { self.d[tau] * tau as f32 / run } else { 1.0 };
        }
        let mut tau_est = 0usize;
        let mut tau = tau_min;
        while tau <= tau_max {
            if cm[tau] < self.threshold {
                while tau + 1 <= tau_max && cm[tau + 1] < cm[tau] {
                    tau += 1;
                }
                tau_est = tau;
                break;
            }
            tau += 1;
        }
        if tau_est == 0 {
            // no clear dip: take the global minimum with low confidence
            let (mut best, mut bv) = (tau_min, f32::MAX);
            for t in tau_min..=tau_max {
                if cm[t] < bv {
                    bv = cm[t];
                    best = t;
                }
            }
            if bv > 0.45 {
                return Estimate { f0: 0.0, confidence: (1.0 - bv).max(0.0) };
            }
            tau_est = best;
        }
        // parabolic interpolation
        let t = tau_est;
        let (a, b, c) = (cm[t.saturating_sub(1).max(1)], cm[t], cm[(t + 1).min(tau_max)]);
        let denom = a - 2.0 * b + c;
        let shift = if denom.abs() > 1e-9 && t > 1 && t < tau_max { 0.5 * (a - c) / denom } else { 0.0 };
        let period = t as f32 + shift.clamp(-1.0, 1.0);
        let f0 = self.sr_int / period;
        Estimate { f0, confidence: (1.0 - cm[t]).clamp(0.0, 1.0) }
    }
}

// ------------------------------------------------------------------------------ scales & keys

pub const NOTE_NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ScaleKind {
    #[default]
    Chromatic,
    Major,
    Minor,
    HarmonicMinor,
    MajorPentatonic,
    MinorPentatonic,
    Blues,
    Dorian,
    Mixolydian,
}

impl ScaleKind {
    pub const ALL: [ScaleKind; 9] = [
        ScaleKind::Chromatic,
        ScaleKind::Major,
        ScaleKind::Minor,
        ScaleKind::HarmonicMinor,
        ScaleKind::MajorPentatonic,
        ScaleKind::MinorPentatonic,
        ScaleKind::Blues,
        ScaleKind::Dorian,
        ScaleKind::Mixolydian,
    ];
    pub fn name(&self) -> &'static str {
        match self {
            ScaleKind::Chromatic => "Chromatic",
            ScaleKind::Major => "Major",
            ScaleKind::Minor => "Natural Minor",
            ScaleKind::HarmonicMinor => "Harmonic Minor",
            ScaleKind::MajorPentatonic => "Major Pentatonic",
            ScaleKind::MinorPentatonic => "Minor Pentatonic",
            ScaleKind::Blues => "Blues",
            ScaleKind::Dorian => "Dorian",
            ScaleKind::Mixolydian => "Mixolydian",
        }
    }
    pub fn intervals(&self) -> &'static [u8] {
        match self {
            ScaleKind::Chromatic => &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
            ScaleKind::Major => &[0, 2, 4, 5, 7, 9, 11],
            ScaleKind::Minor => &[0, 2, 3, 5, 7, 8, 10],
            ScaleKind::HarmonicMinor => &[0, 2, 3, 5, 7, 8, 11],
            ScaleKind::MajorPentatonic => &[0, 2, 4, 7, 9],
            ScaleKind::MinorPentatonic => &[0, 3, 5, 7, 10],
            ScaleKind::Blues => &[0, 3, 5, 6, 7, 10],
            ScaleKind::Dorian => &[0, 2, 3, 5, 7, 9, 10],
            ScaleKind::Mixolydian => &[0, 2, 4, 5, 7, 9, 10],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Key {
    /// 0 = C … 11 = B
    pub root: u8,
    pub scale: ScaleKind,
}

impl Key {
    pub fn allows(&self, midi_note: i32) -> bool {
        let rel = (midi_note - self.root as i32).rem_euclid(12) as u8;
        self.scale.intervals().contains(&rel)
    }
    /// Nearest allowed integer MIDI note to a fractional MIDI pitch.
    pub fn nearest(&self, midi: f32) -> i32 {
        let base = midi.round() as i32;
        let mut best = base;
        let mut bd = f32::MAX;
        for k in -7..=7 {
            let n = base + k;
            if self.allows(n) {
                let d = (n as f32 - midi).abs();
                if d < bd {
                    bd = d;
                    best = n;
                }
            }
        }
        best
    }
}

pub fn midi_name(m: i32) -> String {
    format!("{}{}", NOTE_NAMES[m.rem_euclid(12) as usize], m.div_euclid(12) - 1)
}

/// Krumhansl-Schmuckler key estimation from a pitch-class histogram.
#[derive(Clone, Debug, Default)]
pub struct KeyDetector {
    pub hist: [f32; 12],
}

const KS_MAJOR: [f32; 12] = [6.35, 2.23, 3.48, 2.33, 4.38, 4.09, 2.52, 5.19, 2.39, 3.66, 2.29, 2.88];
const KS_MINOR: [f32; 12] = [6.33, 2.68, 3.52, 5.38, 2.60, 3.53, 2.54, 4.75, 3.98, 2.69, 3.34, 3.17];

impl KeyDetector {
    pub fn clear(&mut self) {
        self.hist = [0.0; 12];
    }
    pub fn add(&mut self, midi: f32, weight: f32) {
        let pc = (midi.round() as i32).rem_euclid(12) as usize;
        self.hist[pc] += weight;
    }
    pub fn total(&self) -> f32 {
        self.hist.iter().sum()
    }
    /// Returns the best (key, correlation).
    pub fn best(&self) -> Option<(Key, f32)> {
        if self.total() < 1.0 {
            return None;
        }
        let corr = |profile: &[f32; 12], shift: usize| -> f32 {
            let hm = self.hist.iter().sum::<f32>() / 12.0;
            let pm = profile.iter().sum::<f32>() / 12.0;
            let (mut num, mut dh, mut dp) = (0.0, 0.0, 0.0);
            for i in 0..12 {
                let h = self.hist[(i + shift) % 12] - hm;
                let p = profile[i] - pm;
                num += h * p;
                dh += h * h;
                dp += p * p;
            }
            if dh * dp <= 0.0 {
                0.0
            } else {
                num / (dh * dp).sqrt()
            }
        };
        let mut best: Option<(Key, f32)> = None;
        for root in 0..12usize {
            for (profile, scale) in [(&KS_MAJOR, ScaleKind::Major), (&KS_MINOR, ScaleKind::Minor)] {
                let c = corr(profile, root);
                if best.map_or(true, |(_, b)| c > b) {
                    best = Some((Key { root: root as u8, scale }, c));
                }
            }
        }
        best
    }
}

// ------------------------------------------------------------------------------ corrector

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PitchMode {
    #[default]
    Vocal,
    Instrument,
}

#[derive(Clone, Copy, Debug)]
pub struct PitchSettings {
    pub mode: PitchMode,
    pub key: Key,
    /// Reference pitch of A4 in Hz.
    pub a4: f32,
    /// Time to reach a new target note; 0 = hard "robot" tuning, 60-150 ms = natural.
    pub retune_ms: f32,
    /// 0..1 overall correction amount.
    pub amount: f32,
    /// 0..1: how much of the natural vibrato / pitch movement is preserved.
    pub preserve: f32,
    /// Deviations smaller than this (cents) are left alone.
    pub tolerance_cents: f32,
    /// Largest correction applied (cents).
    pub max_shift_cents: f32,
    pub min_hz: f32,
    pub max_hz: f32,
    pub confidence: f32,
}

impl Default for PitchSettings {
    fn default() -> Self {
        Self {
            mode: PitchMode::Vocal,
            key: Key::default(),
            a4: 440.0,
            retune_ms: 90.0,
            amount: 1.0,
            preserve: 0.85,
            tolerance_cents: 8.0,
            max_shift_cents: 200.0,
            min_hz: 70.0,
            max_hz: 1000.0,
            confidence: 0.6,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct TrackEntry {
    time: u64,
    /// period in samples (0 when unvoiced)
    period: f32,
    shift_cents: f32,
}

#[derive(Clone, Copy, Debug)]
struct Mark {
    pos: i64,
    half: i32,
}

/// Values for the GUI.
#[derive(Clone, Copy, Debug, Default)]
pub struct PitchMeter {
    pub voiced: bool,
    pub midi: f32,
    pub target_note: i32,
    pub shift_cents: f32,
    pub f0: f32,
    /// Fraction (0..1, ~2 s smoothing) of the audio in which a usable pitch was found.
    pub voiced_ratio: f32,
}

const HN: usize = 1 << 16;
const MASK: usize = HN - 1;

pub struct PitchCorrector {
    pub settings: PitchSettings,
    pub meter: PitchMeter,
    pub key_hist: KeyDetector,
    sr: f32,
    det: PitchDetector,
    hist: [Vec<f32>; 3],
    out: [Vec<f32>; 2],
    wsum: Vec<f32>,
    n_in: i64,
    emit: i64,
    lat: i64,
    hopf: usize,
    hop_count: usize,
    track: VecDeque<TrackEntry>,
    marks: VecDeque<Mark>,
    ts: f64,
    // note logic state
    voiced_prev: bool,
    center: f32,
    note: i32,
    have_note: bool,
    shift: f32,
    med: [f32; 3],
    med_n: usize,
    last_f0: f32,
    last_conf: f32,
}

impl PitchCorrector {
    pub fn new(sr: f32) -> Self {
        let mut det = PitchDetector::new(sr);
        det.min_hz = 50.0;
        det.max_hz = 1500.0;
        let hopf = det.hop_samples();
        let pmax = (sr / 50.0) as i64;
        let lat = det.window_samples() as i64 / 2 + 2 * pmax + 2 * hopf as i64 + 64;
        Self {
            settings: PitchSettings::default(),
            meter: PitchMeter::default(),
            key_hist: KeyDetector::default(),
            sr,
            det,
            hist: [vec![0.0; HN], vec![0.0; HN], vec![0.0; HN]],
            out: [vec![0.0; HN], vec![0.0; HN]],
            wsum: vec![0.0; HN],
            n_in: 0,
            emit: -lat,
            lat,
            hopf,
            hop_count: 0,
            track: VecDeque::new(),
            marks: VecDeque::new(),
            ts: -1.0,
            voiced_prev: false,
            center: 0.0,
            note: 60,
            have_note: false,
            shift: 0.0,
            med: [0.0; 3],
            med_n: 0,
            last_f0: 0.0,
            last_conf: 0.0,
        }
    }

    pub fn prepare(&mut self, sr: f32) {
        let s = self.settings;
        *self = Self::new(sr);
        self.settings = s;
    }

    pub fn reset(&mut self) {
        let (sr, s) = (self.sr, self.settings);
        *self = Self::new(sr);
        self.settings = s;
    }

    pub fn latency(&self) -> usize {
        self.lat as usize
    }

    /// Process a stereo block in place.
    pub fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        self.det.min_hz = 50.0;
        for i in 0..l.len() {
            let idx = (self.n_in as usize) & MASK;
            self.hist[0][idx] = l[i];
            self.hist[1][idx] = r[i];
            let mono = 0.5 * (l[i] + r[i]);
            self.hist[2][idx] = mono;
            self.n_in += 1;
            if let Some(est) = self.det.push(mono) {
                self.last_f0 = est.f0;
                self.last_conf = est.confidence;
                self.analysis_hop(est);
            }
            // emit
            let (mut ol, mut or) = (0.0, 0.0);
            if self.emit >= 0 {
                let o = (self.emit as usize) & MASK;
                let w = self.wsum[o];
                if w > 1.0e-4 {
                    ol = self.out[0][o] / w;
                    or = self.out[1][o] / w;
                }
                self.out[0][o] = 0.0;
                self.out[1][o] = 0.0;
                self.wsum[o] = 0.0;
            }
            self.emit += 1;
            l[i] = ol;
            r[i] = or;
        }
    }

    fn rms_gate(&self) -> bool {
        true
    }

    fn analysis_hop(&mut self, est: Estimate) {
        let s = self.settings;
        let (lo, hi) = match s.mode {
            PitchMode::Vocal => (s.min_hz.max(60.0), s.max_hz.min(1400.0)),
            PitchMode::Instrument => (s.min_hz.max(50.0), s.max_hz.min(1500.0)),
        };
        let dt = self.hopf as f32 / self.sr;
        let voiced = self.rms_gate() && est.confidence >= s.confidence && est.f0 >= lo && est.f0 <= hi;
        let mut want = 0.0f32;
        if voiced {
            let midi = 69.0 + 12.0 * (est.f0 / s.a4).log2();
            // 3-point median to reject octave blips
            self.med = [self.med[1], self.med[2], midi];
            self.med_n = (self.med_n + 1).min(3);
            let m = if self.med_n >= 3 {
                let mut v = self.med;
                v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                v[1]
            } else {
                midi
            };
            if !self.voiced_prev || (m - self.center).abs() > 0.8 {
                self.center = m;
                if !self.voiced_prev {
                    self.have_note = false;
                }
            } else {
                let alpha = 1.0 - (-dt / 0.12).exp();
                self.center += (m - self.center) * alpha;
            }
            // hysteretic note choice
            let cand = s.key.nearest(self.center);
            if !self.have_note {
                self.note = cand;
                self.have_note = true;
            } else if cand != self.note {
                let dn = (self.center - self.note as f32).abs();
                let dc = (self.center - cand as f32).abs();
                if dn > dc + 0.2 {
                    self.note = cand;
                }
            }
            self.key_hist.add(self.center, est.confidence * dt);
            let err = (self.note as f32 - self.center) * 100.0;
            let tol = s.tolerance_cents.max(0.0);
            let err_c = if err.abs() <= tol { 0.0 } else { err - err.signum() * tol };
            let vib = (self.center - m) * 100.0;
            want = s.amount * (err_c + (1.0 - s.preserve) * vib);
            want = want.clamp(-s.max_shift_cents, s.max_shift_cents);
            self.meter.voiced = true;
            self.meter.midi = m;
            self.meter.target_note = self.note;
            self.meter.shift_cents = self.shift;
            self.meter.f0 = est.f0;
        } else {
            self.med_n = 0;
            self.meter.voiced = false;
        }
        // retune smoothing (fast fall-back to zero when unvoiced)
        let tau = if voiced { (s.retune_ms * 0.001).max(0.0) } else { 0.02 };
        let alpha = if tau <= 1.0e-4 { 1.0 } else { 1.0 - (-dt / tau).exp() };
        self.shift += (want - self.shift) * alpha;
        self.voiced_prev = voiced;
        self.meter.shift_cents = self.shift;
        self.meter.voiced_ratio += ((if voiced { 1.0 } else { 0.0 }) - self.meter.voiced_ratio) * (dt / 2.0).min(1.0);

        // time of this estimate = centre of the detector window
        let time = (self.n_in - self.det.window_samples() as i64 / 2).max(0) as u64;
        let period = if voiced { self.sr / est.f0 } else { 0.0 };
        self.track.push_back(TrackEntry { time, period, shift_cents: self.shift });

        self.gen_marks();
        self.synthesize();
        // drop old data
        let keep_from = (self.emit - 4 * (self.sr / 50.0) as i64).max(0) as u64;
        while self.track.len() > 2 && self.track[1].time < keep_from {
            self.track.pop_front();
        }
        while self.marks.len() > 2 && self.marks[1].pos < keep_from as i64 {
            self.marks.pop_front();
        }
    }

    fn track_at(&self, t: i64) -> TrackEntry {
        // last entry with time <= t
        let mut best = self.track.front().copied().unwrap_or_default();
        for e in self.track.iter() {
            if e.time as i64 <= t {
                best = *e;
            } else {
                break;
            }
        }
        best
    }

    #[inline]
    fn mono(&self, i: i64) -> f32 {
        self.hist[2][(i as usize) & MASK]
    }

    fn gen_marks(&mut self) {
        let limit = self.n_in - self.det.window_samples() as i64 / 2 - self.hopf as i64 - 8;
        let unvoiced_p = (self.sr * 0.006) as i64;
        loop {
            let (last_pos, last_half) = match self.marks.back() {
                Some(m) => (m.pos, m.half),
                None => {
                    // first mark
                    let p = unvoiced_p;
                    self.marks.push_back(Mark { pos: p, half: p as i32 });
                    continue;
                }
            };
            let e = self.track_at(last_pos);
            let voiced = e.period > 0.0;
            let p = if voiced { e.period.round() as i64 } else { unvoiced_p }.max(8);
            let nominal = last_pos + p;
            if nominal + p / 6 + p / 2 + 2 > limit || nominal > self.track.back().map_or(0, |t| t.time as i64) {
                break;
            }
            let mut pos = nominal;
            if voiced {
                // align with the previous grain by cross-correlation within +-P/6
                let d = (p / 6).max(1);
                let (mut bs, mut bp) = (f32::MIN, nominal);
                let half = (p / 2).max(2);
                for c in (nominal - d)..=(nominal + d) {
                    let mut s = 0.0f32;
                    let mut j = -half;
                    while j <= half {
                        s += self.mono(last_pos + j) * self.mono(c + j);
                        j += 2;
                    }
                    if s > bs {
                        bs = s;
                        bp = c;
                    }
                }
                pos = bp;
            }
            let _ = last_half;
            let spacing = (pos - last_pos).max(4);
            // the previous mark's half-width is its distance to this mark
            if let Some(prev) = self.marks.back_mut() {
                prev.half = spacing as i32;
            }
            self.marks.push_back(Mark { pos, half: spacing as i32 });
        }
    }

    fn synthesize(&mut self) {
        let m_lim = self.emit + self.hopf as i64;
        if self.marks.len() < 3 {
            return;
        }
        if self.ts < 0.0 {
            self.ts = self.marks[1].pos as f64;
        }
        loop {
            let ts = self.ts;
            let last = self.marks.back().unwrap();
            // need a finalised mark list covering ts (the very last mark has no known half-width yet)
            if ts >= (last.pos - last.half as i64 / 2) as f64 {
                break;
            }
            // nearest analysis mark (excluding the last)
            let n = self.marks.len() - 1;
            let mut k = 0usize;
            let mut bd = f64::MAX;
            for (i, m) in self.marks.iter().take(n).enumerate() {
                let d = (m.pos as f64 - ts).abs();
                if d < bd {
                    bd = d;
                    k = i;
                }
            }
            let mk = self.marks[k];
            let half = mk.half.max(4) as i64;
            if (ts as i64) - half >= m_lim {
                break;
            }
            let e = self.track_at(mk.pos);
            let ratio = 2f32.powf(e.shift_cents / 1200.0) as f64;
            // place the grain
            let tsi = ts.round() as i64;
            for j in -half..=half {
                let w = 0.5 + 0.5 * (std::f32::consts::PI * j as f32 / half as f32).cos();
                let oi = ((tsi + j) as usize) & MASK;
                let si = ((mk.pos + j) as usize) & MASK;
                self.out[0][oi] += self.hist[0][si] * w;
                self.out[1][oi] += self.hist[1][si] * w;
                self.wsum[oi] += w;
            }
            // advance by the (shifted) synthesis period = analysis period / ratio
            self.ts += (half as f64) / ratio;
        }
    }
}

#[allow(dead_code)]
fn unused() -> f32 {
    TAU
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Glottal-ish harmonic tone: sum of decaying harmonics (a periodic, voice-like waveform).
    fn voice(f0: f32, n: usize, sr: f32, start_phase: f32) -> Vec<f32> {
        (0..n)
            .map(|i| {
                let t = i as f32 / sr;
                let mut s = 0.0;
                for h in 1..=12 {
                    s += (TAU * f0 * h as f32 * t + start_phase * h as f32).sin() / h as f32;
                }
                0.3 * s
            })
            .collect()
    }

    fn detect(x: &[f32], sr: f32) -> f32 {
        let mut d = PitchDetector::new(sr);
        let mut last = 0.0;
        for &v in x {
            if let Some(e) = d.push(v) {
                if e.confidence > 0.8 {
                    last = e.f0;
                }
            }
        }
        last
    }

    fn cents(a: f32, b: f32) -> f32 {
        1200.0 * (a / b).log2()
    }

    #[test]
    fn yin_is_accurate() {
        for sr in [44100.0, 48000.0, 96000.0] {
            for f in [82.4f32, 110.0, 220.0, 440.0, 880.0] {
                let x = voice(f, sr as usize, sr, 0.0);
                let est = detect(&x, sr);
                assert!(cents(est, f).abs() < 6.0, "sr {sr} f {f} est {est} ({} cents)", cents(est, f));
            }
        }
    }

    #[test]
    fn yin_reports_unvoiced_for_noise() {
        let mut rng = crate::util::Rng(3);
        let x: Vec<f32> = (0..44100).map(|_| rng.next_f32() * 0.3).collect();
        let mut d = PitchDetector::new(44100.0);
        let mut max_conf = 0.0f32;
        for v in x {
            if let Some(e) = d.push(v) {
                max_conf = max_conf.max(e.confidence);
            }
        }
        assert!(max_conf < 0.8, "{max_conf}");
    }

    #[test]
    fn scale_nearest_and_key_detection() {
        let k = Key { root: 0, scale: ScaleKind::Major };
        assert_eq!(k.nearest(61.2), 62); // C# not allowed -> nearest D (62) vs C (60): 61.2 is closer to 62? |61.2-62|=0.8 vs 1.2
        assert!(k.allows(64) && !k.allows(63));
        let mut kd = KeyDetector::default();
        // C major scale emphasised on tonic/dominant
        for (n, w) in [(60, 5.0), (62, 2.0), (64, 3.0), (65, 2.0), (67, 4.0), (69, 2.0), (71, 1.5)] {
            kd.add(n as f32, w);
        }
        let (key, _) = kd.best().unwrap();
        assert_eq!(key.root, 0);
        assert_eq!(key.scale, ScaleKind::Major);
        assert_eq!(midi_name(69), "A4");
    }

    fn run(pc: &mut PitchCorrector, x: &[f32]) -> Vec<f32> {
        let mut l = x.to_vec();
        let mut r = x.to_vec();
        for c in 0..(x.len() / 512) {
            pc.process(&mut l[c * 512..(c + 1) * 512], &mut r[c * 512..(c + 1) * 512]);
        }
        l
    }

    #[test]
    fn corrects_flat_note_to_pitch() {
        let sr = 44100.0;
        // A4 played 40 cents flat, chromatic scale -> should be pulled to ~440 Hz
        let f_in = 440.0 * 2f32.powf(-40.0 / 1200.0);
        let n = (sr * 3.0) as usize;
        let x = voice(f_in, n / 512 * 512, sr, 0.0);
        let mut pc = PitchCorrector::new(sr);
        pc.settings.retune_ms = 20.0;
        pc.settings.tolerance_cents = 0.0;
        let y = run(&mut pc, &x);
        let tail = &y[y.len() - 16384..];
        let est = detect(tail, sr);
        assert!(cents(est, 440.0).abs() < 12.0, "est {est} ({} cents from 440)", cents(est, 440.0));
        // level preserved
        let rms = |v: &[f32]| (v.iter().map(|a| a * a).sum::<f32>() / v.len() as f32).sqrt();
        let ratio = rms(tail) / rms(&x[x.len() - 16384..]);
        assert!(ratio > 0.8 && ratio < 1.25, "level ratio {ratio}");
    }

    #[test]
    fn in_tune_note_is_left_alone() {
        let sr = 48000.0;
        let x = voice(220.0, 48000 * 2, sr, 0.0);
        let mut pc = PitchCorrector::new(sr);
        let y = run(&mut pc, &x);
        let tail = &y[y.len() - 16384..];
        let est = detect(tail, sr);
        assert!(cents(est, 220.0).abs() < 8.0, "{est}");
    }

    #[test]
    fn keeps_vibrato_when_preserving() {
        let sr = 44100.0;
        let n = (sr * 4.0) as usize / 512 * 512;
        // A4 + 25 cents sharp with +-35 cent vibrato at 5.5 Hz
        let mut phase = 0.0f32;
        let x: Vec<f32> = (0..n)
            .map(|i| {
                let t = i as f32 / sr;
                let cents_dev = 25.0 + 35.0 * (TAU * 5.5 * t).sin();
                let f = 440.0 * 2f32.powf(cents_dev / 1200.0);
                phase += TAU * f / sr;
                let mut s = 0.0;
                for h in 1..=10 {
                    s += (phase * h as f32).sin() / h as f32;
                }
                0.3 * s
            })
            .collect();
        let mut pc = PitchCorrector::new(sr);
        pc.settings.preserve = 1.0;
        pc.settings.retune_ms = 60.0;
        let y = run(&mut pc, &x);
        // measure pitch track spread of the tail
        let mut d = PitchDetector::new(sr);
        let mut vals = vec![];
        for (i, &v) in y.iter().enumerate() {
            if let Some(e) = d.push(v) {
                if i > y.len() - 44100 && e.confidence > 0.8 {
                    vals.push(cents(e.f0, 440.0));
                }
            }
        }
        assert!(vals.len() > 20);
        let mean = vals.iter().sum::<f32>() / vals.len() as f32;
        let spread = vals.iter().map(|v| (v - mean).abs()).fold(0.0, f32::max);
        assert!(spread > 20.0, "vibrato flattened: spread {spread}");
        assert!(mean.abs() < 15.0, "centre not corrected: mean {mean}");
    }

    #[test]
    fn in_tune_passthrough_matches_delayed_input() {
        let sr = 44100.0;
        let x = voice(196.0, 44100 * 2 / 512 * 512, sr, 0.3);
        let mut pc = PitchCorrector::new(sr);
        let lat = pc.latency();
        let y = run(&mut pc, &x);
        let a = &y[30000..40000];
        let b = &x[30000 - lat..40000 - lat];
        let num: f32 = a.iter().zip(b).map(|(p, q)| p * q).sum();
        let den = (a.iter().map(|p| p * p).sum::<f32>() * b.iter().map(|q| q * q).sum::<f32>()).sqrt();
        assert!(num / den > 0.9, "correlation {}", num / den);
    }

    #[test]
    fn melody_with_gaps_and_noise_is_stable() {
        let sr = 48000.0;
        let mut rng = crate::util::Rng(11);
        let mut x: Vec<f32> = Vec::new();
        for (f, cents_off) in [(261.6f32, 30.0f32), (293.7, -35.0), (329.6, 20.0), (392.0, -45.0)] {
            let ff = f * 2f32.powf(cents_off / 1200.0);
            x.extend(voice(ff, 24000, sr, 0.0));
            x.extend((0..4800).map(|_| rng.next_f32() * 0.02)); // breath noise gap
        }
        let n = x.len() / 512 * 512;
        x.truncate(n);
        let mut pc = PitchCorrector::new(sr);
        pc.settings.key = Key { root: 0, scale: ScaleKind::Major };
        let y = run(&mut pc, &x);
        assert!(y.iter().all(|v| v.is_finite() && v.abs() < 2.0));
        let peak_in = x.iter().fold(0.0f32, |a, b| a.max(b.abs()));
        let peak_out = y.iter().fold(0.0f32, |a, b| a.max(b.abs()));
        assert!(peak_out < peak_in * 1.4, "{peak_out} vs {peak_in}");
    }
}

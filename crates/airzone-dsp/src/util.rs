//! Small shared helpers: unit conversion, smoothing, delay lines.

pub const DENORM: f32 = 1.0e-20;
pub const PI: f32 = std::f32::consts::PI;
pub const TAU: f32 = std::f32::consts::TAU;

#[inline]
pub fn db_to_lin(db: f32) -> f32 {
    10f32.powf(db * 0.05)
}

#[inline]
pub fn lin_to_db(x: f32) -> f32 {
    20.0 * x.max(1.0e-12).log10()
}

/// One-pole smoothing coefficient for a time constant in milliseconds.
#[inline]
pub fn time_coeff(ms: f32, sr: f32) -> f32 {
    if ms <= 0.0 {
        0.0
    } else {
        (-1.0 / (ms * 0.001 * sr)).exp()
    }
}

#[inline]
pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Frequency of bin `i` of `n` log-spaced points between `lo` and `hi`.
pub fn log_freq(i: usize, n: usize, lo: f32, hi: f32) -> f32 {
    let t = i as f32 / (n.max(2) - 1) as f32;
    lo * (hi / lo).powf(t)
}

/// A linear parameter smoother (one-pole).
#[derive(Clone, Copy, Debug)]
pub struct Smoother {
    pub current: f32,
    pub target: f32,
    coeff: f32,
}

impl Smoother {
    pub fn new(value: f32) -> Self {
        Self { current: value, target: value, coeff: 0.0 }
    }
    pub fn set_time(&mut self, ms: f32, sr: f32) {
        self.coeff = time_coeff(ms, sr);
    }
    pub fn set_target(&mut self, t: f32) {
        self.target = t;
    }
    pub fn snap(&mut self, v: f32) {
        self.current = v;
        self.target = v;
    }
    #[inline]
    pub fn next(&mut self) -> f32 {
        self.current = self.target + (self.current - self.target) * self.coeff;
        self.current
    }
}

/// Integer-sample delay line with a power-of-two buffer.
#[derive(Clone, Debug)]
pub struct DelayLine {
    buf: Vec<f32>,
    mask: usize,
    pos: usize,
}

impl DelayLine {
    pub fn new(max_samples: usize) -> Self {
        let size = (max_samples + 2).next_power_of_two();
        Self { buf: vec![0.0; size], mask: size - 1, pos: 0 }
    }
    pub fn clear(&mut self) {
        self.buf.iter_mut().for_each(|x| *x = 0.0);
    }
    #[inline]
    pub fn push(&mut self, x: f32) {
        self.pos = (self.pos + 1) & self.mask;
        self.buf[self.pos] = x;
    }
    /// Sample written `d` pushes ago (`d == 0` is the newest).
    #[inline]
    pub fn tap(&self, d: usize) -> f32 {
        self.buf[(self.pos.wrapping_sub(d)) & self.mask]
    }
    /// Fractional tap using linear interpolation.
    #[inline]
    pub fn tap_frac(&self, d: f32) -> f32 {
        let i = d.floor();
        let f = d - i;
        let i = i as usize;
        let a = self.tap(i);
        let b = self.tap(i + 1);
        a + (b - a) * f
    }
    pub fn capacity(&self) -> usize {
        self.buf.len() - 2
    }
}

/// Peak/RMS-style envelope follower with separate attack and release.
#[derive(Clone, Copy, Debug, Default)]
pub struct Envelope {
    pub value: f32,
    att: f32,
    rel: f32,
}

impl Envelope {
    pub fn new(attack_ms: f32, release_ms: f32, sr: f32) -> Self {
        Self { value: 0.0, att: time_coeff(attack_ms, sr), rel: time_coeff(release_ms, sr) }
    }
    pub fn set(&mut self, attack_ms: f32, release_ms: f32, sr: f32) {
        self.att = time_coeff(attack_ms, sr);
        self.rel = time_coeff(release_ms, sr);
    }
    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let c = if x > self.value { self.att } else { self.rel };
        self.value = x + (self.value - x) * c;
        self.value
    }
}

/// Soft sign-preserving clipper (cubic) saturating at +-1.
#[inline]
pub fn soft_clip(x: f32) -> f32 {
    let x = x.clamp(-1.5, 1.5);
    x - x * x * x * (4.0 / 27.0)
}

/// Deterministic xorshift noise for dither / noise sources.
#[derive(Clone, Debug)]
pub struct Rng(pub u32);

impl Rng {
    #[inline]
    pub fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }
    /// Uniform in [-1, 1).
    #[inline]
    pub fn next_f32(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 * (2.0 / 16777216.0) - 1.0
    }
}

/// Convert a linear 0..1 position to a log frequency in [lo, hi].
pub fn pos_to_freq(t: f32, lo: f32, hi: f32) -> f32 {
    lo * (hi / lo).powf(t.clamp(0.0, 1.0))
}

pub fn freq_to_pos(f: f32, lo: f32, hi: f32) -> f32 {
    ((f.max(lo) / lo).ln() / (hi / lo).ln()).clamp(0.0, 1.0)
}

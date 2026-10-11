//! 8-band parametric equaliser (per-band stereo/mid/side/left/right placement)
//! and a 4-band dynamic EQ.

use crate::biquad::{Biquad, ButterCascade, Coeffs, FilterKind};
use crate::util::{db_to_lin, lin_to_db, time_coeff};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Placement {
    #[default]
    Stereo,
    Mid,
    Side,
    Left,
    Right,
}

#[derive(Clone, Copy, Debug)]
pub struct EqBand {
    pub enabled: bool,
    pub kind: FilterKind,
    pub freq: f32,
    pub gain_db: f32,
    pub q: f32,
    /// Butterworth order for high/low pass (2,4,6,8).
    pub order: usize,
    pub placement: Placement,
}

impl Default for EqBand {
    fn default() -> Self {
        Self {
            enabled: false,
            kind: FilterKind::Bell,
            freq: 1000.0,
            gain_db: 0.0,
            q: 1.0,
            order: 2,
            placement: Placement::Stereo,
        }
    }
}

impl EqBand {
    /// Magnitude in dB at `f` (used for response curves in the GUI).
    pub fn magnitude_db(&self, sr: f32, f: f32) -> f32 {
        if !self.enabled {
            return 0.0;
        }
        match self.kind {
            FilterKind::HighPass | FilterKind::LowPass => {
                let mut c = ButterCascade::default();
                c.design(self.kind == FilterKind::HighPass, sr, self.freq, self.order);
                lin_to_db(c.magnitude(sr, f))
            }
            _ => lin_to_db(Coeffs::design(self.kind, sr, self.freq, self.q, self.gain_db).magnitude(sr, f)),
        }
    }
}

pub const EQ_BANDS: usize = 8;

#[derive(Clone, Debug)]
struct BandState {
    last: Option<EqBand>,
    sections: [[Biquad; 4]; 2],
    n: usize,
}

impl Default for BandState {
    fn default() -> Self {
        Self { last: None, sections: [[Biquad::default(); 4]; 2], n: 1 }
    }
}

#[derive(Clone, Debug)]
pub struct ParametricEq {
    pub bands: [EqBand; EQ_BANDS],
    states: Vec<BandState>,
    sr: f32,
}

impl Default for ParametricEq {
    fn default() -> Self {
        Self { bands: [EqBand::default(); EQ_BANDS], states: vec![BandState::default(); EQ_BANDS], sr: 44100.0 }
    }
}

impl ParametricEq {
    pub fn prepare(&mut self, sr: f32) {
        self.sr = sr;
        self.reset();
    }

    pub fn reset(&mut self) {
        for st in &mut self.states {
            st.last = None;
            for ch in &mut st.sections {
                ch.iter_mut().for_each(|b| b.reset());
            }
        }
    }

    fn update(&mut self, i: usize) {
        let b = self.bands[i];
        let st = &mut self.states[i];
        let changed = match &st.last {
            None => true,
            Some(l) => {
                l.kind != b.kind
                    || l.freq != b.freq
                    || l.gain_db != b.gain_db
                    || l.q != b.q
                    || l.order != b.order
            }
        };
        if changed {
            match b.kind {
                FilterKind::HighPass | FilterKind::LowPass => {
                    let mut c = ButterCascade::default();
                    c.design(b.kind == FilterKind::HighPass, self.sr, b.freq, b.order);
                    st.n = c.n;
                    for ch in 0..2 {
                        for k in 0..c.n {
                            st.sections[ch][k].set(c.stages[k].c);
                        }
                    }
                }
                _ => {
                    let c = Coeffs::design(b.kind, self.sr, b.freq, b.q, b.gain_db);
                    st.n = 1;
                    for ch in 0..2 {
                        st.sections[ch][0].set(c);
                    }
                }
            }
        }
        st.last = Some(b);
    }

    pub fn latency(&self) -> usize {
        0
    }

    pub fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        for i in 0..EQ_BANDS {
            if !self.bands[i].enabled {
                continue;
            }
            self.update(i);
            let place = self.bands[i].placement;
            let st = &mut self.states[i];
            let n = st.n;
            for (xl, xr) in l.iter_mut().zip(r.iter_mut()) {
                match place {
                    Placement::Stereo => {
                        let (mut a, mut b) = (*xl, *xr);
                        for k in 0..n {
                            a = st.sections[0][k].process(a);
                            b = st.sections[1][k].process(b);
                        }
                        *xl = a;
                        *xr = b;
                    }
                    Placement::Left => {
                        let mut a = *xl;
                        for k in 0..n {
                            a = st.sections[0][k].process(a);
                        }
                        *xl = a;
                    }
                    Placement::Right => {
                        let mut b = *xr;
                        for k in 0..n {
                            b = st.sections[1][k].process(b);
                        }
                        *xr = b;
                    }
                    Placement::Mid => {
                        let m = 0.5 * (*xl + *xr);
                        let mut p = m;
                        for k in 0..n {
                            p = st.sections[0][k].process(p);
                        }
                        let d = p - m;
                        *xl += d;
                        *xr += d;
                    }
                    Placement::Side => {
                        let s = 0.5 * (*xl - *xr);
                        let mut p = s;
                        for k in 0..n {
                            p = st.sections[0][k].process(p);
                        }
                        let d = p - s;
                        *xl += d;
                        *xr -= d;
                    }
                }
            }
        }
    }

    /// Combined response in dB.
    pub fn response_db(&self, f: f32) -> f32 {
        self.bands.iter().map(|b| b.magnitude_db(self.sr, f)).sum()
    }
}

// ---------------------------------------------------------------- Dynamic EQ

pub const DYN_BANDS: usize = 4;

#[derive(Clone, Copy, Debug)]
pub struct DynBand {
    pub enabled: bool,
    pub freq: f32,
    pub q: f32,
    /// Static gain applied regardless of the signal level.
    pub gain_db: f32,
    pub threshold_db: f32,
    /// Maximum dynamic gain change (positive = expand/boost above threshold,
    /// negative = compress/cut above threshold).
    pub range_db: f32,
    pub ratio: f32,
    pub attack_ms: f32,
    pub release_ms: f32,
}

impl Default for DynBand {
    fn default() -> Self {
        Self {
            enabled: false,
            freq: 1000.0,
            q: 1.0,
            gain_db: 0.0,
            threshold_db: -24.0,
            range_db: -6.0,
            ratio: 3.0,
            attack_ms: 10.0,
            release_ms: 120.0,
        }
    }
}

#[derive(Clone, Debug, Default)]
struct DynState {
    det: [Biquad; 2],
    filt: [Biquad; 2],
    env: f32,
    gain_smooth: f32,
}

#[derive(Clone, Debug)]
pub struct DynamicEq {
    pub bands: [DynBand; DYN_BANDS],
    states: Vec<DynState>,
    sr: f32,
    /// Current dynamic gain per band for metering.
    pub gain_reduction: [f32; DYN_BANDS],
}

impl Default for DynamicEq {
    fn default() -> Self {
        Self {
            bands: [DynBand::default(); DYN_BANDS],
            states: vec![DynState::default(); DYN_BANDS],
            sr: 44100.0,
            gain_reduction: [0.0; DYN_BANDS],
        }
    }
}

const DYN_SUBBLOCK: usize = 16;

impl DynamicEq {
    pub fn prepare(&mut self, sr: f32) {
        self.sr = sr;
        self.reset();
    }
    pub fn reset(&mut self) {
        for s in &mut self.states {
            *s = DynState::default();
        }
    }
    pub fn latency(&self) -> usize {
        0
    }

    pub fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        let sr = self.sr;
        for bi in 0..DYN_BANDS {
            let b = self.bands[bi];
            if !b.enabled {
                self.gain_reduction[bi] = 0.0;
                continue;
            }
            let st = &mut self.states[bi];
            let det_c = Coeffs::design(FilterKind::BandPass, sr, b.freq, b.q.max(0.3), 0.0);
            st.det[0].set(det_c);
            st.det[1].set(det_c);
            let att = time_coeff(b.attack_ms, sr);
            let rel = time_coeff(b.release_ms, sr);
            let n = l.len();
            let mut start = 0;
            while start < n {
                let end = (start + DYN_SUBBLOCK).min(n);
                // detector over the sub-block, then one coefficient update
                for i in start..end {
                    let d = st.det[0].process(l[i]).abs().max(st.det[1].process(r[i]).abs());
                    let c = if d > st.env { att } else { rel };
                    st.env = d + (st.env - d) * c;
                }
                let level_db = lin_to_db(st.env * 1.4142);
                let over = level_db - b.threshold_db;
                let mut dynamic = 0.0;
                if over > 0.0 {
                    let amt = over * (1.0 - 1.0 / b.ratio.max(1.0));
                    dynamic = if b.range_db < 0.0 { -amt.min(-b.range_db) } else { amt.min(b.range_db) };
                }
                st.gain_smooth += (dynamic - st.gain_smooth) * 0.35;
                let total = b.gain_db + st.gain_smooth;
                self.gain_reduction[bi] = st.gain_smooth;
                let c = Coeffs::design(FilterKind::Bell, sr, b.freq, b.q, total);
                st.filt[0].set(c);
                st.filt[1].set(c);
                for i in start..end {
                    l[i] = st.filt[0].process(l[i]);
                    r[i] = st.filt[1].process(r[i]);
                }
                start = end;
            }
        }
        let _ = db_to_lin(0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bell_boost_matches_response() {
        let c = Coeffs::design(FilterKind::Bell, 48000.0, 1000.0, 1.0, 6.0);
        let db = lin_to_db(c.magnitude(48000.0, 1000.0));
        assert!((db - 6.0).abs() < 0.05, "{db}");
    }

    #[test]
    fn flat_eq_is_transparent() {
        let mut eq = ParametricEq::default();
        eq.prepare(48000.0);
        let mut l: Vec<f32> = (0..256).map(|i| (i as f32 * 0.1).sin()).collect();
        let mut r = l.clone();
        let orig = l.clone();
        eq.process(&mut l, &mut r);
        assert_eq!(orig, l);
    }

    #[test]
    fn highpass_attenuates_low() {
        let mut b = EqBand::default();
        b.enabled = true;
        b.kind = FilterKind::HighPass;
        b.freq = 1000.0;
        b.order = 4;
        assert!(b.magnitude_db(48000.0, 100.0) < -35.0);
        assert!(b.magnitude_db(48000.0, 4000.0).abs() < 0.5);
    }
}

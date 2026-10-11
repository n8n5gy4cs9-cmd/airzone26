//! The mastering chain: ordering, bypass, latency bookkeeping and metering.

use crate::eq::{DynamicEq, ParametricEq};
use crate::fx::{Reverb, StereoDelay};
use crate::dynamics::MultibandDynamics;
use crate::maximizer::{Dither, Maximizer};
use crate::meter::{LoudnessMeter, SpectrumAnalyzer, StereoMeter};
use crate::saturation::Exciter;
use crate::spectral::{Capture, SpectralHub};
use crate::stereo::{BassControl, Imager};
use crate::transient::{Impact, LowEndFocus};
use crate::unlimiter::Unlimiter;
use crate::util::{db_to_lin, DelayLine};
use crate::vintage::{VintageComp, VintageEq, VintageLimiter, VintageTape};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ModuleId {
    Unlimiter,
    StemEq,
    MatchEq,
    Equalizer,
    DynamicEq,
    Stabilizer,
    SpectralShaper,
    BassControl,
    LowEndFocus,
    Dynamics,
    VintageComp,
    VintageEq,
    VintageTape,
    Exciter,
    Impact,
    Imager,
    Delay,
    Reverb,
    VintageLimiter,
    Maximizer,
}

pub const MODULE_COUNT: usize = 20;

impl ModuleId {
    pub const ALL: [ModuleId; MODULE_COUNT] = [
        ModuleId::Unlimiter,
        ModuleId::StemEq,
        ModuleId::MatchEq,
        ModuleId::Equalizer,
        ModuleId::DynamicEq,
        ModuleId::Stabilizer,
        ModuleId::SpectralShaper,
        ModuleId::BassControl,
        ModuleId::LowEndFocus,
        ModuleId::Dynamics,
        ModuleId::VintageComp,
        ModuleId::VintageEq,
        ModuleId::VintageTape,
        ModuleId::Exciter,
        ModuleId::Impact,
        ModuleId::Imager,
        ModuleId::Delay,
        ModuleId::Reverb,
        ModuleId::VintageLimiter,
        ModuleId::Maximizer,
    ];

    pub fn index(self) -> usize {
        self as usize
    }
    pub fn from_index(i: usize) -> Option<ModuleId> {
        Self::ALL.get(i).copied()
    }
    pub fn name(self) -> &'static str {
        match self {
            ModuleId::Unlimiter => "Unlimiter",
            ModuleId::StemEq => "Stem EQ",
            ModuleId::MatchEq => "Match EQ",
            ModuleId::Equalizer => "Equalizer",
            ModuleId::DynamicEq => "Dynamic EQ",
            ModuleId::Stabilizer => "Stabilizer",
            ModuleId::SpectralShaper => "Spectral Shaper",
            ModuleId::BassControl => "Bass Control",
            ModuleId::LowEndFocus => "Low End Focus",
            ModuleId::Dynamics => "Dynamics",
            ModuleId::VintageComp => "Vintage Comp",
            ModuleId::VintageEq => "Vintage EQ",
            ModuleId::VintageTape => "Vintage Tape",
            ModuleId::Exciter => "Exciter",
            ModuleId::Impact => "Impact",
            ModuleId::Imager => "Imager",
            ModuleId::Delay => "Delay",
            ModuleId::Reverb => "Reverb",
            ModuleId::VintageLimiter => "Vintage Limiter",
            ModuleId::Maximizer => "Maximizer",
        }
    }
    pub fn is_spectral(self) -> bool {
        matches!(self, ModuleId::StemEq | ModuleId::MatchEq | ModuleId::Stabilizer | ModuleId::SpectralShaper)
    }
}

/// Default signal-flow order (the same as the `ModuleId` declaration order).
pub fn default_order() -> [ModuleId; MODULE_COUNT] {
    ModuleId::ALL
}

/// Validate and repair an order array so it is a permutation of all modules.
pub fn sanitize_order(order: &[u8]) -> [ModuleId; MODULE_COUNT] {
    let mut seen = [false; MODULE_COUNT];
    let mut out = Vec::with_capacity(MODULE_COUNT);
    for &o in order {
        if let Some(m) = ModuleId::from_index(o as usize) {
            if !seen[m.index()] {
                seen[m.index()] = true;
                out.push(m);
            }
        }
    }
    for m in ModuleId::ALL {
        if !seen[m.index()] {
            out.push(m);
        }
    }
    let mut arr = default_order();
    arr.copy_from_slice(&out[..MODULE_COUNT]);
    arr
}

pub struct Engine {
    pub unlimiter: Unlimiter,
    pub eq: ParametricEq,
    pub dyneq: DynamicEq,
    pub hub: SpectralHub,
    pub bass: BassControl,
    pub lef: LowEndFocus,
    pub dynamics: MultibandDynamics,
    pub vcomp: VintageComp,
    pub veq: VintageEq,
    pub vtape: VintageTape,
    pub exciter: Exciter,
    pub impact: Impact,
    pub imager: Imager,
    pub delay: StereoDelay,
    pub reverb: Reverb,
    pub vlimiter: VintageLimiter,
    pub maximizer: Maximizer,
    pub dither: Dither,

    pub enabled: [bool; MODULE_COUNT],
    pub order: [ModuleId; MODULE_COUNT],
    pub input_gain_db: f32,
    pub output_gain_db: f32,
    pub bypass: bool,
    /// Match perceived loudness when bypassed (uses input/output LUFS difference).
    pub in_meter: LoudnessMeter,
    pub out_meter: LoudnessMeter,
    pub stereo_meter: StereoMeter,
    pub spectrum_in: SpectrumAnalyzer,
    pub spectrum_out: SpectrumAnalyzer,

    dry: [DelayLine; 2],
    bypass_mix: f32,
    latency: usize,
    sr: f32,
    scratch_l: Vec<f32>,
    scratch_r: Vec<f32>,
}

impl Engine {
    pub fn new(sr: f32, max_block: usize) -> Self {
        let mut e = Self {
            unlimiter: Unlimiter::default(),
            eq: ParametricEq::default(),
            dyneq: DynamicEq::default(),
            hub: SpectralHub::default(),
            bass: BassControl::default(),
            lef: LowEndFocus::default(),
            dynamics: MultibandDynamics::default(),
            vcomp: VintageComp::default(),
            veq: VintageEq::default(),
            vtape: VintageTape::default(),
            exciter: Exciter::default(),
            impact: Impact::default(),
            imager: Imager::default(),
            delay: StereoDelay::default(),
            reverb: Reverb::default(),
            vlimiter: VintageLimiter::default(),
            maximizer: Maximizer::default(),
            dither: Dither::default(),
            enabled: [false; MODULE_COUNT],
            order: default_order(),
            input_gain_db: 0.0,
            output_gain_db: 0.0,
            bypass: false,
            in_meter: LoudnessMeter::new(sr),
            out_meter: LoudnessMeter::new(sr),
            stereo_meter: StereoMeter::default(),
            spectrum_in: SpectrumAnalyzer::new(sr),
            spectrum_out: SpectrumAnalyzer::new(sr),
            dry: [DelayLine::new(16384), DelayLine::new(16384)],
            bypass_mix: 0.0,
            latency: 0,
            sr,
            scratch_l: vec![0.0; max_block.max(64)],
            scratch_r: vec![0.0; max_block.max(64)],
        };
        e.prepare(sr, max_block);
        e
    }

    pub fn prepare(&mut self, sr: f32, max_block: usize) {
        self.sr = sr;
        self.unlimiter.prepare(sr);
        self.eq.prepare(sr);
        self.dyneq.prepare(sr);
        self.hub.prepare(sr);
        self.bass.prepare(sr);
        self.lef.prepare(sr);
        self.dynamics.prepare(sr);
        self.vcomp.prepare(sr);
        self.veq.prepare(sr);
        self.vtape.prepare(sr);
        self.exciter.prepare(sr);
        self.impact.prepare(sr);
        self.imager.prepare(sr);
        self.delay.prepare(sr);
        self.reverb.prepare(sr);
        self.vlimiter.prepare(sr);
        self.maximizer.prepare(sr);
        self.dither.reset();
        self.in_meter = LoudnessMeter::new(sr);
        self.out_meter = LoudnessMeter::new(sr);
        self.spectrum_in = SpectrumAnalyzer::new(sr);
        self.spectrum_out = SpectrumAnalyzer::new(sr);
        self.dry.iter_mut().for_each(|d| d.clear());
        self.scratch_l.resize(max_block.max(64), 0.0);
        self.scratch_r.resize(max_block.max(64), 0.0);
        self.latency = self.compute_latency();
    }

    pub fn reset(&mut self) {
        let (sr, n) = (self.sr, self.scratch_l.len());
        self.prepare(sr, n);
    }

    fn module_enabled(&self, m: ModuleId) -> bool {
        match m {
            ModuleId::MatchEq => self.enabled[m.index()] || self.hub.match_eq.capture != Capture::Off,
            _ => self.enabled[m.index()],
        }
    }

    fn module_latency(&self, m: ModuleId) -> usize {
        match m {
            ModuleId::Maximizer => self.maximizer.latency(),
            ModuleId::VintageTape => self.vtape.latency(),
            _ => 0,
        }
    }

    pub fn compute_latency(&self) -> usize {
        let mut total = 0;
        let mut spectral = false;
        for m in ModuleId::ALL {
            if !self.module_enabled(m) {
                continue;
            }
            if m.is_spectral() {
                spectral = true;
            } else {
                total += self.module_latency(m);
            }
        }
        if spectral {
            total += self.hub.latency();
        }
        total
    }

    /// Current total latency in samples (recompute with `refresh_latency` after toggles).
    pub fn latency(&self) -> usize {
        self.latency
    }

    pub fn refresh_latency(&mut self) -> usize {
        self.latency = self.compute_latency();
        self.latency
    }

    fn run_module(&mut self, m: ModuleId, l: &mut [f32], r: &mut [f32], hub_done: &mut bool) {
        match m {
            ModuleId::Unlimiter => self.unlimiter.process(l, r),
            ModuleId::StemEq | ModuleId::MatchEq | ModuleId::Stabilizer | ModuleId::SpectralShaper => {
                if !*hub_done {
                    *hub_done = true;
                    self.hub.stem.enabled = self.enabled[ModuleId::StemEq.index()];
                    self.hub.match_eq.enabled = self.enabled[ModuleId::MatchEq.index()];
                    self.hub.stabilizer.enabled = self.enabled[ModuleId::Stabilizer.index()];
                    self.hub.shaper.enabled = self.enabled[ModuleId::SpectralShaper.index()];
                    self.hub.process(l, r);
                }
            }
            ModuleId::Equalizer => self.eq.process(l, r),
            ModuleId::DynamicEq => self.dyneq.process(l, r),
            ModuleId::BassControl => self.bass.process(l, r),
            ModuleId::LowEndFocus => self.lef.process(l, r),
            ModuleId::Dynamics => self.dynamics.process(l, r),
            ModuleId::VintageComp => self.vcomp.process(l, r),
            ModuleId::VintageEq => self.veq.process(l, r),
            ModuleId::VintageTape => self.vtape.process(l, r),
            ModuleId::Exciter => self.exciter.process(l, r),
            ModuleId::Impact => self.impact.process(l, r),
            ModuleId::Imager => self.imager.process(l, r),
            ModuleId::Delay => self.delay.process(l, r),
            ModuleId::Reverb => self.reverb.process(l, r),
            ModuleId::VintageLimiter => self.vlimiter.process(l, r),
            ModuleId::Maximizer => self.maximizer.process(l, r),
        }
    }

    /// Process a block in place. Block length may be anything.
    pub fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        let n = l.len();
        if self.scratch_l.len() < n {
            self.scratch_l.resize(n, 0.0);
            self.scratch_r.resize(n, 0.0);
        }
        let gin = db_to_lin(self.input_gain_db);
        let gout = db_to_lin(self.output_gain_db);
        for i in 0..n {
            l[i] *= gin;
            r[i] *= gin;
        }
        self.in_meter.process(l, r);
        self.spectrum_in.process(l, r);
        // keep a delayed dry copy for latency-compensated bypass
        for i in 0..n {
            self.dry[0].push(l[i]);
            self.dry[1].push(r[i]);
            self.scratch_l[i] = self.dry[0].tap(self.latency.min(16000));
            self.scratch_r[i] = self.dry[1].tap(self.latency.min(16000));
        }
        let order = self.order;
        let mut hub_done = false;
        for m in order {
            if !self.module_enabled(m) {
                continue;
            }
            self.run_module(m, l, r, &mut hub_done);
        }
        for i in 0..n {
            l[i] *= gout;
            r[i] *= gout;
        }
        self.dither.process(l, r);
        // bypass crossfade (~10 ms)
        let target = if self.bypass { 1.0 } else { 0.0 };
        let step = 1.0 / (0.01 * self.sr);
        for i in 0..n {
            if self.bypass_mix < target {
                self.bypass_mix = (self.bypass_mix + step).min(target);
            } else if self.bypass_mix > target {
                self.bypass_mix = (self.bypass_mix - step).max(target);
            }
            if self.bypass_mix > 0.0 {
                l[i] += (self.scratch_l[i] - l[i]) * self.bypass_mix;
                r[i] += (self.scratch_r[i] - r[i]) * self.bypass_mix;
            }
        }
        self.out_meter.process(l, r);
        self.stereo_meter.process(l, r, self.sr);
        self.spectrum_out.process(l, r);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_chain_is_transparent() {
        let mut e = Engine::new(48000.0, 512);
        let src: Vec<f32> = (0..4096).map(|i| 0.3 * (i as f32 * 0.03).sin()).collect();
        let mut l = src.clone();
        let mut r = src.clone();
        e.process(&mut l, &mut r);
        assert_eq!(l, src);
    }

    #[test]
    fn latency_counts_spectral_once() {
        let mut e = Engine::new(48000.0, 512);
        e.enabled[ModuleId::StemEq.index()] = true;
        e.enabled[ModuleId::Stabilizer.index()] = true;
        assert_eq!(e.refresh_latency(), crate::stft::FFT_SIZE);
        e.enabled[ModuleId::Maximizer.index()] = true;
        assert!(e.refresh_latency() > crate::stft::FFT_SIZE);
    }

    #[test]
    fn order_sanitizer_repairs() {
        let o = sanitize_order(&[3, 3, 255, 0]);
        assert_eq!(o[0].index(), 3);
        assert_eq!(o[1].index(), 0);
        let mut seen = [false; MODULE_COUNT];
        for m in o {
            seen[m.index()] = true;
        }
        assert!(seen.iter().all(|x| *x));
    }

    #[test]
    fn full_chain_runs_finite() {
        let mut e = Engine::new(48000.0, 512);
        e.enabled = [true; MODULE_COUNT];
        e.refresh_latency();
        let mut l: Vec<f32> = (0..24576).map(|i| 0.5 * (i as f32 * 0.02).sin() + 0.2 * (i as f32 * 0.9).sin()).collect();
        let mut r = l.clone();
        for c in 0..(24576 / 512) {
            e.process(&mut l[c * 512..(c + 1) * 512], &mut r[c * 512..(c + 1) * 512]);
        }
        assert!(l.iter().chain(r.iter()).all(|x| x.is_finite() && x.abs() < 4.0));
    }
}

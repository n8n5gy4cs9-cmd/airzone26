//! State shared between the audio thread and the editor.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};

use airzone_dsp::assistant::Analysis;
use airzone_dsp::meter::SPEC_POINTS;
use airzone_dsp::spectral::SpectrumProfile;
use atomic_float_shim::AtomicF32;
use parking_lot::Mutex;

pub mod atomic_float_shim {
    pub use nih_plug::prelude::AtomicF32;
}

pub const GR_SLOTS: usize = 8;
pub const GR_MAXIMIZER: usize = 0;
pub const GR_DYN_BASE: usize = 1; // 3 bands
pub const GR_VCOMP: usize = 4;
pub const GR_VLIM: usize = 5;
pub const GR_UNLIM: usize = 6;

pub const CAP_OFF: u8 = 0;
pub const CAP_SOURCE: u8 = 1;
pub const CAP_REFERENCE: u8 = 2;

pub const ASSIST_IDLE: u8 = 0;
pub const ASSIST_LISTEN: u8 = 1;
pub const ASSIST_FINISH: u8 = 2;

pub struct Shared {
    pub in_momentary: AtomicF32,
    pub out_momentary: AtomicF32,
    pub out_short: AtomicF32,
    pub out_integrated: AtomicF32,
    pub out_lra: AtomicF32,
    pub in_integrated: AtomicF32,
    pub true_peak: AtomicF32,
    pub correlation: AtomicF32,
    pub width: AtomicF32,
    pub latency: AtomicU32,
    pub sample_rate: AtomicF32,
    pub gr: [AtomicF32; GR_SLOTS],
    pub spectrum_in: Vec<AtomicF32>,
    pub spectrum_out: Vec<AtomicF32>,
    pub stem_energy: [AtomicF32; 4],
    pub stab_curve: Vec<AtomicF32>,
    pub bass_levels: [AtomicF32; 3],

    // GUI -> audio commands
    pub capture: AtomicU8,
    pub clear_source: AtomicBool,
    pub clear_reference: AtomicBool,
    pub reset_meters: AtomicBool,
    pub assist_cmd: AtomicU8,
    pub pending_reference: Mutex<Option<(SpectrumProfile, String)>>,

    // audio -> GUI results
    pub match_progress: [AtomicU32; 2],
    pub match_curve: Mutex<Vec<f32>>,
    pub match_curve_dirty: AtomicBool,
    pub assist_seconds: AtomicF32,
    pub assist_result: Mutex<Option<Analysis>>,
}

impl Shared {
    pub fn new() -> Self {
        let af = |v: f32| AtomicF32::new(v);
        Self {
            in_momentary: af(-70.0),
            out_momentary: af(-70.0),
            out_short: af(-70.0),
            out_integrated: af(-70.0),
            out_lra: af(0.0),
            in_integrated: af(-70.0),
            true_peak: af(-120.0),
            correlation: af(0.0),
            width: af(0.0),
            latency: AtomicU32::new(0),
            sample_rate: af(44100.0),
            gr: std::array::from_fn(|_| af(0.0)),
            spectrum_in: (0..SPEC_POINTS).map(|_| af(-120.0)).collect(),
            spectrum_out: (0..SPEC_POINTS).map(|_| af(-120.0)).collect(),
            stem_energy: std::array::from_fn(|_| af(0.25)),
            stab_curve: (0..32).map(|_| af(0.0)).collect(),
            bass_levels: std::array::from_fn(|_| af(-100.0)),
            capture: AtomicU8::new(CAP_OFF),
            clear_source: AtomicBool::new(false),
            clear_reference: AtomicBool::new(false),
            reset_meters: AtomicBool::new(false),
            assist_cmd: AtomicU8::new(ASSIST_IDLE),
            pending_reference: Mutex::new(None),
            match_progress: [AtomicU32::new(0), AtomicU32::new(0)],
            match_curve: Mutex::new(vec![0.0; airzone_dsp::spectral::MATCH_BANDS]),
            match_curve_dirty: AtomicBool::new(false),
            assist_seconds: af(0.0),
            assist_result: Mutex::new(None),
        }
    }

    pub fn load(a: &AtomicF32) -> f32 {
        a.load(Ordering::Relaxed)
    }
}

impl Default for Shared {
    fn default() -> Self {
        Self::new()
    }
}

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
    pub pitch_voiced: AtomicBool,
    pub pitch_midi: AtomicF32,
    pub pitch_target: AtomicF32,
    pub pitch_shift: AtomicF32,
    pub pitch_f0: AtomicF32,
    pub pitch_voiced_ratio: AtomicF32,
    pub clarity_boost: AtomicF32,
    pub scope_l: Vec<AtomicF32>,
    pub scope_r: Vec<AtomicF32>,
    pub key_hist: [AtomicF32; 12],
    pub tune_detected: AtomicF32,
    pub tune_applied: AtomicF32,
    pub tune_conf: AtomicF32,
    pub clear_key_hist: AtomicBool,

    // GUI -> audio commands
    pub capture: AtomicU8,
    pub clear_source: AtomicBool,
    pub clear_reference: AtomicBool,
    pub reset_meters: AtomicBool,
    pub assist_cmd: AtomicU8,
    pub pending_reference: Mutex<Option<(SpectrumProfile, String)>>,

    // reference A/B player (GUI -> audio)
    pub pending_ref_track: Mutex<Option<std::sync::Arc<airzone_dsp::reference::ReferenceTrack>>>,
    pub ref_monitor: AtomicBool,
    pub ref_playing: AtomicBool,
    pub ref_loop: AtomicBool,
    pub ref_level_match: AtomicBool,
    pub ref_trim_db: AtomicF32,
    /// Seek target 0..1; negative = no request.
    pub ref_seek: AtomicF32,
    // reference A/B player (audio -> GUI)
    pub ref_pos: AtomicF32,
    pub ref_len_s: AtomicF32,
    pub ref_lufs: AtomicF32,
    pub ref_gain_db: AtomicF32,

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
            pitch_voiced: AtomicBool::new(false),
            pitch_midi: af(0.0),
            pitch_target: af(0.0),
            pitch_shift: af(0.0),
            pitch_f0: af(0.0),
            pitch_voiced_ratio: af(0.0),
            clarity_boost: af(0.0),
            scope_l: (0..airzone_dsp::chain::SCOPE_N).map(|_| af(0.0)).collect(),
            scope_r: (0..airzone_dsp::chain::SCOPE_N).map(|_| af(0.0)).collect(),
            key_hist: std::array::from_fn(|_| af(0.0)),
            tune_detected: af(0.0),
            tune_applied: af(0.0),
            tune_conf: af(0.0),
            clear_key_hist: AtomicBool::new(false),
            capture: AtomicU8::new(CAP_OFF),
            clear_source: AtomicBool::new(false),
            clear_reference: AtomicBool::new(false),
            reset_meters: AtomicBool::new(false),
            assist_cmd: AtomicU8::new(ASSIST_IDLE),
            pending_reference: Mutex::new(None),
            pending_ref_track: Mutex::new(None),
            ref_monitor: AtomicBool::new(false),
            ref_playing: AtomicBool::new(false),
            ref_loop: AtomicBool::new(true),
            ref_level_match: AtomicBool::new(true),
            ref_trim_db: af(0.0),
            ref_seek: af(-1.0),
            ref_pos: af(0.0),
            ref_len_s: af(0.0),
            ref_lufs: af(-70.0),
            ref_gain_db: af(0.0),
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

//! Parameter definitions for every AirZone 26 module.

use std::sync::Arc;

use nih_plug::prelude::*;
use nih_plug_egui::EguiState;
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------- helpers

pub fn db_param(name: &str, default: f32, min: f32, max: f32) -> FloatParam {
    FloatParam::new(name, default, FloatRange::Linear { min, max })
        .with_unit(" dB")
        .with_step_size(0.1)
}

pub fn hz_param(name: &str, default: f32, min: f32, max: f32) -> FloatParam {
    FloatParam::new(name, default, FloatRange::Skewed { min, max, factor: FloatRange::skew_factor(-2.0) })
        .with_unit(" Hz")
        .with_value_to_string(Arc::new(|v| if v >= 1000.0 { format!("{:.2}k", v / 1000.0) } else { format!("{:.0}", v) }))
}

pub fn ms_param(name: &str, default: f32, min: f32, max: f32) -> FloatParam {
    FloatParam::new(name, default, FloatRange::Skewed { min, max, factor: FloatRange::skew_factor(-1.5) })
        .with_unit(" ms")
        .with_value_to_string(Arc::new(|v| format!("{:.1}", v)))
}

pub fn pct_param(name: &str, default: f32) -> FloatParam {
    FloatParam::new(name, default, FloatRange::Linear { min: 0.0, max: 1.0 })
        .with_unit(" %")
        .with_value_to_string(Arc::new(|v| format!("{:.0}", v * 100.0)))
        .with_string_to_value(Arc::new(|s| s.trim().trim_end_matches('%').parse::<f32>().ok().map(|v| v / 100.0)))
}

pub fn lin_param(name: &str, default: f32, min: f32, max: f32) -> FloatParam {
    FloatParam::new(name, default, FloatRange::Linear { min, max }).with_step_size(0.01)
}

pub fn on_param(name: &str, default: bool) -> BoolParam {
    BoolParam::new(name, default)
}

// ---------------------------------------------------------------- enums

#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum KindP {
    #[name = "Bell"]
    Bell,
    #[name = "Low Shelf"]
    LowShelf,
    #[name = "High Shelf"]
    HighShelf,
    #[name = "High Pass"]
    HighPass,
    #[name = "Low Pass"]
    LowPass,
    #[name = "Notch"]
    Notch,
}

#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlopeP {
    #[name = "12 dB/oct"]
    S12,
    #[name = "24 dB/oct"]
    S24,
    #[name = "36 dB/oct"]
    S36,
    #[name = "48 dB/oct"]
    S48,
}

#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaceP {
    #[name = "Stereo"]
    Stereo,
    #[name = "Mid"]
    Mid,
    #[name = "Side"]
    Side,
    #[name = "Left"]
    Left,
    #[name = "Right"]
    Right,
}

#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum SatModeP {
    #[name = "Tape"]
    Tape,
    #[name = "Tube"]
    Tube,
    #[name = "Warm"]
    Warm,
    #[name = "Retro"]
    Retro,
}

#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimiterP {
    #[name = "AZ-1 Clean"]
    Clean,
    #[name = "AZ-2 Balanced"]
    Balanced,
    #[name = "AZ-3 Multiband"]
    Multiband,
    #[name = "AZ-4 Aggressive"]
    Aggressive,
    #[name = "AZ-5 Adaptive"]
    Adaptive,
    #[name = "AZ-6 Clip + Limit"]
    Clipper,
    #[name = "AZ-7 Smooth"]
    Smooth,
}

#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum TapeSpeedP {
    #[name = "7.5 ips"]
    Ips7,
    #[name = "15 ips"]
    Ips15,
    #[name = "30 ips"]
    Ips30,
}

#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum DitherP {
    #[name = "Off"]
    Off,
    #[name = "16 bit"]
    B16,
    #[name = "20 bit"]
    B20,
    #[name = "24 bit"]
    B24,
}

#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum SoloP {
    #[name = "Off"]
    Off,
    #[name = "Bass"]
    Bass,
    #[name = "Drums"]
    Drums,
    #[name = "Vocals"]
    Vocals,
    #[name = "Other"]
    Other,
}

#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum VeLow {
    #[name = "20 Hz"]
    F20,
    #[name = "30 Hz"]
    F30,
    #[name = "60 Hz"]
    F60,
    #[name = "100 Hz"]
    F100,
}

#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum VeHigh {
    #[name = "3 kHz"]
    K3,
    #[name = "4 kHz"]
    K4,
    #[name = "5 kHz"]
    K5,
    #[name = "8 kHz"]
    K8,
    #[name = "10 kHz"]
    K10,
    #[name = "12 kHz"]
    K12,
    #[name = "16 kHz"]
    K16,
}

#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum VeAtten {
    #[name = "5 kHz"]
    K5,
    #[name = "10 kHz"]
    K10,
    #[name = "20 kHz"]
    K20,
}

#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum PitchModeP {
    #[name = "Vocal"]
    Vocal,
    #[name = "Instrument"]
    Instrument,
    #[name = "Full Song (tuning)"]
    FullSong,
    #[name = "Vocal in Full Mix"]
    MixVocal,
}

#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteP {
    #[name = "C"]
    C,
    #[name = "C#"]
    Cs,
    #[name = "D"]
    D,
    #[name = "D#"]
    Ds,
    #[name = "E"]
    E,
    #[name = "F"]
    F,
    #[name = "F#"]
    Fs,
    #[name = "G"]
    G,
    #[name = "G#"]
    Gs,
    #[name = "A"]
    A,
    #[name = "A#"]
    As,
    #[name = "B"]
    B,
}

#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScaleP {
    #[name = "Chromatic"]
    Chromatic,
    #[name = "Major"]
    Major,
    #[name = "Natural Minor"]
    Minor,
    #[name = "Harmonic Minor"]
    HarmonicMinor,
    #[name = "Major Pentatonic"]
    MajorPent,
    #[name = "Minor Pentatonic"]
    MinorPent,
    #[name = "Blues"]
    Blues,
    #[name = "Dorian"]
    Dorian,
    #[name = "Mixolydian"]
    Mixolydian,
}

#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodecFmtP {
    #[name = "MP3-like"]
    Mp3,
    #[name = "AAC-like"]
    Aac,
    #[name = "Opus-like"]
    Opus,
}

// ---------------------------------------------------------------- module params

#[derive(Params)]
pub struct UnlimiterP {
    #[id = "amount"]
    pub amount: FloatParam,
    #[id = "boost"]
    pub max_boost: FloatParam,
    #[id = "thresh"]
    pub threshold: FloatParam,
    #[id = "release"]
    pub release: FloatParam,
    #[id = "comp"]
    pub compensate: BoolParam,
}

impl Default for UnlimiterP {
    fn default() -> Self {
        Self {
            amount: pct_param("Amount", 0.5),
            max_boost: db_param("Max Boost", 6.0, 0.0, 12.0),
            threshold: db_param("Threshold", 1.0, 0.0, 6.0),
            release: ms_param("Release", 60.0, 10.0, 400.0),
            compensate: on_param("Compensate", true),
        }
    }
}

#[derive(Params)]
pub struct StemP {
    #[id = "g0"]
    pub bass_gain: FloatParam,
    #[id = "g1"]
    pub drums_gain: FloatParam,
    #[id = "g2"]
    pub vocals_gain: FloatParam,
    #[id = "g3"]
    pub other_gain: FloatParam,
    #[id = "t0"]
    pub bass_tilt: FloatParam,
    #[id = "t1"]
    pub drums_tilt: FloatParam,
    #[id = "t2"]
    pub vocals_tilt: FloatParam,
    #[id = "t3"]
    pub other_tilt: FloatParam,
    #[id = "focus"]
    pub focus: FloatParam,
    #[id = "solo"]
    pub solo: EnumParam<SoloP>,
}

impl Default for StemP {
    fn default() -> Self {
        Self {
            bass_gain: db_param("Bass Gain", 0.0, -12.0, 12.0),
            drums_gain: db_param("Drums Gain", 0.0, -12.0, 12.0),
            vocals_gain: db_param("Vocals Gain", 0.0, -12.0, 12.0),
            other_gain: db_param("Other Gain", 0.0, -12.0, 12.0),
            bass_tilt: db_param("Bass Tone", 0.0, -6.0, 6.0),
            drums_tilt: db_param("Drums Tone", 0.0, -6.0, 6.0),
            vocals_tilt: db_param("Vocals Tone", 0.0, -6.0, 6.0),
            other_tilt: db_param("Other Tone", 0.0, -6.0, 6.0),
            focus: lin_param("Focus", 2.0, 1.0, 4.0),
            solo: EnumParam::new("Solo", SoloP::Off),
        }
    }
}

#[derive(Params)]
pub struct MatchP {
    #[id = "amount"]
    pub amount: FloatParam,
    #[id = "smooth"]
    pub smoothing: FloatParam,
    #[id = "maxgain"]
    pub max_gain: FloatParam,
}

impl Default for MatchP {
    fn default() -> Self {
        Self {
            amount: pct_param("Amount", 1.0),
            smoothing: lin_param("Smoothing", 4.0, 1.0, 16.0),
            max_gain: db_param("Max Correction", 9.0, 1.0, 18.0),
        }
    }
}

#[derive(Params)]
pub struct EqBandP {
    #[id = "on"]
    pub on: BoolParam,
    #[id = "kind"]
    pub kind: EnumParam<KindP>,
    #[id = "freq"]
    pub freq: FloatParam,
    #[id = "gain"]
    pub gain: FloatParam,
    #[id = "q"]
    pub q: FloatParam,
    #[id = "slope"]
    pub slope: EnumParam<SlopeP>,
    #[id = "place"]
    pub place: EnumParam<PlaceP>,
}

impl EqBandP {
    fn new(i: usize) -> Self {
        let defaults: [(f32, KindP); 8] = [
            (30.0, KindP::HighPass),
            (80.0, KindP::LowShelf),
            (250.0, KindP::Bell),
            (700.0, KindP::Bell),
            (2000.0, KindP::Bell),
            (5000.0, KindP::Bell),
            (10000.0, KindP::HighShelf),
            (20000.0, KindP::LowPass),
        ];
        let (f, k) = defaults[i];
        Self {
            on: on_param("Enabled", false),
            kind: EnumParam::new("Type", k),
            freq: hz_param("Frequency", f, 20.0, 20000.0),
            gain: db_param("Gain", 0.0, -18.0, 18.0),
            q: FloatParam::new("Q", 1.0, FloatRange::Skewed { min: 0.1, max: 12.0, factor: FloatRange::skew_factor(-1.5) })
                .with_step_size(0.01),
            slope: EnumParam::new("Slope", SlopeP::S12),
            place: EnumParam::new("Placement", PlaceP::Stereo),
        }
    }
}

#[derive(Params)]
pub struct EqP {
    #[nested(array, group = "EQ Band")]
    pub bands: [EqBandP; 8],
}

impl Default for EqP {
    fn default() -> Self {
        Self { bands: std::array::from_fn(EqBandP::new) }
    }
}

#[derive(Params)]
pub struct DynEqBandP {
    #[id = "dq_on"]
    pub on: BoolParam,
    #[id = "dq_freq"]
    pub freq: FloatParam,
    #[id = "dq_q"]
    pub q: FloatParam,
    #[id = "dq_gain"]
    pub gain: FloatParam,
    #[id = "dq_thresh"]
    pub threshold: FloatParam,
    #[id = "dq_range"]
    pub range: FloatParam,
    #[id = "dq_ratio"]
    pub ratio: FloatParam,
    #[id = "dq_att"]
    pub attack: FloatParam,
    #[id = "dq_rel"]
    pub release: FloatParam,
}

impl DynEqBandP {
    fn new(i: usize) -> Self {
        let f = [120.0, 400.0, 2500.0, 8000.0][i];
        Self {
            on: on_param("Enabled", false),
            freq: hz_param("Frequency", f, 20.0, 20000.0),
            q: lin_param("Q", 1.0, 0.3, 10.0),
            gain: db_param("Static Gain", 0.0, -12.0, 12.0),
            threshold: db_param("Threshold", -24.0, -60.0, 0.0),
            range: db_param("Dynamic Range", -6.0, -15.0, 15.0),
            ratio: lin_param("Ratio", 3.0, 1.0, 20.0),
            attack: ms_param("Attack", 10.0, 0.5, 200.0),
            release: ms_param("Release", 120.0, 10.0, 1500.0),
        }
    }
}

#[derive(Params)]
pub struct DynEqP {
    #[nested(array, group = "Dynamic EQ Band")]
    pub bands: [DynEqBandP; 4],
}

impl Default for DynEqP {
    fn default() -> Self {
        Self { bands: std::array::from_fn(DynEqBandP::new) }
    }
}

#[derive(Params)]
pub struct StabP {
    #[id = "amount"]
    pub amount: FloatParam,
    #[id = "range"]
    pub range: FloatParam,
    #[id = "slope"]
    pub slope: FloatParam,
    #[id = "speed"]
    pub speed: FloatParam,
    #[id = "useref"]
    pub use_ref: BoolParam,
}

impl Default for StabP {
    fn default() -> Self {
        Self {
            amount: pct_param("Amount", 0.5),
            range: db_param("Range", 4.0, 1.0, 9.0),
            slope: lin_param("Target Slope (dB/oct)", -4.5, -7.0, -2.0),
            speed: pct_param("Speed", 0.3),
            use_ref: on_param("Use Reference", false),
        }
    }
}

#[derive(Params)]
pub struct ShaperP {
    #[id = "depth"]
    pub depth: FloatParam,
    #[id = "thresh"]
    pub threshold: FloatParam,
    #[id = "width"]
    pub width: FloatParam,
    #[id = "low"]
    pub low: FloatParam,
    #[id = "high"]
    pub high: FloatParam,
    #[id = "tilt"]
    pub tilt: FloatParam,
    #[id = "delta"]
    pub delta: BoolParam,
}

impl Default for ShaperP {
    fn default() -> Self {
        Self {
            depth: pct_param("Depth", 0.5),
            threshold: db_param("Sensitivity", 4.0, 0.0, 12.0),
            width: lin_param("Selectivity", 12.0, 3.0, 40.0),
            low: hz_param("Low Limit", 200.0, 50.0, 5000.0),
            high: hz_param("High Limit", 14000.0, 2000.0, 20000.0),
            tilt: lin_param("Tilt (dB/oct)", 0.0, -3.0, 3.0),
            delta: on_param("Listen to Delta", false),
        }
    }
}

#[derive(Params)]
pub struct BassP {
    #[id = "subf"]
    pub sub_freq: FloatParam,
    #[id = "bassf"]
    pub bass_freq: FloatParam,
    #[id = "subg"]
    pub sub_gain: FloatParam,
    #[id = "bassg"]
    pub bass_gain: FloatParam,
    #[id = "submono"]
    pub sub_mono: FloatParam,
    #[id = "bassmono"]
    pub bass_mono: FloatParam,
    #[id = "punch"]
    pub punch: FloatParam,
    #[id = "tight"]
    pub tighten: FloatParam,
    #[id = "auto"]
    pub auto_balance: BoolParam,
    #[id = "ratio"]
    pub target_ratio: FloatParam,
    #[id = "mix"]
    pub mix: FloatParam,
}

impl Default for BassP {
    fn default() -> Self {
        Self {
            sub_freq: hz_param("Sub / Bass Split", 70.0, 30.0, 140.0),
            bass_freq: hz_param("Bass / Low-Mid Split", 250.0, 120.0, 500.0),
            sub_gain: db_param("Sub Gain", 0.0, -12.0, 12.0),
            bass_gain: db_param("Bass Gain", 0.0, -12.0, 12.0),
            sub_mono: pct_param("Sub Mono", 1.0),
            bass_mono: pct_param("Bass Mono", 0.0),
            punch: db_param("Punch", 0.0, -6.0, 9.0),
            tighten: pct_param("Tighten", 0.0),
            auto_balance: on_param("Auto Balance", false),
            target_ratio: db_param("Sub-Bass Target", 1.5, -6.0, 9.0),
            mix: pct_param("Mix", 1.0),
        }
    }
}

#[derive(Params)]
pub struct LefP {
    #[id = "freq"]
    pub freq: FloatParam,
    #[id = "punch"]
    pub punch: FloatParam,
    #[id = "sustain"]
    pub sustain: FloatParam,
    #[id = "contrast"]
    pub contrast: FloatParam,
    #[id = "mix"]
    pub mix: FloatParam,
}

impl Default for LefP {
    fn default() -> Self {
        Self {
            freq: hz_param("Focus Frequency", 120.0, 40.0, 400.0),
            punch: db_param("Punch", 0.0, -6.0, 9.0),
            sustain: db_param("Sustain", 0.0, -9.0, 6.0),
            contrast: pct_param("Contrast", 0.0),
            mix: pct_param("Mix", 1.0),
        }
    }
}

#[derive(Params)]
pub struct DynBandP {
    #[id = "dy_on"]
    pub on: BoolParam,
    #[id = "dy_thresh"]
    pub threshold: FloatParam,
    #[id = "dy_ratio"]
    pub ratio: FloatParam,
    #[id = "dy_knee"]
    pub knee: FloatParam,
    #[id = "dy_att"]
    pub attack: FloatParam,
    #[id = "dy_rel"]
    pub release: FloatParam,
    #[id = "dy_gain"]
    pub gain: FloatParam,
    #[id = "dy_expth"]
    pub exp_threshold: FloatParam,
    #[id = "dy_exprat"]
    pub exp_ratio: FloatParam,
}

impl Default for DynBandP {
    fn default() -> Self {
        Self {
            on: on_param("Enabled", true),
            threshold: db_param("Threshold", -18.0, -60.0, 0.0),
            ratio: lin_param("Ratio", 2.0, 1.0, 20.0),
            knee: db_param("Knee", 6.0, 0.0, 24.0),
            attack: ms_param("Attack", 15.0, 0.5, 200.0),
            release: ms_param("Release", 150.0, 10.0, 2000.0),
            gain: db_param("Gain", 0.0, -12.0, 12.0),
            exp_threshold: db_param("Expander Threshold", -90.0, -90.0, -20.0),
            exp_ratio: lin_param("Expander Ratio", 1.0, 1.0, 4.0),
        }
    }
}

#[derive(Params)]
pub struct DynamicsP {
    #[id = "xo1"]
    pub xo1: FloatParam,
    #[id = "xo2"]
    pub xo2: FloatParam,
    #[id = "link"]
    pub link: FloatParam,
    #[id = "ms"]
    pub mid_side: BoolParam,
    #[id = "out"]
    pub output: FloatParam,
    #[id = "mix"]
    pub mix: FloatParam,
    #[nested(array, group = "Dynamics Band")]
    pub bands: [DynBandP; 3],
}

impl Default for DynamicsP {
    fn default() -> Self {
        Self {
            xo1: hz_param("Crossover 1", 120.0, 40.0, 1000.0),
            xo2: hz_param("Crossover 2", 2500.0, 500.0, 12000.0),
            link: pct_param("Stereo Link", 1.0),
            mid_side: on_param("Mid/Side", false),
            output: db_param("Output", 0.0, -12.0, 12.0),
            mix: pct_param("Mix", 1.0),
            bands: std::array::from_fn(|_| DynBandP::default()),
        }
    }
}

#[derive(Params)]
pub struct VcompP {
    #[id = "in"]
    pub input: FloatParam,
    #[id = "thresh"]
    pub threshold: FloatParam,
    #[id = "ratio"]
    pub ratio: FloatParam,
    #[id = "att"]
    pub attack: FloatParam,
    #[id = "rel"]
    pub release: FloatParam,
    #[id = "makeup"]
    pub makeup: FloatParam,
    #[id = "warm"]
    pub warmth: FloatParam,
    #[id = "mix"]
    pub mix: FloatParam,
}

impl Default for VcompP {
    fn default() -> Self {
        Self {
            input: db_param("Input", 0.0, -12.0, 12.0),
            threshold: db_param("Threshold", -14.0, -40.0, 0.0),
            ratio: lin_param("Ratio", 3.0, 1.5, 10.0),
            attack: ms_param("Attack", 12.0, 0.5, 100.0),
            release: ms_param("Release", 250.0, 40.0, 1500.0),
            makeup: db_param("Makeup", 0.0, -6.0, 18.0),
            warmth: pct_param("Warmth", 0.2),
            mix: pct_param("Mix", 1.0),
        }
    }
}

#[derive(Params)]
pub struct VeqP {
    #[id = "lf"]
    pub low_freq: EnumParam<VeLow>,
    #[id = "lb"]
    pub low_boost: FloatParam,
    #[id = "la"]
    pub low_atten: FloatParam,
    #[id = "hf"]
    pub high_freq: EnumParam<VeHigh>,
    #[id = "hb"]
    pub high_boost: FloatParam,
    #[id = "bw"]
    pub bandwidth: FloatParam,
    #[id = "af"]
    pub atten_freq: EnumParam<VeAtten>,
    #[id = "ha"]
    pub high_atten: FloatParam,
    #[id = "tube"]
    pub tube: FloatParam,
}

impl Default for VeqP {
    fn default() -> Self {
        Self {
            low_freq: EnumParam::new("Low Frequency", VeLow::F60),
            low_boost: lin_param("Low Boost", 0.0, 0.0, 10.0),
            low_atten: lin_param("Low Atten", 0.0, 0.0, 10.0),
            high_freq: EnumParam::new("High Boost Freq", VeHigh::K10),
            high_boost: lin_param("High Boost", 0.0, 0.0, 10.0),
            bandwidth: lin_param("Bandwidth", 5.0, 0.0, 10.0),
            atten_freq: EnumParam::new("High Atten Freq", VeAtten::K10),
            high_atten: lin_param("High Atten", 0.0, 0.0, 10.0),
            tube: pct_param("Tube", 0.15),
        }
    }
}

#[derive(Params)]
pub struct VtapeP {
    #[id = "drive"]
    pub drive: FloatParam,
    #[id = "speed"]
    pub speed: EnumParam<TapeSpeedP>,
    #[id = "sat"]
    pub saturation: FloatParam,
    #[id = "wow"]
    pub wow: FloatParam,
    #[id = "noise"]
    pub noise: FloatParam,
    #[id = "age"]
    pub age: FloatParam,
    #[id = "out"]
    pub output: FloatParam,
}

impl Default for VtapeP {
    fn default() -> Self {
        Self {
            drive: db_param("Drive", 3.0, -6.0, 18.0),
            speed: EnumParam::new("Speed", TapeSpeedP::Ips15),
            saturation: pct_param("Saturation", 0.7),
            wow: pct_param("Wow & Flutter", 0.0),
            noise: pct_param("Hiss", 0.0),
            age: pct_param("Age", 0.0),
            output: db_param("Output", 0.0, -12.0, 12.0),
        }
    }
}

#[derive(Params)]
pub struct ExBandP {
    #[id = "ex_on"]
    pub on: BoolParam,
    #[id = "ex_drive"]
    pub drive: FloatParam,
    #[id = "ex_amount"]
    pub amount: FloatParam,
    #[id = "ex_gain"]
    pub gain: FloatParam,
}

impl Default for ExBandP {
    fn default() -> Self {
        Self {
            on: on_param("Enabled", true),
            drive: db_param("Drive", 6.0, 0.0, 24.0),
            amount: pct_param("Amount", 0.0),
            gain: db_param("Gain", 0.0, -12.0, 12.0),
        }
    }
}

#[derive(Params)]
pub struct ExciterP {
    #[id = "xo1"]
    pub xo1: FloatParam,
    #[id = "xo2"]
    pub xo2: FloatParam,
    #[id = "xo3"]
    pub xo3: FloatParam,
    #[id = "mode"]
    pub mode: EnumParam<SatModeP>,
    #[id = "mix"]
    pub mix: FloatParam,
    #[nested(array, group = "Exciter Band")]
    pub bands: [ExBandP; 4],
}

impl Default for ExciterP {
    fn default() -> Self {
        Self {
            xo1: hz_param("Crossover 1", 150.0, 40.0, 800.0),
            xo2: hz_param("Crossover 2", 1200.0, 300.0, 6000.0),
            xo3: hz_param("Crossover 3", 6000.0, 2000.0, 16000.0),
            mode: EnumParam::new("Mode", SatModeP::Tape),
            mix: pct_param("Mix", 1.0),
            bands: std::array::from_fn(|_| ExBandP::default()),
        }
    }
}

#[derive(Params)]
pub struct ImpactBandP {
    #[id = "im_att"]
    pub attack: FloatParam,
    #[id = "im_sus"]
    pub sustain: FloatParam,
}

impl Default for ImpactBandP {
    fn default() -> Self {
        Self { attack: db_param("Attack", 0.0, -9.0, 12.0), sustain: db_param("Sustain", 0.0, -9.0, 9.0) }
    }
}

#[derive(Params)]
pub struct ImpactP {
    #[id = "xo1"]
    pub xo1: FloatParam,
    #[id = "xo2"]
    pub xo2: FloatParam,
    #[id = "mix"]
    pub mix: FloatParam,
    #[nested(array, group = "Impact Band")]
    pub bands: [ImpactBandP; 3],
}

impl Default for ImpactP {
    fn default() -> Self {
        Self {
            xo1: hz_param("Crossover 1", 200.0, 60.0, 1000.0),
            xo2: hz_param("Crossover 2", 3000.0, 1000.0, 10000.0),
            mix: pct_param("Mix", 1.0),
            bands: std::array::from_fn(|_| ImpactBandP::default()),
        }
    }
}

#[derive(Params)]
pub struct ImagerBandP {
    #[id = "ig_width"]
    pub width: FloatParam,
}

impl Default for ImagerBandP {
    fn default() -> Self {
        Self {
            width: FloatParam::new("Width", 1.0, FloatRange::Linear { min: 0.0, max: 2.0 })
                .with_unit(" %")
                .with_value_to_string(Arc::new(|v| format!("{:.0}", v * 100.0)))
                .with_string_to_value(Arc::new(|s| s.trim().trim_end_matches('%').parse::<f32>().ok().map(|v| v / 100.0))),
        }
    }
}

#[derive(Params)]
pub struct ImagerP {
    #[id = "xo1"]
    pub xo1: FloatParam,
    #[id = "xo2"]
    pub xo2: FloatParam,
    #[id = "xo3"]
    pub xo3: FloatParam,
    #[id = "stz"]
    pub stereoize: FloatParam,
    #[id = "bal"]
    pub balance: FloatParam,
    #[nested(array, group = "Imager Band")]
    pub bands: [ImagerBandP; 4],
}

impl Default for ImagerP {
    fn default() -> Self {
        Self {
            xo1: hz_param("Crossover 1", 150.0, 40.0, 800.0),
            xo2: hz_param("Crossover 2", 1500.0, 300.0, 6000.0),
            xo3: hz_param("Crossover 3", 6000.0, 2000.0, 16000.0),
            stereoize: pct_param("Stereoize", 0.0),
            balance: lin_param("Balance", 0.0, -1.0, 1.0),
            bands: std::array::from_fn(|_| ImagerBandP::default()),
        }
    }
}

#[derive(Params)]
pub struct DelayP {
    #[id = "tl"]
    pub time_l: FloatParam,
    #[id = "tr"]
    pub time_r: FloatParam,
    #[id = "fb"]
    pub feedback: FloatParam,
    #[id = "pp"]
    pub ping_pong: BoolParam,
    #[id = "lc"]
    pub lowcut: FloatParam,
    #[id = "hc"]
    pub highcut: FloatParam,
    #[id = "mix"]
    pub mix: FloatParam,
    #[id = "solo"]
    pub wet_solo: BoolParam,
}

impl Default for DelayP {
    fn default() -> Self {
        Self {
            time_l: ms_param("Time L", 250.0, 5.0, 2000.0),
            time_r: ms_param("Time R", 375.0, 5.0, 2000.0),
            feedback: pct_param("Feedback", 0.3),
            ping_pong: on_param("Ping-Pong", false),
            lowcut: hz_param("Low Cut", 200.0, 20.0, 2000.0),
            highcut: hz_param("High Cut", 6000.0, 1000.0, 20000.0),
            mix: pct_param("Mix", 0.15),
            wet_solo: on_param("Solo Wet (hear only the echoes)", false),
        }
    }
}

#[derive(Params)]
pub struct ReverbP {
    #[id = "room"]
    pub room: FloatParam,
    #[id = "damp"]
    pub damping: FloatParam,
    #[id = "width"]
    pub width: FloatParam,
    #[id = "pre"]
    pub predelay: FloatParam,
    #[id = "lc"]
    pub lowcut: FloatParam,
    #[id = "mix"]
    pub mix: FloatParam,
    #[id = "solo"]
    pub wet_solo: BoolParam,
}

impl Default for ReverbP {
    fn default() -> Self {
        Self {
            room: pct_param("Room Size", 0.5),
            damping: pct_param("Damping", 0.5),
            width: pct_param("Width", 1.0),
            predelay: ms_param("Pre-Delay", 10.0, 0.0, 200.0),
            lowcut: hz_param("Low Cut", 150.0, 20.0, 1000.0),
            mix: pct_param("Mix", 0.1),
            wet_solo: on_param("Solo Wet (hear only the reverb)", false),
        }
    }
}

#[derive(Params)]
pub struct VlimP {
    #[id = "in"]
    pub input: FloatParam,
    #[id = "out"]
    pub output: FloatParam,
    #[id = "att"]
    pub attack: FloatParam,
    #[id = "rel"]
    pub release: FloatParam,
    #[id = "char"]
    pub character: FloatParam,
}

impl Default for VlimP {
    fn default() -> Self {
        Self {
            input: db_param("Input", 0.0, 0.0, 18.0),
            output: db_param("Output", 0.0, -12.0, 3.0),
            attack: ms_param("Attack", 0.2, 0.02, 5.0),
            release: ms_param("Release", 100.0, 20.0, 1500.0),
            character: pct_param("Character", 0.3),
        }
    }
}

#[derive(Params)]
pub struct MaxP {
    #[id = "mode"]
    pub mode: EnumParam<LimiterP>,
    #[id = "gain"]
    pub gain: FloatParam,
    #[id = "ceil"]
    pub ceiling: FloatParam,
    #[id = "rel"]
    pub release: FloatParam,
    #[id = "char"]
    pub character: FloatParam,
    #[id = "indep"]
    pub independence: FloatParam,
    #[id = "tp"]
    pub true_peak: BoolParam,
}

impl Default for MaxP {
    fn default() -> Self {
        Self {
            mode: EnumParam::new("Algorithm", LimiterP::Balanced),
            gain: db_param("Gain", 0.0, 0.0, 20.0),
            ceiling: db_param("Ceiling", -0.3, -12.0, 0.0),
            release: ms_param("Release", 120.0, 10.0, 1500.0),
            character: pct_param("Character", 0.0),
            independence: pct_param("Stereo Independence", 0.0),
            true_peak: on_param("True Peak", true),
        }
    }
}

#[derive(Params)]
pub struct PitchP {
    #[id = "mode"]
    pub mode: EnumParam<PitchModeP>,
    #[id = "root"]
    pub root: EnumParam<NoteP>,
    #[id = "scale"]
    pub scale: EnumParam<ScaleP>,
    #[id = "retune"]
    pub retune: FloatParam,
    #[id = "amount"]
    pub amount: FloatParam,
    #[id = "preserve"]
    pub preserve: FloatParam,
    #[id = "tol"]
    pub tolerance: FloatParam,
    #[id = "maxshift"]
    pub max_shift: FloatParam,
    #[id = "minhz"]
    pub min_hz: FloatParam,
    #[id = "maxhz"]
    pub max_hz: FloatParam,
    #[id = "conf"]
    pub confidence: FloatParam,
    #[id = "a4"]
    pub a4: FloatParam,
    #[id = "tauto"]
    pub tune_auto: BoolParam,
    #[id = "tmanual"]
    pub tune_manual: FloatParam,
    #[id = "ttarget"]
    pub tune_target: FloatParam,
    #[id = "tamount"]
    pub tune_amount: FloatParam,
}

impl Default for PitchP {
    fn default() -> Self {
        Self {
            mode: EnumParam::new("Mode", PitchModeP::Vocal),
            root: EnumParam::new("Key", NoteP::C),
            scale: EnumParam::new("Scale", ScaleP::Chromatic),
            retune: ms_param("Retune Speed", 90.0, 0.0, 400.0),
            amount: pct_param("Correction Amount", 1.0),
            preserve: pct_param("Preserve Vibrato", 0.85),
            tolerance: lin_param("Tolerance (cents)", 8.0, 0.0, 50.0),
            max_shift: lin_param("Max Correction (cents)", 200.0, 20.0, 400.0),
            min_hz: hz_param("Lowest Pitch", 70.0, 50.0, 500.0),
            max_hz: hz_param("Highest Pitch", 1000.0, 200.0, 1500.0),
            confidence: lin_param("Voicing Confidence", 0.6, 0.3, 0.95),
            a4: lin_param("A4 Reference (Hz)", 440.0, 415.0, 466.0),
            tune_auto: on_param("Auto Detect Tuning", true),
            tune_manual: lin_param("Manual Tuning (cents)", 0.0, -100.0, 100.0),
            tune_target: lin_param("Target A4 (Hz)", 440.0, 415.0, 466.0),
            tune_amount: pct_param("Tuning Amount", 1.0),
        }
    }
}

#[derive(Params)]
pub struct ClarityP {
    #[id = "amount"]
    pub amount: FloatParam,
    #[id = "freq"]
    pub freq: FloatParam,
    #[id = "harm"]
    pub harmonics: FloatParam,
    #[id = "warm"]
    pub warmth: FloatParam,
    #[id = "mix"]
    pub mix: FloatParam,
}

impl Default for ClarityP {
    fn default() -> Self {
        Self {
            amount: pct_param("Detail", 0.4),
            freq: hz_param("Lower Edge", 2000.0, 500.0, 8000.0),
            harmonics: pct_param("Harmonics", 0.2),
            warmth: lin_param("Warmth", 0.0, -1.0, 1.0),
            mix: pct_param("Mix", 1.0),
        }
    }
}

#[derive(Params)]
pub struct CodecP {
    #[id = "fmt"]
    pub format: EnumParam<CodecFmtP>,
    #[id = "kbps"]
    pub kbps: FloatParam,
    #[id = "delta"]
    pub delta: BoolParam,
}

impl Default for CodecP {
    fn default() -> Self {
        Self {
            format: EnumParam::new("Codec Style", CodecFmtP::Mp3),
            kbps: FloatParam::new("Bitrate", 128.0, FloatRange::Linear { min: 32.0, max: 320.0 }).with_unit(" kbps").with_step_size(1.0),
            delta: on_param("Listen to Difference", false),
        }
    }
}

// ---------------------------------------------------------------- module switches

#[derive(Params)]
pub struct Switches {
    #[id = "on_unlimiter"]
    pub unlimiter: BoolParam,
    #[id = "on_stem"]
    pub stem: BoolParam,
    #[id = "on_match"]
    pub match_eq: BoolParam,
    #[id = "on_eq"]
    pub eq: BoolParam,
    #[id = "on_dyneq"]
    pub dyneq: BoolParam,
    #[id = "on_stab"]
    pub stabilizer: BoolParam,
    #[id = "on_shaper"]
    pub shaper: BoolParam,
    #[id = "on_bass"]
    pub bass: BoolParam,
    #[id = "on_lef"]
    pub lef: BoolParam,
    #[id = "on_dyn"]
    pub dynamics: BoolParam,
    #[id = "on_vcomp"]
    pub vcomp: BoolParam,
    #[id = "on_veq"]
    pub veq: BoolParam,
    #[id = "on_vtape"]
    pub vtape: BoolParam,
    #[id = "on_exciter"]
    pub exciter: BoolParam,
    #[id = "on_impact"]
    pub impact: BoolParam,
    #[id = "on_imager"]
    pub imager: BoolParam,
    #[id = "on_delay"]
    pub delay: BoolParam,
    #[id = "on_reverb"]
    pub reverb: BoolParam,
    #[id = "on_vlim"]
    pub vlimiter: BoolParam,
    #[id = "on_max"]
    pub maximizer: BoolParam,
    #[id = "on_pitch"]
    pub pitch: BoolParam,
    #[id = "on_clarity"]
    pub clarity: BoolParam,
    #[id = "on_codec"]
    pub codec: BoolParam,
}

impl Default for Switches {
    fn default() -> Self {
        let p = |n: &str| BoolParam::new(format!("{n} On"), false);
        Self {
            unlimiter: p("Unlimiter"),
            stem: p("Stem EQ"),
            match_eq: p("Match EQ"),
            eq: p("Equalizer"),
            dyneq: p("Dynamic EQ"),
            stabilizer: p("Stabilizer"),
            shaper: p("Spectral Shaper"),
            bass: p("Bass Control"),
            lef: p("Low End Focus"),
            dynamics: p("Dynamics"),
            vcomp: p("Vintage Comp"),
            veq: p("Vintage EQ"),
            vtape: p("Vintage Tape"),
            exciter: p("Exciter"),
            impact: p("Impact"),
            imager: p("Imager"),
            delay: p("Delay"),
            reverb: p("Reverb"),
            vlimiter: p("Vintage Limiter"),
            maximizer: p("Maximizer"),
            pitch: p("Pitch Correction"),
            clarity: p("Clarity"),
            codec: p("Codec Preview"),
        }
    }
}

impl Switches {
    /// Switch parameters in `ModuleId` order.
    pub fn by_index(&self) -> [&BoolParam; 23] {
        [
            &self.unlimiter,
            &self.stem,
            &self.match_eq,
            &self.eq,
            &self.dyneq,
            &self.stabilizer,
            &self.shaper,
            &self.bass,
            &self.lef,
            &self.dynamics,
            &self.vcomp,
            &self.veq,
            &self.vtape,
            &self.exciter,
            &self.impact,
            &self.imager,
            &self.delay,
            &self.reverb,
            &self.vlimiter,
            &self.maximizer,
            &self.pitch,
            &self.clarity,
            &self.codec,
        ]
    }
}

// ---------------------------------------------------------------- persisted data

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct MatchData {
    pub source_power: Vec<f32>,
    pub source_frames: u32,
    pub ref_power: Vec<f32>,
    pub ref_frames: u32,
    pub ref_name: String,
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct UiPrefs {
    pub preset_name: String,
    pub genre: u8,
    pub target: u8,
}

// ---------------------------------------------------------------- root

#[derive(Params)]
pub struct AirZoneParams {
    // new key: sizes stored by older builds (including broken resizes) are ignored
    #[persist = "editor-size-v2"]
    pub editor_state: Arc<EguiState>,
    #[persist = "chain-order"]
    pub order: Arc<RwLock<Vec<u8>>>,
    #[persist = "match-data"]
    pub match_data: Arc<Mutex<MatchData>>,
    #[persist = "ui-prefs"]
    pub ui_prefs: Arc<Mutex<UiPrefs>>,

    #[id = "bypass"]
    pub bypass: BoolParam,
    #[id = "in_gain"]
    pub input_gain: FloatParam,
    #[id = "out_gain"]
    pub output_gain: FloatParam,
    #[id = "dither"]
    pub dither: EnumParam<DitherP>,
    #[id = "gmatch"]
    pub gain_match: BoolParam,

    #[nested(group = "Modules")]
    pub on: Switches,
    #[nested(id_prefix = "un", group = "Unlimiter")]
    pub unlimiter: UnlimiterP,
    #[nested(id_prefix = "st", group = "Stem EQ")]
    pub stem: StemP,
    #[nested(id_prefix = "mq", group = "Match EQ")]
    pub match_eq: MatchP,
    #[nested(id_prefix = "eq", group = "Equalizer")]
    pub eq: EqP,
    #[nested(id_prefix = "dq", group = "Dynamic EQ")]
    pub dyneq: DynEqP,
    #[nested(id_prefix = "sb", group = "Stabilizer")]
    pub stab: StabP,
    #[nested(id_prefix = "ss", group = "Spectral Shaper")]
    pub shaper: ShaperP,
    #[nested(id_prefix = "bc", group = "Bass Control")]
    pub bass: BassP,
    #[nested(id_prefix = "lf", group = "Low End Focus")]
    pub lef: LefP,
    #[nested(id_prefix = "dy", group = "Dynamics")]
    pub dynamics: DynamicsP,
    #[nested(id_prefix = "vc", group = "Vintage Comp")]
    pub vcomp: VcompP,
    #[nested(id_prefix = "ve", group = "Vintage EQ")]
    pub veq: VeqP,
    #[nested(id_prefix = "vt", group = "Vintage Tape")]
    pub vtape: VtapeP,
    #[nested(id_prefix = "ex", group = "Exciter")]
    pub exciter: ExciterP,
    #[nested(id_prefix = "im", group = "Impact")]
    pub impact: ImpactP,
    #[nested(id_prefix = "ig", group = "Imager")]
    pub imager: ImagerP,
    #[nested(id_prefix = "dl", group = "Delay")]
    pub delay: DelayP,
    #[nested(id_prefix = "rv", group = "Reverb")]
    pub reverb: ReverbP,
    #[nested(id_prefix = "vl", group = "Vintage Limiter")]
    pub vlimiter: VlimP,
    #[nested(id_prefix = "mx", group = "Maximizer")]
    pub max: MaxP,
    #[nested(id_prefix = "pc", group = "Pitch Correction")]
    pub pitch: PitchP,
    #[nested(id_prefix = "cl", group = "Clarity")]
    pub clarity: ClarityP,
    #[nested(id_prefix = "cp", group = "Codec Preview")]
    pub codec: CodecP,
}

impl Default for AirZoneParams {
    fn default() -> Self {
        Self {
            editor_state: EguiState::from_size(1040, 680),
            // (default window size; users can pick another one from the "Window" menu)
            order: Arc::new(RwLock::new(airzone_dsp::chain::default_order().iter().map(|m| m.index() as u8).collect())),
            match_data: Arc::new(Mutex::new(MatchData::default())),
            ui_prefs: Arc::new(Mutex::new(UiPrefs::default())),
            bypass: BoolParam::new("Bypass", false),
            input_gain: db_param("Input Gain", 0.0, -24.0, 24.0),
            output_gain: db_param("Output Gain", 0.0, -24.0, 12.0),
            dither: EnumParam::new("Dither", DitherP::Off),
            gain_match: BoolParam::new("Loudness-Matched Bypass", true),
            on: Switches::default(),
            unlimiter: UnlimiterP::default(),
            stem: StemP::default(),
            match_eq: MatchP::default(),
            eq: EqP::default(),
            dyneq: DynEqP::default(),
            stab: StabP::default(),
            shaper: ShaperP::default(),
            bass: BassP::default(),
            lef: LefP::default(),
            dynamics: DynamicsP::default(),
            vcomp: VcompP::default(),
            veq: VeqP::default(),
            vtape: VtapeP::default(),
            exciter: ExciterP::default(),
            impact: ImpactP::default(),
            imager: ImagerP::default(),
            delay: DelayP::default(),
            reverb: ReverbP::default(),
            vlimiter: VlimP::default(),
            max: MaxP::default(),
            pitch: PitchP::default(),
            clarity: ClarityP::default(),
            codec: CodecP::default(),
        }
    }
}

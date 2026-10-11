//! Per-module presets: every one of the 21 modules has its own small preset library.
//!
//! Applying a module preset resets *only that module's parameters* to their defaults, sets the
//! preset's values and switches the module on. Everything else in the chain is left untouched.

use std::collections::HashMap;
use std::path::PathBuf;

use airzone_dsp::chain::ModuleId;
use nih_plug::prelude::*;
use serde::{Deserialize, Serialize};

use crate::params::AirZoneParams;
use crate::presets::{param_lookup, user_dir, Preset};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModulePreset {
    /// `ModuleId` index
    pub module: u8,
    pub name: String,
    pub description: String,
    /// plain values with *full* parameter ids
    pub values: Vec<(String, f32)>,
    #[serde(default)]
    pub user: bool,
}

impl ModulePreset {
    pub fn module_id(&self) -> Option<ModuleId> {
        ModuleId::from_index(self.module as usize)
    }

    /// As a complete-chain preset (used by tests and for sharing): module values + switch on.
    pub fn as_preset(&self) -> Preset {
        let mut values = self.values.clone();
        if let Some(m) = self.module_id() {
            values.push((module_switch(m).to_string(), 1.0));
        }
        Preset { name: self.name.clone(), category: "Module".into(), description: self.description.clone(), values }
    }
}

/// Id prefix shared by all parameters of a module.
pub fn module_prefix(m: ModuleId) -> &'static str {
    match m {
        ModuleId::Unlimiter => "un_",
        ModuleId::StemEq => "st_",
        ModuleId::MatchEq => "mq_",
        ModuleId::Equalizer => "eq_",
        ModuleId::DynamicEq => "dq_",
        ModuleId::Stabilizer => "sb_",
        ModuleId::SpectralShaper => "ss_",
        ModuleId::BassControl => "bc_",
        ModuleId::LowEndFocus => "lf_",
        ModuleId::Dynamics => "dy_",
        ModuleId::VintageComp => "vc_",
        ModuleId::VintageEq => "ve_",
        ModuleId::VintageTape => "vt_",
        ModuleId::Exciter => "ex_",
        ModuleId::Impact => "im_",
        ModuleId::Imager => "ig_",
        ModuleId::Delay => "dl_",
        ModuleId::Reverb => "rv_",
        ModuleId::VintageLimiter => "vl_",
        ModuleId::Maximizer => "mx_",
        ModuleId::PitchCorrect => "pc_",
        ModuleId::Clarity => "cl_",
        ModuleId::CodecPreview => "cp_",
    }
}

/// The on/off switch parameter of a module.
pub fn module_switch(m: ModuleId) -> &'static str {
    match m {
        ModuleId::Unlimiter => "on_unlimiter",
        ModuleId::StemEq => "on_stem",
        ModuleId::MatchEq => "on_match",
        ModuleId::Equalizer => "on_eq",
        ModuleId::DynamicEq => "on_dyneq",
        ModuleId::Stabilizer => "on_stab",
        ModuleId::SpectralShaper => "on_shaper",
        ModuleId::BassControl => "on_bass",
        ModuleId::LowEndFocus => "on_lef",
        ModuleId::Dynamics => "on_dyn",
        ModuleId::VintageComp => "on_vcomp",
        ModuleId::VintageEq => "on_veq",
        ModuleId::VintageTape => "on_vtape",
        ModuleId::Exciter => "on_exciter",
        ModuleId::Impact => "on_impact",
        ModuleId::Imager => "on_imager",
        ModuleId::Delay => "on_delay",
        ModuleId::Reverb => "on_reverb",
        ModuleId::VintageLimiter => "on_vlim",
        ModuleId::Maximizer => "on_max",
        ModuleId::PitchCorrect => "on_pitch",
        ModuleId::Clarity => "on_clarity",
        ModuleId::CodecPreview => "on_codec",
    }
}

// ------------------------------------------------------------------------------ builder

struct MB {
    m: ModuleId,
    v: Vec<(String, f32)>,
}

fn mb(m: ModuleId) -> MB {
    MB { m, v: Vec::new() }
}

impl MB {
    /// Set a parameter by its id *without* the module prefix.
    fn s(mut self, k: &str, x: f32) -> Self {
        self.v.push((format!("{}{}", module_prefix(self.m), k), x));
        self
    }
    /// Set `k_1 .. k_n` from a slice.
    fn arr(mut self, k: &str, xs: &[f32]) -> Self {
        for (i, x) in xs.iter().enumerate() {
            self = self.s(&format!("{}_{}", k, i + 1), *x);
        }
        self
    }
    fn done(self, name: &str, desc: &str) -> ModulePreset {
        ModulePreset { module: self.m.index() as u8, name: name.into(), description: desc.into(), values: self.v, user: false }
    }
}

// enum constants used below
const BELL: f32 = 0.0;
const LSH: f32 = 1.0;
const HSH: f32 = 2.0;
const HPF: f32 = 3.0;
const SIDE: f32 = 2.0;
const TAPE: f32 = 0.0;
const TUBE: f32 = 1.0;
const WARM: f32 = 2.0;
const RETRO: f32 = 3.0;

/// Built-in module presets (several per module).
pub fn factory() -> Vec<ModulePreset> {
    use ModuleId as M;
    let mut v: Vec<ModulePreset> = Vec::new();

    // ---------------------------------------------------------------- Reverb
    v.push(mb(M::Reverb).s("room", 0.25).s("damp", 0.7).s("width", 1.0).s("pre", 8.0).s("lc", 300.0).s("mix", 0.05).done("Mastering Glue Room", "Tiny room that glues dry stems together."));
    v.push(mb(M::Reverb).s("room", 0.35).s("damp", 0.55).s("width", 1.0).s("pre", 5.0).s("lc", 200.0).s("mix", 0.10).done("Small Studio", "Short, natural studio ambience."));
    v.push(mb(M::Reverb).s("room", 0.6).s("damp", 0.6).s("width", 1.0).s("pre", 20.0).s("lc", 150.0).s("mix", 0.12).done("Warm Hall", "Medium hall with soft highs."));
    v.push(mb(M::Reverb).s("room", 0.9).s("damp", 0.45).s("width", 1.0).s("pre", 35.0).s("lc", 120.0).s("mix", 0.18).done("Large Cathedral", "Long, spacious tail for cinematic material."));
    v.push(mb(M::Reverb).s("room", 0.5).s("damp", 0.25).s("width", 1.0).s("pre", 2.0).s("lc", 250.0).s("mix", 0.15).done("Bright Plate", "Bright, dense plate-style reverb."));
    v.push(mb(M::Reverb).s("room", 0.45).s("damp", 0.85).s("width", 0.8).s("pre", 12.0).s("lc", 180.0).s("mix", 0.12).done("Dark Chamber", "Dark, subdued chamber."));
    v.push(mb(M::Reverb).s("room", 0.3).s("damp", 0.4).s("width", 1.0).s("pre", 25.0).s("lc", 350.0).s("mix", 0.08).done("Vocal Air", "Light airy space that keeps vocals clear."));
    v.push(mb(M::Reverb).s("room", 0.15).s("damp", 0.6).s("width", 0.7).s("pre", 0.0).s("lc", 400.0).s("mix", 0.06).done("Tight Ambience", "Barely-there ambience, almost no tail."));

    // ---------------------------------------------------------------- Delay
    v.push(mb(M::Delay).s("tl", 90.0).s("tr", 120.0).s("fb", 0.1).s("pp", 0.0).s("lc", 300.0).s("hc", 6000.0).s("mix", 0.10).done("Slap Back", "Short slap for depth."));
    v.push(mb(M::Delay).s("tl", 500.0).s("tr", 500.0).s("fb", 0.25).s("pp", 0.0).s("lc", 250.0).s("hc", 5000.0).s("mix", 0.12).done("Quarter Note (120 BPM)", "500 ms echo, set the times to your tempo."));
    v.push(mb(M::Delay).s("tl", 375.0).s("tr", 375.0).s("fb", 0.30).s("pp", 0.0).s("lc", 300.0).s("hc", 5000.0).s("mix", 0.12).done("Dotted Eighth (120 BPM)", "Rhythmic dotted-eighth echo."));
    v.push(mb(M::Delay).s("tl", 300.0).s("tr", 450.0).s("fb", 0.35).s("pp", 1.0).s("lc", 350.0).s("hc", 6000.0).s("mix", 0.14).done("Ping-Pong Wide", "Bouncing stereo echoes."));
    v.push(mb(M::Delay).s("tl", 350.0).s("tr", 350.0).s("fb", 0.45).s("pp", 0.0).s("lc", 400.0).s("hc", 3500.0).s("mix", 0.15).done("Tape Echo", "Dark, repeating tape-style echo."));
    v.push(mb(M::Delay).s("tl", 15.0).s("tr", 22.0).s("fb", 0.0).s("pp", 0.0).s("lc", 150.0).s("hc", 10000.0).s("mix", 0.5).done("Haas Widener", "Very short delays that widen a narrow source."));
    v.push(mb(M::Delay).s("tl", 140.0).s("tr", 190.0).s("fb", 0.15).s("pp", 0.0).s("lc", 500.0).s("hc", 4500.0).s("mix", 0.07).done("Subtle Depth", "Hidden echo that adds depth without being heard."));

    // ---------------------------------------------------------------- Equalizer
    v.push(mb(M::Equalizer).s("on_2", 1.0).s("kind_2", LSH).s("freq_2", 80.0).s("gain_2", 1.2).s("q_2", 0.7).s("on_7", 1.0).s("kind_7", HSH).s("freq_7", 10000.0).s("gain_7", 1.5).s("q_7", 0.7).done("Gentle Smile", "Soft low and high lift."));
    v.push(mb(M::Equalizer).s("on_7", 1.0).s("kind_7", HSH).s("freq_7", 12000.0).s("gain_7", 2.5).s("q_7", 0.6).done("Air Lift", "Open, silky high shelf."));
    v.push(mb(M::Equalizer).s("on_1", 1.0).s("kind_1", HPF).s("freq_1", 30.0).s("slope_1", 1.0).done("Low Cut 30 Hz", "Removes rumble below 30 Hz (24 dB/oct)."));
    v.push(mb(M::Equalizer).s("on_3", 1.0).s("kind_3", BELL).s("freq_3", 250.0).s("gain_3", -2.0).s("q_3", 1.0).s("on_4", 1.0).s("kind_4", BELL).s("freq_4", 400.0).s("gain_4", -1.0).s("q_4", 1.2).done("Mud Cleanup", "Clears low-mid build-up."));
    v.push(mb(M::Equalizer).s("on_5", 1.0).s("kind_5", BELL).s("freq_5", 3500.0).s("gain_5", 1.5).s("q_5", 0.9).s("on_6", 1.0).s("kind_6", BELL).s("freq_6", 5500.0).s("gain_6", 1.0).s("q_6", 1.0).done("Vocal Presence", "Brings vocals and leads forward."));
    v.push(mb(M::Equalizer).s("on_2", 1.0).s("kind_2", LSH).s("freq_2", 120.0).s("gain_2", 1.0).s("q_2", 0.7).s("on_5", 1.0).s("kind_5", BELL).s("freq_5", 4000.0).s("gain_5", -1.0).s("q_5", 0.8).s("on_7", 1.0).s("kind_7", HSH).s("freq_7", 9000.0).s("gain_7", -0.8).s("q_7", 0.7).done("Warm & Smooth", "Warmer, softer top end."));
    v.push(mb(M::Equalizer).s("on_1", 1.0).s("kind_1", HPF).s("freq_1", 120.0).s("slope_1", 1.0).s("place_1", SIDE).done("Side Low Cut 120 Hz", "Keeps the low end centred by high-passing only the side signal."));
    v.push(mb(M::Equalizer).s("on_7", 1.0).s("kind_7", HSH).s("freq_7", 9000.0).s("gain_7", 2.0).s("q_7", 0.7).s("place_7", SIDE).done("Wide Air (Side Only)", "Adds air only to the side channel for a wider top."));
    v.push(mb(M::Equalizer).s("on_2", 1.0).s("kind_2", LSH).s("freq_2", 60.0).s("gain_2", 2.0).s("q_2", 0.7).s("on_3", 1.0).s("kind_3", BELL).s("freq_3", 120.0).s("gain_3", 1.0).s("q_3", 1.0).done("Bass Weight", "More weight in the bass."));

    // ---------------------------------------------------------------- Dynamic EQ
    v.push(mb(M::DynamicEq).s("dq_on_1", 1.0).s("dq_freq_1", 250.0).s("dq_q_1", 1.2).s("dq_thresh_1", -26.0).s("dq_range_1", -4.0).s("dq_ratio_1", 3.0).s("dq_on_2", 1.0).s("dq_freq_2", 380.0).s("dq_q_2", 1.5).s("dq_thresh_2", -26.0).s("dq_range_2", -3.0).s("dq_ratio_2", 3.0).done("De-Mud", "Cuts 250-400 Hz only when it builds up."));
    v.push(mb(M::DynamicEq).s("dq_on_1", 1.0).s("dq_freq_1", 7000.0).s("dq_q_1", 3.0).s("dq_thresh_1", -30.0).s("dq_range_1", -5.0).s("dq_ratio_1", 4.0).s("dq_att_1", 2.0).s("dq_rel_1", 60.0).done("De-Ess", "Dynamic cut for sibilance."));
    v.push(mb(M::DynamicEq).s("dq_on_1", 1.0).s("dq_freq_1", 2600.0).s("dq_q_1", 1.5).s("dq_thresh_1", -26.0).s("dq_range_1", -4.0).s("dq_ratio_1", 3.0).done("Harsh Mid Tamer", "Tames forward 2.5 kHz harshness."));
    v.push(mb(M::DynamicEq).s("dq_on_1", 1.0).s("dq_freq_1", 90.0).s("dq_q_1", 1.0).s("dq_thresh_1", -24.0).s("dq_range_1", -3.0).s("dq_ratio_1", 3.0).s("dq_att_1", 15.0).s("dq_rel_1", 150.0).done("Bass Tightener", "Pulls back boomy bass peaks."));
    v.push(mb(M::DynamicEq).s("dq_on_1", 1.0).s("dq_freq_1", 450.0).s("dq_q_1", 1.4).s("dq_thresh_1", -26.0).s("dq_range_1", -3.0).s("dq_ratio_1", 3.0).done("Box Remover", "Dynamic cut of boxy 450 Hz."));
    v.push(mb(M::DynamicEq).s("dq_on_1", 1.0).s("dq_freq_1", 10000.0).s("dq_q_1", 0.8).s("dq_thresh_1", -40.0).s("dq_range_1", 3.0).s("dq_ratio_1", 2.0).done("Dynamic Sparkle", "Boosts highs only when there is high-frequency content."));

    // ---------------------------------------------------------------- Stabilizer
    v.push(mb(M::Stabilizer).s("amount", 0.4).s("range", 3.0).s("slope", -4.5).s("speed", 0.25).done("Gentle Balance", "Light, slow tonal levelling."));
    v.push(mb(M::Stabilizer).s("amount", 0.8).s("range", 6.0).s("slope", -4.5).s("speed", 0.4).done("Strong Balance", "Firm tonal correction toward a natural slope."));
    v.push(mb(M::Stabilizer).s("amount", 0.6).s("range", 4.0).s("slope", -3.8).s("speed", 0.3).done("Brighter Target", "Aims for a brighter overall balance."));
    v.push(mb(M::Stabilizer).s("amount", 0.6).s("range", 4.0).s("slope", -5.2).s("speed", 0.3).done("Darker Target", "Aims for a warmer, darker balance."));
    v.push(mb(M::Stabilizer).s("amount", 0.6).s("range", 4.0).s("slope", -4.5).s("speed", 0.3).s("useref", 1.0).done("Follow Reference", "Follows the Match EQ reference shape."));

    // ---------------------------------------------------------------- Spectral Shaper
    v.push(mb(M::SpectralShaper).s("depth", 0.6).s("thresh", 3.0).s("width", 12.0).s("low", 2000.0).s("high", 10000.0).done("Soothe Harshness", "Reduces resonant harshness in the upper mids."));
    v.push(mb(M::SpectralShaper).s("depth", 0.35).s("thresh", 5.0).s("width", 14.0).s("low", 200.0).s("high", 14000.0).done("Subtle Resonance Control", "Gentle full-range resonance suppression."));
    v.push(mb(M::SpectralShaper).s("depth", 0.55).s("thresh", 3.0).s("width", 8.0).s("low", 5000.0).s("high", 12000.0).done("Vocal Sibilance", "Focused on the sibilance region."));
    v.push(mb(M::SpectralShaper).s("depth", 0.2).s("tilt", 0.6).done("Tilt Brighter", "Gentle tilt toward the highs."));
    v.push(mb(M::SpectralShaper).s("depth", 0.2).s("tilt", -0.6).done("Tilt Warmer", "Gentle tilt toward the lows."));
    v.push(mb(M::SpectralShaper).s("depth", 0.5).s("thresh", 3.0).s("delta", 1.0).done("Listen to Delta", "Plays only what is being removed."));

    // ---------------------------------------------------------------- Stem EQ
    v.push(mb(M::StemEq).s("g2", 3.0).s("g3", -0.5).s("t2", 1.0).s("focus", 2.5).done("Vocals +3", "Lifts the vocal stem."));
    v.push(mb(M::StemEq).s("g2", -3.0).s("g3", 0.5).s("t2", -0.5).s("focus", 2.5).done("Vocals -3", "Sits the vocal deeper."));
    v.push(mb(M::StemEq).s("g1", 2.5).s("g3", -0.8).s("t1", 1.0).s("focus", 2.5).done("Drums +2.5", "More punch from the drums."));
    v.push(mb(M::StemEq).s("g0", 2.0).s("focus", 2.0).done("Bass +2", "More bass weight."));
    v.push(mb(M::StemEq).s("g0", -3.0).s("focus", 2.0).done("Bass -3", "Tames overbearing bass."));
    v.push(mb(M::StemEq).s("t2", 3.0).s("focus", 2.0).done("Brighter Vocals", "Opens up the vocal."));
    v.push(mb(M::StemEq).s("t0", -2.0).s("t1", -1.5).s("focus", 2.0).done("Warm Bass & Drums", "Darker rhythm section."));
    v.push(mb(M::StemEq).s("g2", -12.0).s("focus", 3.5).done("Karaoke Reduction", "Strong vocal reduction (not a perfect separation)."));
    v.push(mb(M::StemEq).s("g0", -0.5).s("g1", 1.0).s("g2", 1.0).s("g3", -0.5).s("t1", 0.5).s("t2", 0.5).s("focus", 2.0).done("Balanced Rebalance", "Small moves that even out a lopsided mix."));

    // ---------------------------------------------------------------- Match EQ
    v.push(mb(M::MatchEq).s("amount", 0.5).s("smooth", 6.0).s("maxgain", 6.0).done("Subtle Match", "Half-strength match, broad and safe."));
    v.push(mb(M::MatchEq).s("amount", 1.0).s("smooth", 4.0).s("maxgain", 12.0).done("Full Match", "Full-strength match."));
    v.push(mb(M::MatchEq).s("amount", 0.8).s("smooth", 10.0).s("maxgain", 6.0).done("Smooth Broad Match", "Very smooth curve that follows only the big picture."));

    // ---------------------------------------------------------------- Unlimiter
    v.push(mb(M::Unlimiter).s("amount", 0.35).s("boost", 4.0).s("thresh", 1.0).s("release", 60.0).s("comp", 1.0).done("Gentle Restore", "Light peak restoration."));
    v.push(mb(M::Unlimiter).s("amount", 0.7).s("boost", 8.0).s("thresh", 0.8).s("release", 60.0).s("comp", 1.0).done("Restore Crushed Mix", "Strong restoration for over-limited audio."));
    v.push(mb(M::Unlimiter).s("amount", 1.0).s("boost", 12.0).s("thresh", 0.5).s("release", 40.0).s("comp", 1.0).done("Max Punch", "Maximum transient recovery."));
    v.push(mb(M::Unlimiter).s("amount", 0.5).s("boost", 6.0).s("thresh", 1.0).s("release", 50.0).s("comp", 0.0).done("Peaks Only (No Compensation)", "Boosts peaks without lowering the body."));

    // ---------------------------------------------------------------- Bass Control
    v.push(mb(M::BassControl).s("subf", 90.0).s("submono", 1.0).done("Club Sub Mono", "Mono below 90 Hz for club systems."));
    v.push(mb(M::BassControl).s("tight", 0.4).s("punch", 1.5).s("submono", 1.0).done("Tight Bass", "Controlled, punchy bass."));
    v.push(mb(M::BassControl).s("auto", 1.0).s("ratio", 1.5).s("submono", 1.0).done("Auto Balance", "Slowly balances sub against bass."));
    v.push(mb(M::BassControl).s("subg", 2.5).s("submono", 1.0).done("More Sub", "Adds sub-bass weight."));
    v.push(mb(M::BassControl).s("subg", -2.5).s("bassg", -1.0).s("submono", 1.0).done("Less Boom", "Reduces boominess."));
    v.push(mb(M::BassControl).s("subf", 140.0).s("submono", 1.0).s("bassmono", 0.4).done("Vinyl Safe", "Mono bass for vinyl cutting."));

    // ---------------------------------------------------------------- Low End Focus
    v.push(mb(M::LowEndFocus).s("freq", 100.0).s("punch", 3.0).s("sustain", -2.0).s("contrast", 0.3).done("Punchy Kick", "Emphasises kick attack."));
    v.push(mb(M::LowEndFocus).s("freq", 120.0).s("punch", 1.5).s("sustain", -3.0).s("contrast", 0.6).done("Tight Groove", "Short, tight low end."));
    v.push(mb(M::LowEndFocus).s("freq", 70.0).s("sustain", 3.0).done("Deep & Long", "Longer sub tails."));
    v.push(mb(M::LowEndFocus).s("freq", 110.0).s("contrast", 0.7).done("Controlled Low End", "Dynamic control of the lows."));

    // ---------------------------------------------------------------- Dynamics
    v.push(mb(M::Dynamics).arr("dy_thresh", &[-22.0, -20.0, -20.0]).arr("dy_ratio", &[1.6, 1.6, 1.6]).arr("dy_att", &[30.0, 15.0, 6.0]).arr("dy_rel", &[220.0, 140.0, 90.0]).done("Gentle Glue", "Light three-band glue."));
    v.push(mb(M::Dynamics).arr("dy_thresh", &[-26.0, -24.0, -24.0]).arr("dy_ratio", &[2.6, 2.6, 2.6]).arr("dy_gain", &[0.8, 0.8, 0.8]).arr("dy_att", &[25.0, 12.0, 5.0]).arr("dy_rel", &[200.0, 130.0, 80.0]).done("Loud & Tight", "Firm control for loud masters."));
    v.push(mb(M::Dynamics).arr("dy_thresh", &[-28.0, -26.0, -26.0]).arr("dy_ratio", &[1.3, 1.3, 1.3]).done("Transparent Leveler", "Very gentle levelling."));
    v.push(mb(M::Dynamics).s("dy_thresh_1", -24.0).s("dy_ratio_1", 3.0).s("dy_on_2", 0.0).s("dy_on_3", 0.0).done("Bass Control Only", "Compresses only the low band."));
    v.push(mb(M::Dynamics).s("dy_on_1", 0.0).s("dy_on_2", 0.0).s("dy_thresh_3", -28.0).s("dy_ratio_3", 3.0).done("De-Harsh Highs", "Compresses only the high band."));
    v.push(mb(M::Dynamics).s("ms", 1.0).s("link", 0.5).arr("dy_thresh", &[-22.0, -22.0, -22.0]).arr("dy_ratio", &[2.0, 2.0, 2.0]).done("Mid/Side Control", "Mid and side compressed independently."));
    v.push(mb(M::Dynamics).arr("dy_thresh", &[-22.0, -20.0, -20.0]).arr("dy_ratio", &[1.6, 1.6, 1.6]).arr("dy_expth", &[-60.0, -60.0, -60.0]).arr("dy_exprat", &[1.5, 1.5, 1.5]).done("Noise Floor Tamer", "Gentle compression plus downward expansion of quiet noise."));

    // ---------------------------------------------------------------- Vintage Comp
    v.push(mb(M::VintageComp).s("in", 0.0).s("thresh", -20.0).s("ratio", 3.0).s("att", 30.0).s("rel", 400.0).s("makeup", 2.5).s("warm", 0.3).done("Opto Smooth", "Slow, smooth opto-style levelling."));
    v.push(mb(M::VintageComp).s("in", 3.0).s("thresh", -22.0).s("ratio", 3.5).s("att", 20.0).s("rel", 300.0).s("makeup", 3.0).s("warm", 0.25).done("Vocal Leveler", "Evens out a lead vocal."));
    v.push(mb(M::VintageComp).s("thresh", -16.0).s("ratio", 2.0).s("att", 18.0).s("rel", 220.0).s("makeup", 1.5).s("warm", 0.2).done("Mix Bus Glue", "Classic bus glue."));
    v.push(mb(M::VintageComp).s("in", 6.0).s("thresh", -30.0).s("ratio", 8.0).s("att", 5.0).s("rel", 150.0).s("makeup", 6.0).s("warm", 0.4).s("mix", 0.35).done("Parallel Squash", "Heavy compression blended in parallel."));
    v.push(mb(M::VintageComp).s("thresh", -10.0).s("ratio", 1.6).s("att", 40.0).s("rel", 500.0).s("makeup", 0.5).s("warm", 0.7).done("Gentle Tube Warmth", "Mostly saturation, hardly any compression."));
    v.push(mb(M::VintageComp).s("in", 4.0).s("thresh", -24.0).s("ratio", 6.0).s("att", 3.0).s("rel", 120.0).s("makeup", 4.0).s("warm", 0.3).s("mix", 0.5).done("Drum Smash", "Punchy parallel drum-style compression."));

    // ---------------------------------------------------------------- Vintage EQ
    v.push(mb(M::VintageEq).s("lf", 2.0).s("lb", 6.0).s("la", 4.0).done("Big Bass", "Boost and cut at 60 Hz for big, tight bass."));
    v.push(mb(M::VintageEq).s("hf", 4.0).s("hb", 4.0).s("bw", 6.0).done("Silky Top", "Smooth 10 kHz lift."));
    v.push(mb(M::VintageEq).s("hf", 6.0).s("hb", 5.0).s("bw", 7.0).done("Air Band 16k", "Broad air above 10 kHz."));
    v.push(mb(M::VintageEq).s("lf", 2.0).s("lb", 3.0).s("la", 1.0).s("hf", 3.0).s("hb", 2.0).s("bw", 5.0).s("af", 1.0).s("ha", 2.0).s("tube", 0.3).done("Smooth & Warm", "Warm low end, gentle top."));
    v.push(mb(M::VintageEq).s("hf", 2.0).s("hb", 4.0).s("bw", 4.0).done("Presence 5k", "Forward presence."));
    v.push(mb(M::VintageEq).s("af", 0.0).s("ha", 4.0).done("Roll Off Harshness", "Attenuates the highs from 5 kHz."));

    // ---------------------------------------------------------------- Vintage Tape
    v.push(mb(M::VintageTape).s("drive", 4.0).s("speed", 1.0).s("sat", 0.6).s("age", 0.05).done("Tape Glue 15 ips", "Subtle tape saturation."));
    v.push(mb(M::VintageTape).s("drive", 6.0).s("speed", 0.0).s("sat", 0.8).s("wow", 0.05).s("age", 0.2).s("out", -1.0).done("Warm 7.5 ips", "Warmer, darker tape."));
    v.push(mb(M::VintageTape).s("drive", 2.0).s("speed", 2.0).s("sat", 0.35).done("Hi-Fi 30 ips", "Clean, high-speed tape."));
    v.push(mb(M::VintageTape).s("drive", 8.0).s("speed", 0.0).s("sat", 0.9).s("wow", 0.35).s("noise", 0.3).s("age", 0.7).s("out", -2.0).done("Cassette Dust", "Lo-fi cassette character."));
    v.push(mb(M::VintageTape).s("drive", 2.0).s("speed", 1.0).s("sat", 0.4).done("Subtle Saturation", "Barely noticeable saturation."));
    v.push(mb(M::VintageTape).s("drive", 6.0).s("speed", 0.0).s("sat", 0.8).s("wow", 0.25).s("noise", 0.15).s("age", 0.5).s("out", -1.0).done("Worn Reel", "Worn, wobbly tape."));

    // ---------------------------------------------------------------- Exciter
    v.push(mb(M::Exciter).s("mode", TAPE).arr("ex_amount", &[0.0, 0.0, 0.1, 0.35]).arr("ex_drive", &[6.0, 6.0, 6.0, 6.0]).done("Air Sparkle", "Gentle top-end harmonics."));
    v.push(mb(M::Exciter).s("mode", TUBE).arr("ex_amount", &[0.25, 0.2, 0.05, 0.0]).arr("ex_drive", &[5.0, 5.0, 5.0, 5.0]).done("Tube Warmth", "Even harmonics in the lows and mids."));
    v.push(mb(M::Exciter).s("mode", TAPE).arr("ex_amount", &[0.0, 0.1, 0.3, 0.15]).arr("ex_drive", &[6.0, 6.0, 6.0, 6.0]).done("Presence Boost", "Adds presence in the upper mids."));
    v.push(mb(M::Exciter).s("mode", RETRO).s("mix", 0.6).arr("ex_amount", &[0.0, 0.2, 0.35, 0.4]).arr("ex_drive", &[12.0, 12.0, 12.0, 12.0]).done("Retro Grit", "Edgy, gritty excitement."));
    v.push(mb(M::Exciter).s("mode", TAPE).arr("ex_amount", &[0.4, 0.1, 0.0, 0.0]).arr("ex_drive", &[8.0, 8.0, 8.0, 8.0]).done("Bass Harmonics", "Harmonics that help bass translate on small speakers."));
    v.push(mb(M::Exciter).s("mode", WARM).arr("ex_amount", &[0.2, 0.25, 0.15, 0.05]).arr("ex_drive", &[5.0, 5.0, 5.0, 5.0]).done("Even Harmonic Warmth", "Smooth second-harmonic colour."));

    // ---------------------------------------------------------------- Impact
    v.push(mb(M::Impact).arr("im_att", &[3.0, 1.5, 1.0]).arr("im_sus", &[-1.0, 0.0, 0.0]).done("Punchy Drums", "More snap and attack."));
    v.push(mb(M::Impact).arr("im_att", &[-3.0, -2.0, -2.0]).done("Soften Transients", "Reduces harsh attacks."));
    v.push(mb(M::Impact).arr("im_sus", &[2.0, 3.0, 2.0]).done("Lush Sustain", "Longer, fuller tails."));
    v.push(mb(M::Impact).arr("im_att", &[4.0, 0.0, 0.0]).arr("im_sus", &[-3.0, 0.0, 0.0]).done("Tight Low Punch", "Short, punchy lows."));
    v.push(mb(M::Impact).arr("im_att", &[0.0, 1.5, 3.0]).done("Snap & Air", "Accentuates high-frequency attacks."));

    // ---------------------------------------------------------------- Imager
    v.push(mb(M::Imager).arr("ig_width", &[1.0, 1.05, 1.3, 1.5]).done("Wide Highs", "Widens the top end."));
    v.push(mb(M::Imager).arr("ig_width", &[0.0, 1.0, 1.25, 1.4]).done("Mono Bass, Wide Top", "Centred lows with a wide top."));
    v.push(mb(M::Imager).arr("ig_width", &[1.0, 1.1, 1.15, 1.2]).done("Subtle Widen", "Barely noticeable extra width."));
    v.push(mb(M::Imager).arr("ig_width", &[0.6, 0.7, 0.7, 0.7]).done("Narrow for Mono Check", "Narrows the mix for mono compatibility."));
    v.push(mb(M::Imager).arr("ig_width", &[0.0, 1.0, 1.4, 1.6]).s("stz", 0.6).done("Stereoize Mono Source", "Adds artificial width to near-mono material."));
    v.push(mb(M::Imager).arr("ig_width", &[1.0, 0.8, 1.2, 1.3]).done("Centre Focus", "Narrower mids so a lead stays in the centre."));
    v.push(mb(M::Imager).arr("ig_width", &[0.0, 0.0, 0.0, 0.0]).done("Full Mono", "Sums everything to mono."));

    // ---------------------------------------------------------------- Vintage Limiter
    v.push(mb(M::VintageLimiter).s("in", 2.0).s("att", 0.4).s("rel", 120.0).s("char", 0.2).done("Soft Peak Taming", "Light peak control."));
    v.push(mb(M::VintageLimiter).s("in", 6.0).s("out", -0.5).s("att", 0.05).s("rel", 60.0).s("char", 0.5).done("Fast Clip-ish", "Fast and edgy."));
    v.push(mb(M::VintageLimiter).s("in", 5.0).s("att", 0.2).s("rel", 120.0).s("char", 0.5).done("Fairchild-ish", "Smooth vintage limiting."));
    v.push(mb(M::VintageLimiter).s("att", 0.5).s("rel", 200.0).s("char", 0.0).done("Clean Safety", "Transparent safety limiting."));

    // ---------------------------------------------------------------- Maximizer
    v.push(mb(M::Maximizer).s("mode", 0.0).s("gain", 2.0).s("ceil", -1.0).s("rel", 200.0).done("Transparent -1 dBTP", "Clean limiting for dynamic music."));
    v.push(mb(M::Maximizer).s("mode", 1.0).s("gain", 3.5).s("ceil", -1.0).s("rel", 150.0).done("Streaming Master", "Balanced loudness for streaming."));
    v.push(mb(M::Maximizer).s("mode", 2.0).s("gain", 6.5).s("ceil", -0.8).s("rel", 110.0).done("Loud & Clean", "Multiband loudness."));
    v.push(mb(M::Maximizer).s("mode", 3.0).s("gain", 9.0).s("ceil", -0.4).s("rel", 90.0).s("char", 0.3).done("Club Loud", "Aggressive and loud."));
    v.push(mb(M::Maximizer).s("mode", 4.0).s("gain", 5.0).s("ceil", -0.7).s("rel", 120.0).done("Rock Adaptive", "Release adapts to the music."));
    v.push(mb(M::Maximizer).s("mode", 3.0).s("gain", 11.0).s("ceil", -0.3).s("rel", 80.0).s("char", 0.5).done("Maximum Impact", "As loud as it goes."));
    v.push(mb(M::Maximizer).s("mode", 0.0).s("gain", 1.0).s("ceil", -1.5).s("rel", 200.0).done("Broadcast -1.5 dBTP", "Conservative broadcast ceiling."));

    // ---------------------------------------------------------------- Pitch Correction
    v.push(mb(M::PitchCorrect).s("mode", 0.0).s("retune", 120.0).s("preserve", 0.9).s("tol", 12.0).done("Vocal - Natural", "Transparent vocal tuning."));
    v.push(mb(M::PitchCorrect).s("mode", 0.0).s("retune", 40.0).s("preserve", 0.6).s("tol", 4.0).done("Vocal - Tight Pop", "Fast modern pop tuning."));
    v.push(mb(M::PitchCorrect).s("mode", 0.0).s("retune", 0.0).s("preserve", 0.0).s("tol", 0.0).s("maxshift", 400.0).done("Vocal - Hard Tune", "Instant robotic snapping."));
    v.push(mb(M::PitchCorrect).s("mode", 3.0).s("retune", 150.0).s("amount", 0.8).s("preserve", 0.9).s("tol", 14.0).done("Vocal in Full Mix - Gentle", "Estimates the lead vocal in a mix and tunes only that."));
    v.push(mb(M::PitchCorrect).s("mode", 1.0).s("retune", 150.0).s("preserve", 0.9).s("tol", 14.0).done("Instrument - Gentle", "Light intonation fix for a monophonic line."));
    v.push(mb(M::PitchCorrect).s("mode", 1.0).s("retune", 220.0).s("amount", 0.8).s("preserve", 0.95).s("tol", 15.0).s("minhz", 50.0).s("maxhz", 400.0).done("Bass - Intonation", "Low-range tracking for bass."));
    v.push(mb(M::PitchCorrect).s("mode", 2.0).s("tauto", 1.0).done("Full Song - Auto Tune to 440", "Moves the whole song to A=440 when it is detuned."));
    v.push(mb(M::PitchCorrect).s("mode", 2.0).s("tauto", 0.0).s("tmanual", 31.8).done("Full Song - 432 to 440", "Raises a 432 Hz master to 440 Hz."));
    v.push(mb(M::PitchCorrect).s("mode", 2.0).s("tauto", 0.0).s("tmanual", -31.8).done("Full Song - 440 to 432", "Lowers a 440 Hz master to 432 Hz."));
    v.push(mb(M::PitchCorrect).s("mode", 0.0).s("scale", 1.0).s("retune", 100.0).s("preserve", 0.88).s("tol", 10.0).done("Vocal - Major Scale (set key)", "Corrects to a major scale; choose the key."));
    v.push(mb(M::PitchCorrect).s("mode", 0.0).s("scale", 2.0).s("retune", 100.0).s("preserve", 0.88).s("tol", 10.0).done("Vocal - Minor Scale (set key)", "Corrects to a natural minor scale; choose the key."));

    // ---------------------------------------------------------------- Clarity
    v.push(mb(M::Clarity).s("amount", 0.4).s("freq", 2000.0).s("harm", 0.2).s("mix", 1.0).done("Natural Detail", "Lifts quiet detail in the upper mids and highs."));
    v.push(mb(M::Clarity).s("amount", 0.7).s("freq", 3000.0).s("harm", 0.35).s("warm", 0.0).done("Open & Airy", "Strong detail lift with a little harmonic air."));
    v.push(mb(M::Clarity).s("amount", 0.5).s("freq", 1500.0).s("harm", 0.15).s("warm", 0.4).done("Warm Clarity", "Detail plus low-shelf warmth."));
    v.push(mb(M::Clarity).s("amount", 0.3).s("freq", 5000.0).s("harm", 0.4).done("Air Only", "Only the very top is brought out."));
    v.push(mb(M::Clarity).s("amount", 0.6).s("freq", 1200.0).s("harm", 0.0).s("warm", -0.3).done("Vocal Presence", "Detail focused on the vocal range, a little less low-end warmth."));
    v.push(mb(M::Clarity).s("amount", 0.25).s("freq", 2500.0).s("harm", 0.1).s("mix", 0.6).done("Subtle Parallel", "Gentle effect blended in parallel."));

    // ---------------------------------------------------------------- Codec Preview
    v.push(mb(M::CodecPreview).s("fmt", 0.0).s("kbps", 128.0).done("MP3-like 128 kbps", "Hear a typical streaming-quality lossy encode."));
    v.push(mb(M::CodecPreview).s("fmt", 0.0).s("kbps", 320.0).done("MP3-like 320 kbps", "Highest-quality MP3-style encode."));
    v.push(mb(M::CodecPreview).s("fmt", 1.0).s("kbps", 96.0).done("AAC-like 96 kbps", "Low-bitrate AAC-style encode."));
    v.push(mb(M::CodecPreview).s("fmt", 1.0).s("kbps", 256.0).done("AAC-like 256 kbps", "High-quality AAC-style encode."));
    v.push(mb(M::CodecPreview).s("fmt", 2.0).s("kbps", 64.0).done("Opus-like 64 kbps", "Voice/streaming-style Opus encode."));
    v.push(mb(M::CodecPreview).s("fmt", 0.0).s("kbps", 64.0).s("delta", 1.0).done("Hear the Difference (64 kbps)", "Plays only what a 64 kbps encode would remove."));
    v.push(mb(M::CodecPreview).s("fmt", 0.0).s("kbps", 32.0).done("Worst Case 32 kbps", "Extreme low bitrate to expose weaknesses."));

    v
}

pub fn factory_for(m: ModuleId) -> Vec<ModulePreset> {
    factory().into_iter().filter(|p| p.module as usize == m.index()).collect()
}

// ------------------------------------------------------------------------------ applying

fn module_ptrs(params: &AirZoneParams, m: ModuleId) -> Vec<(String, ParamPtr)> {
    let prefix = module_prefix(m);
    params.param_map().into_iter().filter(|(id, _, _)| id.starts_with(prefix)).map(|(id, ptr, _)| (id, ptr)).collect()
}

fn set_gui(ptr: ParamPtr, norm: f32, setter: &ParamSetter) {
    unsafe {
        setter.raw_context.raw_begin_set_parameter(ptr);
        setter.raw_context.raw_set_parameter_normalized(ptr, norm.clamp(0.0, 1.0));
        setter.raw_context.raw_end_set_parameter(ptr);
    }
}

/// Reset a module's parameters to defaults (the on/off switch is left as it is).
pub fn reset_module_gui(m: ModuleId, params: &AirZoneParams, setter: &ParamSetter) {
    for (_id, ptr) in module_ptrs(params, m) {
        let d = unsafe { ptr.default_normalized_value() };
        set_gui(ptr, d, setter);
    }
}

/// Apply a module preset: defaults for the module, then the preset values, then the switch on.
pub fn apply_module_gui(p: &ModulePreset, params: &AirZoneParams, setter: &ParamSetter) {
    let Some(m) = p.module_id() else { return };
    let given: HashMap<&str, f32> = p.values.iter().map(|(k, v)| (k.as_str(), *v)).collect();
    for (id, ptr) in module_ptrs(params, m) {
        let norm = match given.get(id.as_str()) {
            Some(plain) => unsafe { ptr.preview_normalized(*plain) },
            None => unsafe { ptr.default_normalized_value() },
        };
        set_gui(ptr, norm, setter);
    }
    let map = param_lookup(params);
    if let Some(sw) = map.get(module_switch(m)) {
        set_gui(*sw, 1.0, setter);
    }
}

/// Capture the current values of one module as a user preset.
pub fn capture_module(m: ModuleId, name: &str, params: &AirZoneParams) -> ModulePreset {
    let values = module_ptrs(params, m).into_iter().map(|(id, ptr)| (id, unsafe { ptr.unmodulated_plain_value() })).collect();
    ModulePreset { module: m.index() as u8, name: name.into(), description: "User preset".into(), values, user: true }
}

// ------------------------------------------------------------------------------ user storage

fn module_dir(m: ModuleId) -> Option<PathBuf> {
    user_dir().map(|d| d.join("Modules").join(m.name().replace(' ', "_")))
}

fn safe_name(name: &str) -> String {
    name.chars().map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' { c } else { '_' }).collect()
}

pub fn save_user_module(p: &ModulePreset) -> Result<PathBuf, String> {
    let m = p.module_id().ok_or("unknown module")?;
    let dir = module_dir(m).ok_or("no user directory available")?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{}.azmodpreset", safe_name(&p.name)));
    let json = serde_json::to_string_pretty(p).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(path)
}

pub fn load_user_modules(m: ModuleId) -> Vec<ModulePreset> {
    let Some(dir) = module_dir(m) else { return Vec::new() };
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut out: Vec<ModulePreset> = rd
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |x| x == "azmodpreset"))
        .filter_map(|e| std::fs::read_to_string(e.path()).ok())
        .filter_map(|s| serde_json::from_str::<ModulePreset>(&s).ok())
        .map(|mut p| {
            p.user = true;
            p.module = m.index() as u8;
            p
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

pub fn delete_user_module(m: ModuleId, name: &str) {
    if let Some(dir) = module_dir(m) {
        let _ = std::fs::remove_file(dir.join(format!("{}.azmodpreset", safe_name(name))));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airzone_dsp::chain::Engine;
    use std::sync::Arc;

    #[test]
    fn every_module_has_presets_and_names_are_unique_per_module() {
        let all = factory();
        for m in ModuleId::ALL {
            let list: Vec<&ModulePreset> = all.iter().filter(|p| p.module as usize == m.index()).collect();
            assert!(list.len() >= 3, "{} has only {} presets", m.name(), list.len());
            let mut names: Vec<&str> = list.iter().map(|p| p.name.as_str()).collect();
            names.sort();
            names.dedup();
            assert_eq!(names.len(), list.len(), "duplicate names in {}", m.name());
        }
        assert!(all.len() >= 100, "only {} module presets", all.len());
    }

    #[test]
    fn ids_exist_belong_to_the_module_and_are_in_range() {
        let params = AirZoneParams::default();
        let map = param_lookup(&params);
        let mut bad = Vec::new();
        for p in factory() {
            let m = p.module_id().unwrap();
            for (id, v) in &p.values {
                if !id.starts_with(module_prefix(m)) {
                    bad.push(format!("{}: {id} is not a {} parameter", p.name, m.name()));
                    continue;
                }
                match map.get(id) {
                    None => bad.push(format!("{}: unknown id {id}", p.name)),
                    Some(ptr) => {
                        let n = unsafe { ptr.preview_normalized(*v) };
                        let back = unsafe { ptr.preview_plain(n.clamp(0.0, 1.0)) };
                        if (back - v).abs() > 0.02 * v.abs().max(1.0) {
                            bad.push(format!("{}: {id}={v} clamps to {back}", p.name));
                        }
                    }
                }
            }
            assert!(map.contains_key(module_switch(m)), "missing switch for {}", m.name());
        }
        assert!(bad.is_empty(), "{:#?}", bad);
    }

    #[test]
    fn prefixes_cover_the_whole_param_map() {
        // every non-global parameter must belong to exactly one module prefix
        let params = AirZoneParams::default();
        for (id, _ptr, _g) in params.param_map() {
            if id.starts_with("on_") || matches!(id.as_str(), "bypass" | "in_gain" | "out_gain" | "dither" | "gmatch") {
                continue;
            }
            let hits = ModuleId::ALL.iter().filter(|m| id.starts_with(module_prefix(**m))).count();
            assert_eq!(hits, 1, "parameter {id} matches {hits} module prefixes");
        }
    }

    fn signal(n: usize) -> (Vec<f32>, Vec<f32>) {
        let mut rng = airzone_dsp::util::Rng(9);
        let mut l = vec![0.0; n];
        let mut r = vec![0.0; n];
        let mut lp = 0.0f32;
        for i in 0..n {
            lp += (rng.next_f32() * 0.3 - lp) * 0.2;
            let t = i as f32 / 48000.0;
            let beat = (t * 2.0).fract();
            let kick = (-beat * 18.0).exp() * (std::f32::consts::TAU * (50.0 + 80.0 * (-beat * 30.0).exp()) * t).sin();
            let voice: f32 = (1..=8).map(|h| (std::f32::consts::TAU * 220.0 * h as f32 * t).sin() / h as f32).sum::<f32>() * 0.12;
            l[i] = 0.35 * kick + 0.2 * (std::f32::consts::TAU * 55.0 * t).sin() + voice + lp * 0.4;
            r[i] = 0.35 * kick + 0.2 * (std::f32::consts::TAU * 55.0 * t).sin() + voice * 0.8 + lp * 0.35;
        }
        (l, r)
    }

    #[test]
    fn every_module_preset_renders_sanely_on_its_own() {
        let sr = 48000.0;
        let (src_l, src_r) = signal(sr as usize * 4);
        let params = Arc::new(AirZoneParams::default());
        for p in factory() {
            let vals = crate::sync::Values::from_preset(params.clone(), &p.as_preset());
            let mut e = Engine::new(sr, 512);
            crate::sync::apply(&vals, None, &mut e);
            e.refresh_latency();
            let mut l = src_l.clone();
            let mut r = src_r.clone();
            for c in 0..(l.len() / 512) {
                e.process(&mut l[c * 512..(c + 1) * 512], &mut r[c * 512..(c + 1) * 512]);
            }
            let tail = (sr as usize * 2)..(l.len() - 512);
            assert!(l[tail.clone()].iter().chain(r[tail.clone()].iter()).all(|x| x.is_finite()), "'{}' not finite", p.name);
            let peak = l[tail.clone()].iter().chain(r[tail.clone()].iter()).fold(0.0f32, |a, b| a.max(b.abs()));
            assert!(peak < 6.0, "'{}' peak {}", p.name, peak);
            assert!(peak > 0.005, "'{}' silent", p.name);
        }
    }
}

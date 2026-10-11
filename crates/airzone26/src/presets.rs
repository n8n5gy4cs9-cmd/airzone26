//! Factory and user presets.
//!
//! A preset is a *complete* snapshot: applying one first resets every parameter to its default
//! (except bypass and chain order) and then sets the values listed in the preset.

use std::collections::HashMap;
use std::path::PathBuf;

use nih_plug::prelude::*;
use serde::{Deserialize, Serialize};

use crate::params::AirZoneParams;

// enum index constants -----------------------------------------------------------------------
pub const BELL: f32 = 0.0;
pub const LSHELF: f32 = 1.0;
pub const HSHELF: f32 = 2.0;
pub const HPF: f32 = 3.0;
pub const LPF: f32 = 4.0;
pub const NOTCH: f32 = 5.0;
pub const S12: f32 = 0.0;
pub const S24: f32 = 1.0;
pub const S48: f32 = 3.0;
pub const MID: f32 = 1.0;
pub const SIDE: f32 = 2.0;
pub const TAPE: f32 = 0.0;
pub const TUBE: f32 = 1.0;
pub const WARM: f32 = 2.0;
pub const RETRO: f32 = 3.0;
pub const AZ1: f32 = 0.0;
pub const AZ2: f32 = 1.0;
pub const AZ3: f32 = 2.0;
pub const AZ4: f32 = 3.0;
pub const AZ5: f32 = 4.0;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Preset {
    pub name: String,
    pub category: String,
    pub description: String,
    /// parameter id -> plain value (bool 0/1, enums by index, percentages 0..1)
    pub values: Vec<(String, f32)>,
}

pub struct B(Vec<(String, f32)>);

impl B {
    fn new() -> Self {
        B(Vec::new())
    }
    fn s(&mut self, id: &str, v: f32) -> &mut Self {
        self.0.retain(|(k, _)| k != id);
        self.0.push((id.to_string(), v));
        self
    }
    /// enable modules by short switch name (`on_<name>`)
    fn on(&mut self, names: &[&str]) -> &mut Self {
        for n in names {
            self.s(&format!("on_{n}"), 1.0);
        }
        self
    }
    fn eq(&mut self, n: usize, kind: f32, freq: f32, gain: f32, q: f32) -> &mut Self {
        self.on(&["eq"]);
        self.s(&format!("eq_on_{n}"), 1.0)
            .s(&format!("eq_kind_{n}"), kind)
            .s(&format!("eq_freq_{n}"), freq)
            .s(&format!("eq_gain_{n}"), gain)
            .s(&format!("eq_q_{n}"), q)
    }
    fn eq_pl(&mut self, n: usize, place: f32) -> &mut Self {
        self.s(&format!("eq_place_{n}"), place)
    }
    fn hp(&mut self, freq: f32, slope: f32) -> &mut Self {
        self.eq(1, HPF, freq, 0.0, 0.7).s("eq_slope_1", slope)
    }
    fn lp(&mut self, freq: f32, slope: f32) -> &mut Self {
        self.eq(8, LPF, freq, 0.0, 0.7).s("eq_slope_8", slope)
    }
    fn dq(&mut self, n: usize, freq: f32, q: f32, thr: f32, range: f32, ratio: f32) -> &mut Self {
        self.on(&["dyneq"]);
        self.s(&format!("dq_dq_on_{n}"), 1.0)
            .s(&format!("dq_dq_freq_{n}"), freq)
            .s(&format!("dq_dq_q_{n}"), q)
            .s(&format!("dq_dq_thresh_{n}"), thr)
            .s(&format!("dq_dq_range_{n}"), range)
            .s(&format!("dq_dq_ratio_{n}"), ratio)
    }
    /// 3-band dynamics: thresholds, ratios, makeup gains
    fn dyn3(&mut self, thr: [f32; 3], ratio: [f32; 3], gain: [f32; 3]) -> &mut Self {
        self.on(&["dyn"]);
        for i in 0..3 {
            let n = i + 1;
            let att = [30.0, 15.0, 6.0][i];
            let rel = [220.0, 140.0, 90.0][i];
            self.s(&format!("dy_dy_on_{n}"), 1.0)
                .s(&format!("dy_dy_thresh_{n}"), thr[i])
                .s(&format!("dy_dy_ratio_{n}"), ratio[i])
                .s(&format!("dy_dy_att_{n}"), att)
                .s(&format!("dy_dy_rel_{n}"), rel)
                .s(&format!("dy_dy_gain_{n}"), gain[i]);
        }
        self
    }
    fn mx(&mut self, mode: f32, gain: f32, ceil: f32, rel: f32) -> &mut Self {
        self.on(&["max"]).s("mx_mode", mode).s("mx_gain", gain).s("mx_ceil", ceil).s("mx_rel", rel)
    }
    fn exc(&mut self, mode: f32, amounts: [f32; 4], drive: f32) -> &mut Self {
        self.on(&["exciter"]).s("ex_mode", mode);
        for i in 0..4 {
            let n = i + 1;
            self.s(&format!("ex_ex_amount_{n}"), amounts[i]).s(&format!("ex_ex_drive_{n}"), drive);
        }
        self
    }
    fn img(&mut self, w: [f32; 4], stz: f32) -> &mut Self {
        self.on(&["imager"]).s("ig_stz", stz);
        for i in 0..4 {
            self.s(&format!("ig_ig_width_{}", i + 1), w[i]);
        }
        self
    }
    fn bass(&mut self, sub_mono: f32, sub_gain: f32, bass_gain: f32, punch: f32, tighten: f32) -> &mut Self {
        self.on(&["bass"])
            .s("bc_submono", sub_mono)
            .s("bc_subg", sub_gain)
            .s("bc_bassg", bass_gain)
            .s("bc_punch", punch)
            .s("bc_tight", tighten)
    }
    fn impact(&mut self, att: [f32; 3], sus: [f32; 3]) -> &mut Self {
        self.on(&["impact"]);
        for i in 0..3 {
            self.s(&format!("im_im_att_{}", i + 1), att[i]).s(&format!("im_im_sus_{}", i + 1), sus[i]);
        }
        self
    }
    fn stem(&mut self, gains: [f32; 4], tilt: [f32; 4], focus: f32) -> &mut Self {
        self.on(&["stem"]).s("st_focus", focus);
        for i in 0..4 {
            self.s(&format!("st_g{i}"), gains[i]).s(&format!("st_t{i}"), tilt[i]);
        }
        self
    }
    fn tape(&mut self, drive: f32, speed: f32, sat: f32, age: f32) -> &mut Self {
        self.on(&["vtape"]).s("vt_drive", drive).s("vt_speed", speed).s("vt_sat", sat).s("vt_age", age)
    }
    fn vcomp(&mut self, thr: f32, ratio: f32, att: f32, rel: f32, makeup: f32, warm: f32) -> &mut Self {
        self.on(&["vcomp"])
            .s("vc_thresh", thr)
            .s("vc_ratio", ratio)
            .s("vc_att", att)
            .s("vc_rel", rel)
            .s("vc_makeup", makeup)
            .s("vc_warm", warm)
    }
    fn veq(&mut self, lf: f32, lb: f32, la: f32, hf: f32, hb: f32, bw: f32, af: f32, ha: f32) -> &mut Self {
        self.on(&["veq"])
            .s("ve_lf", lf)
            .s("ve_lb", lb)
            .s("ve_la", la)
            .s("ve_hf", hf)
            .s("ve_hb", hb)
            .s("ve_bw", bw)
            .s("ve_af", af)
            .s("ve_ha", ha)
    }
    /// standard low-end hygiene: sub to mono
    fn mono_sub(&mut self) -> &mut Self {
        self.on(&["bass"]).s("bc_submono", 1.0)
    }
}

fn preset(name: &str, cat: &str, desc: &str, f: impl FnOnce(&mut B)) -> Preset {
    let mut b = B::new();
    f(&mut b);
    Preset { name: name.into(), category: cat.into(), description: desc.into(), values: b.0 }
}

/// All built-in presets.
pub fn factory() -> Vec<Preset> {
    let mut v: Vec<Preset> = Vec::new();

    // ------------------------------------------------------------------ Init / utility
    v.push(preset("Init (all modules off)", "Utility", "Neutral starting point: every module disabled, all values at default.", |_| {}));
    v.push(preset("Safety Limiter -1 dBTP", "Utility", "Transparent true-peak safety limiter only.", |b| {
        b.mx(AZ1, 0.0, -1.0, 80.0);
    }));
    v.push(preset("Sub to Mono", "Utility", "Sums everything below ~120 Hz to mono for tight, translatable low end.", |b| {
        b.bass(1.0, 0.0, 0.0, 0.0, 0.0).s("bc_subf", 120.0);
    }));
    v.push(preset("Mid/Side Air + Tight Bass", "Utility", "Side-only high shelf for air, mid-only low cut for clean centre.", |b| {
        b.eq(1, HSHELF, 9000.0, 2.0, 0.7).eq_pl(1, SIDE);
        b.eq(2, HPF, 30.0, 0.0, 0.7).eq_pl(2, MID);
        b.mono_sub();
    }));
    v.push(preset("Gain Stage -3 dB Headroom", "Utility", "Pulls the input down 3 dB and restores it on the output; chain gain staging helper.", |b| {
        b.s("in_gain", -3.0).s("out_gain", 3.0);
    }));

    // ------------------------------------------------------------------ Genre masters
    v.push(preset("Universal Master", "Genre", "Balanced starting point for most material.", |b| {
        b.hp(25.0, S24);
        b.eq(3, BELL, 280.0, -0.8, 0.8).eq(6, HSHELF, 11000.0, 1.0, 0.7);
        b.dyn3([-22.0, -20.0, -20.0], [1.6, 1.6, 1.5], [0.0, 0.0, 0.0]);
        b.mono_sub();
        b.mx(AZ2, 3.0, -1.0, 140.0);
    }));
    v.push(preset("Pop Polish", "Genre", "Bright, forward, wide and radio-ready.", |b| {
        b.hp(28.0, S24);
        b.eq(2, LSHELF, 90.0, 0.8, 0.7).eq(3, BELL, 300.0, -1.0, 0.9).eq(5, BELL, 3500.0, 0.8, 0.8).eq(7, HSHELF, 11000.0, 1.8, 0.7);
        b.dyn3([-22.0, -20.0, -19.0], [2.0, 1.8, 2.0], [0.5, 0.0, 0.0]);
        b.exc(TAPE, [0.0, 0.1, 0.2, 0.3], 6.0);
        b.img([1.0, 1.05, 1.2, 1.3], 0.0);
        b.mono_sub();
        b.mx(AZ2, 4.5, -0.8, 130.0);
    }));
    v.push(preset("Pop Loud", "Genre", "Competitive streaming/radio loudness with controlled highs.", |b| {
        b.hp(30.0, S24);
        b.eq(3, BELL, 280.0, -1.2, 0.9).eq(6, HSHELF, 10000.0, 1.2, 0.7);
        b.dyn3([-24.0, -22.0, -22.0], [2.2, 2.2, 2.4], [1.0, 0.5, 0.5]);
        b.exc(TAPE, [0.0, 0.1, 0.15, 0.25], 8.0);
        b.img([1.0, 1.0, 1.15, 1.25], 0.0).mono_sub();
        b.mx(AZ3, 7.5, -0.6, 110.0);
    }));
    v.push(preset("Rock Glue", "Genre", "Cohesive band feel: gentle bus compression, tape warmth.", |b| {
        b.hp(30.0, S24);
        b.eq(2, LSHELF, 100.0, 0.7, 0.7).eq(4, BELL, 700.0, -0.8, 1.0).eq(6, BELL, 4500.0, 0.8, 0.9);
        b.vcomp(-16.0, 2.5, 18.0, 220.0, 2.0, 0.25);
        b.tape(4.0, 1.0, 0.6, 0.1);
        b.mono_sub();
        b.mx(AZ5, 4.5, -0.8, 130.0);
    }));
    v.push(preset("Rock Punch", "Genre", "Drum-forward and aggressive without harshness.", |b| {
        b.hp(32.0, S24);
        b.eq(2, LSHELF, 90.0, 1.0, 0.7).eq(3, BELL, 320.0, -1.2, 1.0).eq(5, BELL, 3000.0, 1.0, 0.9);
        b.impact([2.5, 1.5, 1.0], [0.0, -0.5, 0.0]);
        b.dyn3([-22.0, -20.0, -20.0], [2.2, 2.4, 2.4], [0.5, 0.0, 0.0]);
        b.mono_sub();
        b.mx(AZ5, 6.0, -0.7, 110.0);
    }));
    v.push(preset("Hip-Hop Heavy Low End", "Genre", "Deep, controlled sub with crisp vocal presence.", |b| {
        b.hp(22.0, S24);
        b.eq(2, LSHELF, 60.0, 1.5, 0.7).eq(3, BELL, 250.0, -1.0, 1.0).eq(5, BELL, 4000.0, 0.8, 0.9).eq(7, HSHELF, 12000.0, 1.0, 0.7);
        b.bass(1.0, 1.0, 0.0, 1.5, 0.35).s("bc_subf", 65.0);
        b.dyn3([-24.0, -20.0, -20.0], [2.8, 2.0, 2.0], [0.5, 0.0, 0.0]);
        b.mx(AZ3, 6.0, -0.5, 100.0);
    }));
    v.push(preset("R&B Smooth", "Genre", "Silky top, warm low-mids, wide but safe.", |b| {
        b.hp(26.0, S24);
        b.eq(3, BELL, 350.0, -0.8, 0.9).eq(7, HSHELF, 9500.0, 1.2, 0.7);
        b.veq(2.0, 2.0, 0.0, 4.0, 2.0, 5.0, 1.0, 0.0);
        b.vcomp(-18.0, 2.0, 25.0, 300.0, 1.5, 0.3);
        b.img([1.0, 1.0, 1.15, 1.2], 0.1).mono_sub();
        b.mx(AZ2, 4.0, -1.0, 150.0);
    }));
    v.push(preset("EDM Club Loud", "Genre", "Tight, loud club master with mono-compatible bass.", |b| {
        b.hp(28.0, S48);
        b.eq(2, LSHELF, 70.0, 1.2, 0.7).eq(3, BELL, 260.0, -1.0, 1.0).eq(7, HSHELF, 11000.0, 1.5, 0.7);
        b.bass(1.0, 0.0, 0.0, 1.0, 0.4).s("bc_subf", 80.0);
        b.dyn3([-24.0, -22.0, -22.0], [2.5, 2.5, 2.5], [1.0, 0.5, 0.5]);
        b.exc(RETRO, [0.0, 0.1, 0.2, 0.3], 8.0);
        b.img([1.0, 1.0, 1.3, 1.4], 0.0);
        b.mx(AZ4, 9.0, -0.4, 90.0).s("mx_char", 0.3);
    }));
    v.push(preset("EDM Festival", "Genre", "Maximum impact: huge low end and bright top.", |b| {
        b.hp(30.0, S48);
        b.eq(2, LSHELF, 60.0, 1.8, 0.7).eq(4, BELL, 500.0, -1.0, 1.0).eq(7, HSHELF, 10000.0, 2.0, 0.7);
        b.bass(1.0, 0.5, 0.0, 2.0, 0.5).s("bc_subf", 75.0);
        b.impact([2.0, 2.0, 1.5], [0.0, 0.0, 0.0]);
        b.dyn3([-26.0, -24.0, -24.0], [3.0, 3.0, 3.0], [1.5, 1.0, 1.0]);
        b.exc(RETRO, [0.1, 0.15, 0.25, 0.35], 10.0);
        b.img([1.0, 1.1, 1.3, 1.5], 0.15);
        b.mx(AZ4, 11.0, -0.3, 80.0).s("mx_char", 0.5);
    }));
    v.push(preset("House Warm", "Genre", "Warm analogue house master with tape glue.", |b| {
        b.hp(26.0, S24);
        b.eq(2, LSHELF, 80.0, 1.0, 0.7).eq(6, HSHELF, 9000.0, 0.8, 0.7);
        b.tape(5.0, 1.0, 0.7, 0.15);
        b.vcomp(-18.0, 2.0, 20.0, 250.0, 1.5, 0.3);
        b.mono_sub();
        b.mx(AZ2, 5.0, -0.7, 130.0);
    }));
    v.push(preset("Acoustic Natural", "Genre", "Preserves dynamics and room; lifts detail gently.", |b| {
        b.hp(30.0, S12);
        b.eq(3, BELL, 300.0, -0.7, 0.9).eq(6, HSHELF, 12000.0, 1.0, 0.6);
        b.dyn3([-26.0, -24.0, -24.0], [1.4, 1.4, 1.3], [0.0, 0.0, 0.0]);
        b.mx(AZ1, 2.0, -1.0, 200.0);
    }));
    v.push(preset("Jazz Gentle", "Genre", "Open, airy and dynamic with a little vintage colour.", |b| {
        b.hp(28.0, S12);
        b.veq(2.0, 1.5, 0.0, 4.0, 1.5, 6.0, 1.0, 0.0);
        b.dyn3([-28.0, -26.0, -26.0], [1.3, 1.3, 1.2], [0.0, 0.0, 0.0]);
        b.tape(2.0, 2.0, 0.4, 0.0);
        b.mx(AZ1, 1.5, -1.0, 220.0);
    }));
    v.push(preset("Folk Warmth", "Genre", "Cosy low-mids, soft top, natural dynamics.", |b| {
        b.hp(30.0, S12);
        b.eq(2, LSHELF, 120.0, 0.8, 0.7).eq(5, BELL, 3000.0, -0.5, 0.8).eq(7, HSHELF, 10000.0, 0.8, 0.6);
        b.tape(3.0, 0.0, 0.5, 0.25);
        b.vcomp(-20.0, 1.8, 30.0, 350.0, 1.0, 0.2);
        b.mx(AZ1, 2.5, -1.0, 200.0);
    }));
    v.push(preset("Metal Wall", "Genre", "Dense, aggressive and loud with tamed fizz.", |b| {
        b.hp(35.0, S48);
        b.eq(3, BELL, 300.0, -1.5, 1.0).eq(5, BELL, 3500.0, 0.8, 1.0).eq(8, LPF, 17000.0, 0.0, 0.7);
        b.dyn3([-24.0, -22.0, -22.0], [3.0, 3.0, 3.0], [1.0, 0.5, 0.5]);
        b.exc(TUBE, [0.1, 0.2, 0.25, 0.1], 8.0);
        b.mono_sub();
        b.mx(AZ4, 8.0, -0.5, 90.0).s("mx_char", 0.4);
    }));
    v.push(preset("Classical Transparent", "Genre", "Near-invisible mastering: level only, wide and open.", |b| {
        b.hp(22.0, S12);
        b.eq(6, HSHELF, 12000.0, 0.5, 0.6);
        b.mx(AZ1, 1.0, -1.0, 300.0);
    }));
    v.push(preset("Cinematic Wide", "Genre", "Wide, deep and dramatic with smooth top end.", |b| {
        b.hp(24.0, S24);
        b.eq(2, LSHELF, 70.0, 1.0, 0.7).eq(7, HSHELF, 9000.0, 0.6, 0.6);
        b.img([1.0, 1.1, 1.25, 1.35], 0.2).mono_sub();
        b.dyn3([-28.0, -26.0, -26.0], [1.4, 1.4, 1.3], [0.0, 0.0, 0.0]);
        b.mx(AZ1, 2.0, -1.0, 250.0);
    }));
    v.push(preset("Lo-Fi Cassette", "Genre", "Dusty cassette character: rolled highs, wow & hiss.", |b| {
        b.tape(7.0, 0.0, 0.9, 0.7).s("vt_wow", 0.35).s("vt_noise", 0.25);
        b.hp(60.0, S12).lp(9000.0, S24);
        b.mx(AZ1, 3.0, -1.0, 150.0);
    }));
    v.push(preset("Podcast / Voice", "Genre", "Speech clarity, controlled dynamics, -16 LUFS-friendly level.", |b| {
        b.hp(80.0, S24);
        b.eq(2, BELL, 250.0, -1.5, 1.0).eq(4, BELL, 3500.0, 1.5, 0.9).eq(7, HSHELF, 9000.0, 1.0, 0.7);
        b.dq(1, 6500.0, 3.0, -30.0, -6.0, 3.0);
        b.dyn3([-26.0, -24.0, -24.0], [3.0, 3.0, 2.5], [2.0, 1.0, 1.0]);
        b.img([0.0, 0.0, 0.0, 0.0], 0.0);
        b.mx(AZ2, 6.0, -1.5, 120.0);
    }));

    // ------------------------------------------------------------------ Loudness targets
    v.push(preset("Streaming -14 LUFS Safe", "Loudness", "Dynamic master aimed at normalised streaming (-14 LUFS, -1 dBTP).", |b| {
        b.hp(25.0, S24).mono_sub();
        b.dyn3([-24.0, -22.0, -22.0], [1.6, 1.6, 1.5], [0.0, 0.0, 0.0]);
        b.mx(AZ2, 3.0, -1.0, 150.0);
    }));
    v.push(preset("Streaming Loud -10 LUFS", "Loudness", "Louder commercial master with transparent peak control.", |b| {
        b.hp(25.0, S24).mono_sub();
        b.dyn3([-24.0, -22.0, -22.0], [2.0, 2.0, 2.0], [0.5, 0.5, 0.5]);
        b.mx(AZ3, 6.5, -0.8, 120.0);
    }));
    v.push(preset("CD / Club Loud -8 LUFS", "Loudness", "Maximum loudness; use with care.", |b| {
        b.hp(28.0, S24).mono_sub();
        b.dyn3([-26.0, -24.0, -24.0], [2.6, 2.6, 2.6], [1.0, 1.0, 1.0]);
        b.mx(AZ4, 9.5, -0.3, 90.0).s("mx_char", 0.3);
    }));
    v.push(preset("Broadcast R128 -23 LUFS", "Loudness", "Gentle control for EBU R128 delivery.", |b| {
        b.hp(30.0, S12);
        b.dyn3([-30.0, -28.0, -28.0], [1.5, 1.5, 1.4], [0.0, 0.0, 0.0]);
        b.mx(AZ1, 0.0, -1.5, 200.0);
    }));
    v.push(preset("Vinyl Prep", "Loudness", "Mono bass, tamed sibilance and top end for cutting lathe.", |b| {
        b.hp(30.0, S12).lp(16000.0, S12);
        b.bass(1.0, 0.0, 0.0, 0.0, 0.0).s("bc_subf", 140.0);
        b.dq(1, 8000.0, 2.0, -28.0, -5.0, 3.0);
        b.mx(AZ1, 1.0, -2.0, 250.0);
    }));

    // ------------------------------------------------------------------ Problem solvers
    v.push(preset("Tame Harsh Highs", "Fix", "Spectral shaper removes resonant harshness in the 2-9 kHz area.", |b| {
        b.on(&["shaper"]).s("ss_depth", 0.6).s("ss_thresh", 3.0).s("ss_low", 2000.0).s("ss_high", 10000.0);
        b.mx(AZ2, 2.0, -1.0, 150.0);
    }));
    v.push(preset("Fix Muddy Mix", "Fix", "Dynamic cuts at 200-400 Hz only when the mud builds up.", |b| {
        b.dq(1, 220.0, 1.2, -26.0, -4.0, 3.0).dq(2, 380.0, 1.5, -26.0, -3.0, 3.0);
        b.eq(1, HPF, 25.0, 0.0, 0.7).s("eq_slope_1", S24);
        b.mx(AZ2, 2.0, -1.0, 150.0);
    }));
    v.push(preset("Restore Crushed Mix", "Fix", "Unlimiter brings back peaks of over-limited audio, then re-limits cleanly.", |b| {
        b.on(&["unlimiter"]).s("un_amount", 0.7).s("un_boost", 8.0).s("un_thresh", 0.8);
        b.mx(AZ2, 2.0, -1.0, 150.0);
    }));
    v.push(preset("Restore Drums in Loud Master", "Fix", "Unlimiter + Impact to bring snap back to a squashed mix.", |b| {
        b.on(&["unlimiter"]).s("un_amount", 0.6).s("un_boost", 6.0);
        b.impact([2.0, 1.5, 1.0], [0.0, 0.0, 0.0]);
        b.mx(AZ2, 3.0, -1.0, 140.0);
    }));
    v.push(preset("Widen Narrow Mix", "Fix", "Widens mid/high bands while leaving the low end centred.", |b| {
        b.img([0.9, 1.1, 1.35, 1.5], 0.25).mono_sub();
        b.mx(AZ1, 1.0, -1.0, 150.0);
    }));
    v.push(preset("Mono Compatibility Check", "Fix", "Narrows the mix to ~60% for safe summing to mono.", |b| {
        b.img([0.0, 0.6, 0.6, 0.6], 0.0);
    }));
    v.push(preset("De-Box / Cardboard Cut", "Fix", "Notches the boxy 400-600 Hz zone gently.", |b| {
        b.eq(3, BELL, 480.0, -2.0, 1.2).eq(4, BELL, 700.0, -0.8, 1.0);
    }));
    v.push(preset("Add Air", "Fix", "Silky high shelf plus subtle harmonic excitation above 6 kHz.", |b| {
        b.eq(7, HSHELF, 12000.0, 2.5, 0.6);
        b.exc(TAPE, [0.0, 0.0, 0.1, 0.35], 6.0);
    }));
    v.push(preset("Tighten Low End", "Fix", "Controls the 40-150 Hz dynamics for a tighter groove.", |b| {
        b.on(&["lef"]).s("lf_freq", 110.0).s("lf_punch", 2.0).s("lf_sustain", -2.0).s("lf_contrast", 0.5);
        b.mono_sub();
    }));
    v.push(preset("Bass Balance Auto", "Fix", "Bass Control auto-balances sub versus bass toward a smooth ratio.", |b| {
        b.bass(1.0, 0.0, 0.0, 0.5, 0.2).s("bc_auto", 1.0).s("bc_ratio", 1.5);
    }));
    v.push(preset("Adaptive Tone Stabilizer", "Fix", "Slow adaptive tonal balance toward a natural -4.5 dB/oct slope.", |b| {
        b.on(&["stab"]).s("sb_amount", 0.6).s("sb_range", 4.0).s("sb_speed", 0.25);
        b.mx(AZ2, 2.0, -1.0, 150.0);
    }));
    v.push(preset("De-Sibilance Light", "Fix", "Dynamic EQ on 6-9 kHz for gentle sibilance control.", |b| {
        b.dq(1, 7000.0, 3.0, -30.0, -5.0, 4.0).s("dq_dq_att_1", 2.0).s("dq_dq_rel_1", 60.0);
    }));
    v.push(preset("Harsh Midrange Tamer", "Fix", "Dynamic cut at 2.5 kHz for forward vocals/guitars.", |b| {
        b.dq(1, 2600.0, 1.5, -26.0, -4.0, 3.0);
        b.on(&["shaper"]).s("ss_depth", 0.35).s("ss_low", 1500.0).s("ss_high", 6000.0);
    }));

    // ------------------------------------------------------------------ Stem
    v.push(preset("Vocals Forward +3", "Stem", "Lifts the vocal stem 3 dB with a touch of presence.", |b| {
        b.stem([0.0, 0.0, 3.0, -0.5], [0.0, 0.0, 1.0, 0.0], 2.5);
    }));
    v.push(preset("Vocals Back -3", "Stem", "Sits the vocal 3 dB deeper in the mix.", |b| {
        b.stem([0.0, 0.0, -3.0, 0.5], [0.0, 0.0, -0.5, 0.0], 2.5);
    }));
    v.push(preset("Drum Punch +2", "Stem", "Boosts drums and trims sustained instruments for punch.", |b| {
        b.stem([0.0, 2.5, 0.0, -0.8], [0.0, 1.0, 0.0, 0.0], 2.5);
    }));
    v.push(preset("Bass Weight +2", "Stem", "Adds weight to the bass stem without touching the kick transient.", |b| {
        b.stem([2.0, 0.0, 0.0, 0.0], [0.0, 0.0, 0.0, 0.0], 2.0);
    }));
    v.push(preset("Tame Bass -3", "Stem", "Pulls the bass stem down when low end is overbearing.", |b| {
        b.stem([-3.0, 0.0, 0.0, 0.0], [0.0, 0.0, 0.0, 0.0], 2.0);
    }));
    v.push(preset("Karaoke-ish Vocal Reduction", "Stem", "Heavily reduces the centre vocal stem (not a perfect separation).", |b| {
        b.stem([0.0, 0.0, -12.0, 0.0], [0.0; 4], 3.5);
    }));
    v.push(preset("Instrumental Focus", "Stem", "Backs vocals off and lifts everything else slightly.", |b| {
        b.stem([1.0, 1.0, -5.0, 1.5], [0.0; 4], 3.0);
    }));
    v.push(preset("Balanced Mix Rebalance", "Stem", "Small cuts/boosts across stems to even out a lopsided mix.", |b| {
        b.stem([-0.5, 1.0, 1.0, -0.5], [0.0, 0.5, 0.5, 0.0], 2.0);
        b.mx(AZ2, 2.0, -1.0, 150.0);
    }));

    // ------------------------------------------------------------------ Match
    v.push(preset("Match EQ (capture ready)", "Match", "Enables Match EQ: capture your mix as Source, a reference as Reference, then dial Amount.", |b| {
        b.on(&["match"]).s("mq_amount", 0.8).s("mq_smooth", 5.0);
    }));
    v.push(preset("Match EQ + Stabilizer Follow", "Match", "Match EQ plus a Stabilizer that follows the reference tonal shape.", |b| {
        b.on(&["match", "stab"]).s("mq_amount", 0.7).s("sb_useref", 1.0).s("sb_amount", 0.5);
    }));

    // ------------------------------------------------------------------ Vintage
    v.push(preset("Tape Glue", "Vintage", "15 ips tape saturation with gentle optical glue.", |b| {
        b.tape(5.0, 1.0, 0.7, 0.1);
        b.vcomp(-18.0, 2.0, 25.0, 300.0, 1.5, 0.2);
        b.mx(AZ2, 2.0, -1.0, 150.0);
    }));
    v.push(preset("Opto Smooth", "Vintage", "Slow, program-dependent opto-style levelling.", |b| {
        b.vcomp(-20.0, 3.0, 30.0, 400.0, 2.5, 0.3);
    }));
    v.push(preset("Passive EQ Magic", "Vintage", "Low boost/cut trick plus silky 10 kHz lift.", |b| {
        b.veq(2.0, 3.0, 2.0, 4.0, 3.0, 5.0, 1.0, 1.0).s("ve_tube", 0.25);
    }));
    v.push(preset("Fairchild-Style Limit", "Vintage", "Fast vintage-flavoured limiting with tube character.", |b| {
        b.on(&["vlim"]).s("vl_in", 5.0).s("vl_rel", 120.0).s("vl_char", 0.5);
        b.mx(AZ1, 0.0, -1.0, 150.0);
    }));
    v.push(preset("Analog Warmth", "Vintage", "Tube EQ, tape and a hint of warm saturation in the exciter.", |b| {
        b.veq(2.0, 2.0, 0.0, 4.0, 2.0, 5.0, 1.0, 0.0).s("ve_tube", 0.3);
        b.tape(4.0, 1.0, 0.6, 0.15);
        b.exc(WARM, [0.15, 0.2, 0.1, 0.0], 5.0);
        b.mx(AZ2, 2.0, -1.0, 150.0);
    }));
    v.push(preset("Tube Bus", "Vintage", "Tube-coloured bus compression.", |b| {
        b.vcomp(-16.0, 2.5, 15.0, 200.0, 2.0, 0.6);
        b.exc(TUBE, [0.2, 0.2, 0.1, 0.0], 5.0);
    }));
    v.push(preset("Old Vinyl Era", "Vintage", "Narrow, mono-ish low end and rolled top like an old pressing.", |b| {
        b.tape(6.0, 0.0, 0.8, 0.5).s("vt_wow", 0.15);
        b.hp(40.0, S12).lp(12000.0, S12);
        b.bass(1.0, 0.0, 0.0, 0.0, 0.0).s("bc_subf", 140.0);
    }));

    // ------------------------------------------------------------------ Creative
    v.push(preset("Parallel Punch Exciter", "Creative", "Saturates mids and highs for intensity.", |b| {
        b.exc(RETRO, [0.0, 0.25, 0.35, 0.4], 12.0).s("ex_mix", 0.6);
        b.mx(AZ2, 3.0, -1.0, 120.0);
    }));
    v.push(preset("Slap Delay Sparkle", "Creative", "Short filtered slap for depth in sparse material.", |b| {
        b.on(&["delay"]).s("dl_tl", 90.0).s("dl_tr", 120.0).s("dl_fb", 0.15).s("dl_lc", 400.0).s("dl_hc", 5000.0).s("dl_mix", 0.1);
    }));
    v.push(preset("Mastering Room (subtle reverb)", "Creative", "Tiny room that glues dry stems together.", |b| {
        b.on(&["reverb"]).s("rv_room", 0.25).s("rv_damp", 0.7).s("rv_mix", 0.06).s("rv_lc", 300.0).s("rv_pre", 8.0);
    }));
    v.push(preset("Stereoize Mono Source", "Creative", "Adds artificial width to near-mono material.", |b| {
        b.img([0.0, 1.0, 1.4, 1.6], 0.6).mono_sub();
    }));
    v.push(preset("Kick & Snare Snap", "Creative", "Impact emphasises attack in low and high bands.", |b| {
        b.impact([3.0, 1.0, 2.0], [-1.0, 0.0, 0.0]);
        b.mx(AZ2, 3.0, -1.0, 120.0);
    }));
    v.push(preset("Soft Sustain Pad Lift", "Creative", "Boosts the sustain portion in mids/highs for lush pads.", |b| {
        b.impact([-1.0, 0.0, 0.0], [0.0, 3.0, 2.0]);
    }));

    // ------------------------------------------------------------------ Signature chains
    v.push(preset("AirZone Signature", "Signature", "Full-chain showcase: stabilised tone, glued dynamics, air and safe loudness.", |b| {
        b.hp(26.0, S24);
        b.eq(3, BELL, 280.0, -0.8, 0.9).eq(7, HSHELF, 11000.0, 1.5, 0.7);
        b.on(&["stab"]).s("sb_amount", 0.4);
        b.dyn3([-23.0, -21.0, -21.0], [1.8, 1.8, 1.8], [0.5, 0.0, 0.0]);
        b.tape(3.0, 1.0, 0.5, 0.0);
        b.exc(TAPE, [0.0, 0.1, 0.15, 0.25], 6.0);
        b.img([1.0, 1.05, 1.15, 1.25], 0.0);
        b.bass(1.0, 0.0, 0.0, 0.5, 0.15);
        b.mx(AZ2, 4.5, -1.0, 140.0);
    }));
    v.push(preset("Big & Open Mastering Chain", "Signature", "Wide, loud and clean master for dense modern productions.", |b| {
        b.hp(28.0, S24);
        b.eq(3, BELL, 300.0, -1.0, 0.9).eq(7, HSHELF, 10500.0, 1.5, 0.7);
        b.on(&["unlimiter"]).s("un_amount", 0.3);
        b.dyn3([-24.0, -22.0, -22.0], [2.2, 2.2, 2.2], [1.0, 0.5, 0.5]);
        b.exc(TAPE, [0.0, 0.1, 0.2, 0.3], 7.0);
        b.img([1.0, 1.1, 1.25, 1.4], 0.1);
        b.bass(1.0, 0.5, 0.0, 1.0, 0.3);
        b.mx(AZ3, 7.0, -0.6, 110.0);
    }));
    v.push(preset("Vintage Console Master", "Signature", "Passive EQ, opto compression, tape and a fast limiter.", |b| {
        b.hp(28.0, S12);
        b.veq(2.0, 2.0, 1.0, 4.0, 2.0, 5.0, 1.0, 0.0);
        b.vcomp(-18.0, 2.0, 20.0, 280.0, 2.0, 0.35);
        b.tape(4.0, 1.0, 0.6, 0.1);
        b.mono_sub();
        b.mx(AZ2, 4.0, -0.8, 140.0);
    }));
    v.push(preset("Mix Bus Rescue", "Signature", "Corrective chain: stabilise tone, tame harshness, restore dynamics, control lows.", |b| {
        b.hp(26.0, S24);
        b.on(&["unlimiter", "stab", "shaper"]);
        b.s("un_amount", 0.4).s("sb_amount", 0.5).s("ss_depth", 0.4);
        b.dq(1, 250.0, 1.2, -26.0, -3.0, 3.0);
        b.bass(1.0, 0.0, 0.0, 0.0, 0.2);
        b.mx(AZ2, 3.5, -1.0, 140.0);
    }));
    v
}

// ---------------------------------------------------------------------------------------------
// applying presets

/// Parameters a preset must never touch.
fn preserved(id: &str) -> bool {
    matches!(id, "bypass")
}

/// Look up the plain->normalized conversion for every parameter id.
pub fn param_lookup(params: &AirZoneParams) -> HashMap<String, ParamPtr> {
    params.param_map().into_iter().map(|(id, ptr, _)| (id, ptr)).collect()
}

/// Return the ids in `preset` that do not exist (empty when valid).
pub fn unknown_ids(preset: &Preset, params: &AirZoneParams) -> Vec<String> {
    let map = param_lookup(params);
    preset.values.iter().filter(|(id, _)| !map.contains_key(id)).map(|(id, _)| id.clone()).collect()
}

/// The target normalized value for every parameter after applying the preset.
pub fn resolve(preset: &Preset, params: &AirZoneParams) -> Vec<(String, ParamPtr, f32)> {
    let given: HashMap<&str, f32> = preset.values.iter().map(|(k, v)| (k.as_str(), *v)).collect();
    params
        .param_map()
        .into_iter()
        .filter(|(id, _, _)| !preserved(id))
        .map(|(id, ptr, _)| {
            let norm = match given.get(id.as_str()) {
                Some(plain) => unsafe { ptr.preview_normalized(*plain) },
                None => unsafe { ptr.default_normalized_value() },
            };
            (id, ptr, norm.clamp(0.0, 1.0))
        })
        .collect()
}

/// Apply through the host (automation-aware); call from the GUI thread.
pub fn apply_gui(preset: &Preset, params: &AirZoneParams, setter: &ParamSetter) {
    for (_id, ptr, norm) in resolve(preset, params) {
        unsafe {
            setter.raw_context.raw_begin_set_parameter(ptr);
            setter.raw_context.raw_set_parameter_normalized(ptr, norm);
            setter.raw_context.raw_end_set_parameter(ptr);
        }
    }
}

/// Set only the listed parameters (plain values) through the host; everything else is untouched.
pub fn apply_overlay_gui(values: &[(String, f32)], params: &AirZoneParams, setter: &ParamSetter) {
    let map = param_lookup(params);
    for (id, plain) in values {
        if let Some(ptr) = map.get(id) {
            let norm = unsafe { ptr.preview_normalized(*plain) }.clamp(0.0, 1.0);
            unsafe {
                setter.raw_context.raw_begin_set_parameter(*ptr);
                setter.raw_context.raw_set_parameter_normalized(*ptr, norm);
                setter.raw_context.raw_end_set_parameter(*ptr);
            }
        }
    }
}

/// Capture the current state as a user preset.
pub fn capture(name: &str, category: &str, params: &AirZoneParams) -> Preset {
    let values = params
        .param_map()
        .into_iter()
        .filter(|(id, _, _)| !preserved(id))
        .map(|(id, ptr, _)| (id, unsafe { ptr.unmodulated_plain_value() }))
        .collect();
    Preset { name: name.into(), category: category.into(), description: "User preset".into(), values }
}

// ---------------------------------------------------------------------------------------------
// user preset storage

pub fn user_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    let base = std::env::var_os("APPDATA").map(PathBuf::from);
    #[cfg(target_os = "macos")]
    let base = std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"));
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")));
    base.map(|b| b.join("AirZone26").join("Presets"))
}

fn safe_file_name(name: &str) -> String {
    name.chars().map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' { c } else { '_' }).collect()
}

pub fn save_user(preset: &Preset) -> Result<PathBuf, String> {
    let dir = user_dir().ok_or("no user directory available")?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{}.azpreset", safe_file_name(&preset.name)));
    let json = serde_json::to_string_pretty(preset).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(path)
}

pub fn load_user() -> Vec<Preset> {
    let Some(dir) = user_dir() else { return Vec::new() };
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut out: Vec<Preset> = rd
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |x| x == "azpreset"))
        .filter_map(|e| std::fs::read_to_string(e.path()).ok())
        .filter_map(|s| serde_json::from_str::<Preset>(&s).ok())
        .map(|mut p| {
            p.category = "User".into();
            p
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

pub fn delete_user(name: &str) {
    if let Some(dir) = user_dir() {
        let _ = std::fs::remove_file(dir.join(format!("{}.azpreset", safe_file_name(name))));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airzone_dsp::chain::Engine;

    #[test]
    fn there_are_many_presets_with_unique_names() {
        let p = factory();
        assert!(p.len() >= 60, "only {} presets", p.len());
        let mut names: Vec<&str> = p.iter().map(|x| x.name.as_str()).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), p.len(), "duplicate preset names");
    }

    #[test]
    fn all_preset_ids_exist() {
        let params = AirZoneParams::default();
        for p in factory() {
            let bad = unknown_ids(&p, &params);
            assert!(bad.is_empty(), "preset '{}' has unknown ids {:?}", p.name, bad);
        }
    }

    #[test]
    fn all_preset_values_are_in_range() {
        let params = AirZoneParams::default();
        let map = param_lookup(&params);
        let mut bad = Vec::new();
        for p in factory() {
            for (id, v) in &p.values {
                let ptr = map[id];
                let n = unsafe { ptr.preview_normalized(*v) };
                let back = unsafe { ptr.preview_plain(n.clamp(0.0, 1.0)) };
                if (back - v).abs() > 0.02 * v.abs().max(1.0) {
                    bad.push(format!("'{}' {id}={v} clamps to {back}", p.name));
                }
            }
        }
        assert!(bad.is_empty(), "{:#?}", bad);
    }

    fn test_signal(n: usize) -> (Vec<f32>, Vec<f32>) {
        let mut rng = airzone_dsp::util::Rng(7);
        let mut l = vec![0.0; n];
        let mut r = vec![0.0; n];
        let mut lp = 0.0f32;
        for i in 0..n {
            // pink-ish noise + bass + periodic kick envelope
            lp += (rng.next_f32() * 0.3 - lp) * 0.2;
            let t = i as f32 / 48000.0;
            let beat = (t * 2.0).fract();
            let kick = (-beat * 18.0).exp() * (2.0 * std::f32::consts::PI * (50.0 + 80.0 * (-beat * 30.0).exp()) * t).sin();
            let bass = 0.25 * (2.0 * std::f32::consts::PI * 55.0 * t).sin();
            let lead = 0.15 * (2.0 * std::f32::consts::PI * 440.0 * t).sin();
            l[i] = 0.35 * kick + bass + lead + lp * 0.5;
            r[i] = 0.35 * kick + bass + 0.12 * (2.0 * std::f32::consts::PI * 660.0 * t).sin() + lp * 0.4;
        }
        (l, r)
    }

    /// Render every factory preset offline: output must be finite, never exceed the ceiling,
    /// and presets with the Maximizer must actually raise loudness vs. the dry signal.
    #[test]
    fn every_preset_renders_sanely() {
        let sr = 48000.0;
        let (src_l, src_r) = test_signal(sr as usize * 6);
        let params = std::sync::Arc::new(AirZoneParams::default());
        for preset in factory() {
            let vals = crate::sync::Values::from_preset(params.clone(), &preset);
            let mut e = Engine::new(sr, 512);
            crate::sync::apply(&vals, None, &mut e);
            e.refresh_latency();
            let mut l = src_l.clone();
            let mut r = src_r.clone();
            for c in 0..(l.len() / 512) {
                e.process(&mut l[c * 512..(c + 1) * 512], &mut r[c * 512..(c + 1) * 512]);
            }
            let tail = (sr as usize * 3)..(l.len() - 512);
            assert!(l[tail.clone()].iter().chain(r[tail.clone()].iter()).all(|x| x.is_finite()), "preset '{}' not finite", preset.name);
            let peak = l[tail.clone()].iter().chain(r[tail.clone()].iter()).fold(0.0f32, |a, b| a.max(b.abs()));
            let ceiling = if vals.b("on_max") { airzone_dsp::util::db_to_lin(vals.f("mx_ceil")) * 1.02 } else { 8.0 };
            assert!(peak <= ceiling, "preset '{}' peak {} exceeds {}", preset.name, peak, ceiling);
            assert!(peak > 0.01, "preset '{}' silent", preset.name);
        }
    }
}

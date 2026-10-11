//! Master Assistant window: listen, analyse, review explained suggestions, apply / undo.

use std::sync::atomic::Ordering;

use airzone_dsp::assistant::{suggest, Analysis, AssistantOptions, Genre, LoudnessTarget, Suggestion};
use airzone_dsp::biquad::FilterKind;
use airzone_dsp::chain::{ModuleId, MODULE_COUNT};
use airzone_dsp::maximizer::LimiterMode;
use nih_plug::prelude::*;
use nih_plug_egui::egui::{self, RichText};

use super::widgets::*;
use crate::params::AirZoneParams;
use crate::presets::{self, Preset};
use crate::shared::*;

pub struct AssistUi {
    pub open: bool,
    pub genre: usize,
    pub target: usize,
    pub custom_lufs: f32,
    pub custom_ceiling: f32,
    pub allow: [bool; MODULE_COUNT],
    pub listening: bool,
    pub result: Option<(Analysis, Suggestion)>,
    pub backup: Option<Preset>,
    pub applied: bool,
}

impl Default for AssistUi {
    fn default() -> Self {
        Self {
            open: false,
            genre: 0,
            target: 0,
            custom_lufs: -12.0,
            custom_ceiling: -1.0,
            allow: AssistantOptions::default().allow,
            listening: false,
            result: None,
            backup: None,
            applied: false,
        }
    }
}

fn target_of(a: &AssistUi) -> LoudnessTarget {
    match a.target {
        0 => LoudnessTarget::Streaming,
        1 => LoudnessTarget::Loud,
        2 => LoudnessTarget::Broadcast,
        _ => LoudnessTarget::Custom { lufs: a.custom_lufs, ceiling_db: a.custom_ceiling },
    }
}

/// Turn a suggestion into parameter assignments (plain values).
pub fn suggestion_values(s: &Suggestion) -> Vec<(String, f32)> {
    let mut v: Vec<(String, f32)> = Vec::new();
    let mut set = |id: String, x: f32| v.push((id, x));
    if !s.eq.is_empty() || s.highpass_hz.is_some() {
        set("on_eq".into(), 1.0);
    }
    if let Some(hz) = s.highpass_hz {
        set("eq_on_1".into(), 1.0);
        set("eq_kind_1".into(), 3.0);
        set("eq_freq_1".into(), hz);
        set("eq_slope_1".into(), 1.0);
    }
    for (i, e) in s.eq.iter().take(6).enumerate() {
        let n = i + 2;
        let kind = match e.kind {
            FilterKind::LowShelf => 1.0,
            FilterKind::HighShelf => 2.0,
            _ => 0.0,
        };
        set(format!("eq_on_{n}"), 1.0);
        set(format!("eq_kind_{n}"), kind);
        set(format!("eq_freq_{n}"), e.freq);
        set(format!("eq_gain_{n}"), e.gain_db);
        set(format!("eq_q_{n}"), e.q);
    }
    if let Some((thr, ratio)) = s.dynamics {
        set("on_dyn".into(), 1.0);
        for n in 1..=3 {
            set(format!("dy_dy_on_{n}"), 1.0);
            set(format!("dy_dy_thresh_{n}"), thr);
            set(format!("dy_dy_ratio_{n}"), ratio);
        }
    }
    if let Some(a) = s.exciter_high_amount {
        set("on_exciter".into(), 1.0);
        set("ex_ex_amount_4".into(), a);
        set("ex_ex_amount_3".into(), a * 0.5);
    }
    if let Some(w) = s.imager_high_width {
        set("on_imager".into(), 1.0);
        set("ig_ig_width_3".into(), w);
        set("ig_ig_width_4".into(), w);
    }
    if s.bass_sub_mono {
        set("on_bass".into(), 1.0);
        set("bc_submono".into(), 1.0);
        if let Some(g) = s.bass_sub_gain_db {
            set("bc_subg".into(), g);
        }
    }
    if s.stabilizer {
        set("on_stab".into(), 1.0);
    }
    if let Some(a) = s.unlimiter_amount {
        set("on_unlimiter".into(), 1.0);
        set("un_amount".into(), a);
    }
    if let Some(g) = s.maximizer_gain_db {
        set("on_max".into(), 1.0);
        set("mx_gain".into(), g);
        if let Some(c) = s.ceiling_db {
            set("mx_ceil".into(), c);
        }
        if let Some(m) = s.limiter_mode {
            let idx = LimiterMode::ALL.iter().position(|x| *x == m).unwrap_or(1);
            set("mx_mode".into(), idx as f32);
        }
    }
    v
}

pub fn show(ctx: &egui::Context, a: &mut AssistUi, params: &AirZoneParams, sh: &Shared, setter: &ParamSetter) {
    if !a.open {
        return;
    }
    // collect a finished analysis
    if a.listening && sh.assist_cmd.load(Ordering::Relaxed) == ASSIST_IDLE {
        if let Some(res) = sh.assist_result.lock().take() {
            a.listening = false;
            let opts = AssistantOptions { genre: Genre::ALL[a.genre], target: target_of(a), allow: a.allow };
            let sug = suggest(&res, &opts);
            a.result = Some((res, sug));
            a.applied = false;
        }
    }
    let mut open = a.open;
    egui::Window::new("Master Assistant").open(&mut open).default_width(560.0).show(ctx, |ui| {
        ui.label(RichText::new("The assistant listens to your programme, measures it and suggests settings. It guides — you decide: review the notes, then apply or undo.").color(TEXT_DIM).size(11.0));
        ui.separator();
        ui.horizontal(|ui| {
            ui.label("Genre");
            egui::ComboBox::from_id_salt("as_genre").selected_text(Genre::ALL[a.genre].name()).show_ui(ui, |ui| {
                for (i, g) in Genre::ALL.iter().enumerate() {
                    ui.selectable_value(&mut a.genre, i, g.name());
                }
            });
            ui.label("Target");
            let names = ["Streaming −14 LUFS", "Loud −9 LUFS", "Broadcast −23 LUFS", "Custom"];
            egui::ComboBox::from_id_salt("as_target").selected_text(names[a.target]).show_ui(ui, |ui| {
                for (i, n) in names.iter().enumerate() {
                    ui.selectable_value(&mut a.target, i, *n);
                }
            });
        });
        if a.target == 3 {
            ui.horizontal(|ui| {
                ui.add(egui::Slider::new(&mut a.custom_lufs, -30.0..=-5.0).text("LUFS"));
                ui.add(egui::Slider::new(&mut a.custom_ceiling, -3.0..=0.0).text("Ceiling dBTP"));
            });
        }
        ui.collapsing("Modules the assistant may adjust", |ui| {
            ui.horizontal_wrapped(|ui| {
                for m in ModuleId::ALL {
                    if matches!(m, ModuleId::Equalizer | ModuleId::Dynamics | ModuleId::Exciter | ModuleId::Imager | ModuleId::BassControl | ModuleId::Stabilizer | ModuleId::Unlimiter | ModuleId::Maximizer) {
                        ui.checkbox(&mut a.allow[m.index()], m.name());
                    }
                }
            });
        });
        ui.separator();
        ui.horizontal(|ui| {
            if !a.listening {
                if ui.button("🎧 Start listening").clicked() {
                    sh.assist_result.lock().take();
                    sh.assist_cmd.store(ASSIST_LISTEN, Ordering::Relaxed);
                    a.listening = true;
                    a.result = None;
                }
                ui.label("Play the loudest, most representative section of your song.");
            } else {
                let secs = Shared::load(&sh.assist_seconds);
                ui.label(format!("Listening… {:.1} s", secs));
                if ui.add_enabled(secs > 5.0, egui::Button::new("Analyze now")).clicked() {
                    sh.assist_cmd.store(ASSIST_FINISH, Ordering::Relaxed);
                }
                if ui.button("Cancel").clicked() {
                    sh.assist_cmd.store(ASSIST_FINISH, Ordering::Relaxed);
                    a.listening = false;
                }
            }
        });
        if let Some((res, sug)) = &a.result {
            ui.separator();
            ui.label(RichText::new("Measurements").strong().color(ACCENT));
            ui.label(format!(
                "Loudness {:.1} LUFS · LRA {:.1} LU · True peak {:.1} dBTP · Crest {:.1} dB · Width {:.0}% · Correlation {:+.2} · Sub−Bass {:+.1} dB · Analysed {:.0} s",
                res.lufs, res.lra, res.true_peak_db, res.crest_db, res.width * 100.0, res.correlation, res.sub_minus_bass_db, res.seconds
            ));
            ui.label(RichText::new("Suggestions").strong().color(ACCENT));
            egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                for n in &sug.notes {
                    ui.label(format!("• {n}"));
                }
            });
            ui.horizontal(|ui| {
                if ui.button("✔ Apply suggestions").clicked() {
                    a.backup = Some(presets::capture("before assistant", "User", params));
                    presets::apply_overlay_gui(&suggestion_values(sug), params, setter);
                    a.applied = true;
                }
                if ui.add_enabled(a.backup.is_some(), egui::Button::new("↶ Undo")).clicked() {
                    if let Some(b) = a.backup.take() {
                        presets::apply_gui(&b, params, setter);
                        a.applied = false;
                    }
                }
                if a.applied {
                    ui.label(RichText::new("Applied — fine-tune any module afterwards.").color(GREEN));
                }
            });
        }
    });
    a.open = open;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::AirZoneParams;

    #[test]
    fn suggestion_ids_are_valid() {
        let s = Suggestion {
            eq: vec![airzone_dsp::assistant::EqSuggestion { kind: FilterKind::Bell, freq: 300.0, gain_db: -1.0, q: 0.8 }; 6],
            highpass_hz: Some(24.0),
            dynamics: Some((-20.0, 2.0)),
            exciter_high_amount: Some(0.3),
            imager_high_width: Some(1.2),
            bass_sub_mono: true,
            bass_sub_gain_db: Some(-1.0),
            stabilizer: true,
            unlimiter_amount: Some(0.4),
            maximizer_gain_db: Some(4.0),
            ceiling_db: Some(-1.0),
            limiter_mode: Some(LimiterMode::Multiband),
            notes: vec![],
        };
        let vals = suggestion_values(&s);
        let preset = Preset { name: "t".into(), category: "t".into(), description: String::new(), values: vals };
        let params = AirZoneParams::default();
        assert!(presets::unknown_ids(&preset, &params).is_empty());
    }
}

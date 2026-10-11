//! Per-module editor panels.

use std::sync::atomic::Ordering;

use airzone_dsp::chain::ModuleId;
use airzone_dsp::eq::EqBand;
use nih_plug::prelude::*;
use nih_plug_egui::egui::{self, vec2, Align2, Color32, FontId, Pos2, Rect, RichText, Sense, Stroke, Ui};

use super::widgets::*;
use super::State;
use crate::params::*;
use crate::shared::*;

macro_rules! sl {
    ($ui:expr, $s:expr, $($p:expr),+ $(,)?) => { $( slider($ui, $s, &$p); )+ };
}

pub struct Ctx<'a> {
    pub p: &'a AirZoneParams,
    pub setter: &'a ParamSetter<'a>,
    pub sh: &'a Shared,
}

pub fn show(ui: &mut Ui, m: ModuleId, c: &Ctx, st: &mut State) {
    ui.horizontal_wrapped(|ui| {
        ui.heading(RichText::new(m.name()).color(ACCENT));
        let sw = c.p.on.by_index()[m.index()];
        toggle(ui, c.setter, sw);
        ui.label(RichText::new(description(m)).color(TEXT_DIM).size(11.0));
    });
    module_preset_bar(ui, m, c, st);
    ui.separator();
    match m {
        ModuleId::Unlimiter => unlimiter(ui, c),
        ModuleId::StemEq => stem(ui, c),
        ModuleId::MatchEq => match_eq(ui, c, st),
        ModuleId::Equalizer => equalizer(ui, c),
        ModuleId::DynamicEq => dyneq(ui, c),
        ModuleId::Stabilizer => stabilizer(ui, c),
        ModuleId::SpectralShaper => shaper(ui, c),
        ModuleId::BassControl => bass(ui, c),
        ModuleId::LowEndFocus => lef(ui, c),
        ModuleId::Dynamics => dynamics(ui, c),
        ModuleId::VintageComp => vcomp(ui, c),
        ModuleId::VintageEq => veq(ui, c),
        ModuleId::VintageTape => vtape(ui, c),
        ModuleId::Exciter => exciter(ui, c),
        ModuleId::Impact => impact(ui, c),
        ModuleId::Imager => imager(ui, c),
        ModuleId::Delay => delay(ui, c),
        ModuleId::Reverb => reverb(ui, c),
        ModuleId::VintageLimiter => vlim(ui, c),
        ModuleId::Maximizer => maximizer(ui, c),
        ModuleId::PitchCorrect => pitch(ui, c),
        ModuleId::Clarity => clarity(ui, c),
        ModuleId::CodecPreview => codec(ui, c),
    }
}

/// Preset menu, previous/next, reset and save for the module being edited.
fn module_preset_bar(ui: &mut Ui, m: ModuleId, c: &Ctx, st: &mut State) {
    use crate::module_presets as mp;
    let list = st.module_preset_list(m);
    let current = st.module_current(m);
    let mut chosen: Option<mp::ModulePreset> = None;
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("Presets").size(11.0).color(TEXT_DIM));
        let idx = list.iter().position(|p| p.name == current);
        if arrow_button(ui, Dir::Left).on_hover_text("Previous module preset").clicked() && !list.is_empty() {
            chosen = Some(list[idx.map(|i| (i + list.len() - 1) % list.len()).unwrap_or(0)].clone());
        }
        let label = if current.is_empty() { format!("{} presets ({})", m.name(), list.len()) } else { current.clone() };
        egui::ComboBox::from_id_salt(("modpreset", m.index())).selected_text(label).width(210.0).height(420.0).show_ui(ui, |ui| {
            for p in &list {
                let tag = if p.user { format!("{}  (user)", p.name) } else { p.name.clone() };
                if ui.selectable_label(p.name == current, tag).on_hover_text(&p.description).clicked() {
                    chosen = Some(p.clone());
                }
            }
        });
        if arrow_button(ui, Dir::Right).on_hover_text("Next module preset").clicked() && !list.is_empty() {
            chosen = Some(list[idx.map(|i| (i + 1) % list.len()).unwrap_or(0)].clone());
        }
        if ui.small_button("Reset").on_hover_text("Return this module to its default settings").clicked() {
            mp::reset_module_gui(m, c.p, c.setter);
            st.set_module_current(m, "");
        }
        ui.add(egui::TextEdit::singleline(&mut st.mod_save_name).desired_width(110.0).hint_text("preset name"));
        if ui.small_button("Save").on_hover_text("Save this module's settings as a user preset").clicked() && !st.mod_save_name.trim().is_empty() {
            let name = st.mod_save_name.trim().to_string();
            let p = mp::capture_module(m, &name, c.p);
            match mp::save_user_module(&p) {
                Ok(_) => {
                    st.forget_user_modules(m);
                    st.set_module_current(m, &name);
                    st.status = format!("Saved module preset '{}'", name);
                }
                Err(e) => st.status = format!("Save failed: {e}"),
            }
            st.mod_save_name.clear();
        }
    });
    if let Some(p) = chosen {
        mp::apply_module_gui(&p, c.p, c.setter);
        st.set_module_current(m, &p.name);
    }
}

pub fn description(m: ModuleId) -> &'static str {
    match m {
        ModuleId::Unlimiter => "Restores peaks and punch to over-limited audio (upward expansion).",
        ModuleId::StemEq => "DSP stem separation (bass / drums / vocals / other) with gain and tone per stem.",
        ModuleId::MatchEq => "Learn the tonal balance of a reference and match your master to it.",
        ModuleId::Equalizer => "8-band parametric EQ with mid/side/left/right band placement.",
        ModuleId::DynamicEq => "4 bands that act only when the signal crosses the threshold.",
        ModuleId::Stabilizer => "Slow adaptive EQ that keeps the tonal balance on target.",
        ModuleId::SpectralShaper => "Resonance suppression and spectral tilt in the frequency domain.",
        ModuleId::BassControl => "Balance, mono-fold and tighten sub and bass regions.",
        ModuleId::LowEndFocus => "Punch, sustain and contrast for the low end.",
        ModuleId::Dynamics => "3-band compressor / expander with M/S and stereo link.",
        ModuleId::VintageComp => "Opto-style compressor with program-dependent release.",
        ModuleId::VintageEq => "Passive-style EQ with the classic boost/cut trick.",
        ModuleId::VintageTape => "Tape machine: saturation, head bump, wow & flutter, age.",
        ModuleId::Exciter => "4-band harmonic exciter with anti-aliased saturation.",
        ModuleId::Impact => "3-band attack / sustain shaping.",
        ModuleId::Imager => "4-band stereo width, stereoize and balance.",
        ModuleId::Delay => "Stereo delay with ping-pong and filtered feedback.",
        ModuleId::Reverb => "Compact algorithmic reverb for glue and ambience.",
        ModuleId::VintageLimiter => "Fast, coloured peak limiter.",
        ModuleId::Maximizer => "Lookahead true-peak limiter with 5 algorithms.",
        ModuleId::PitchCorrect => "Natural pitch correction: vocals, instruments, or whole-song tuning.",
        ModuleId::Clarity => "Lifts quiet detail and air in the upper mids and highs without making loud parts harsh.",
        ModuleId::CodecPreview => "Hear roughly how your master will sound after lossy MP3 / AAC / Opus encoding (monitoring only).",
    }
}

fn unlimiter(ui: &mut Ui, c: &Ctx) {
    let p = &c.p.unlimiter;
    sl!(ui, c.setter, p.amount, p.max_boost, p.threshold, p.release);
    toggle(ui, c.setter, &p.compensate);
    bar(ui, "Boost", &format!("{:+.1} dB", Shared::load(&c.sh.gr[GR_UNLIM])), Shared::load(&c.sh.gr[GR_UNLIM]), 0.0, 12.0, ACCENT2, 220.0);
}

fn stem(ui: &mut Ui, c: &Ctx) {
    let p = &c.p.stem;
    ui.label(RichText::new("Stems are estimated with harmonic/percussive and centre-channel analysis. Great for small corrections (±6 dB); extreme settings will sound artificial.").color(TEXT_DIM).size(11.0));
    let names = ["Bass", "Drums", "Vocals", "Other"];
    let gains = [&p.bass_gain, &p.drums_gain, &p.vocals_gain, &p.other_gain];
    let tilts = [&p.bass_tilt, &p.drums_tilt, &p.vocals_tilt, &p.other_tilt];
    columns(ui, 4, 200.0, |i, ui| {
        ui.label(RichText::new(names[i]).strong().color(ACCENT));
        let e = Shared::load(&c.sh.stem_energy[i]);
        bar(ui, "Energy", &format!("{:.0}%", e * 100.0), e, 0.0, 1.0, ACCENT, 90.0);
        slider(ui, c.setter, gains[i]);
        slider(ui, c.setter, tilts[i]);
    });
    sl!(ui, c.setter, p.focus);
    combo(ui, c.setter, &p.solo, "stem_solo");
}

fn match_eq(ui: &mut Ui, c: &Ctx, st: &mut State) {
    let p = &c.p.match_eq;
    sl!(ui, c.setter, p.amount, p.smoothing, p.max_gain);
    ui.separator();
    let cap = c.sh.capture.load(Ordering::Relaxed);
    let src = c.sh.match_progress[0].load(Ordering::Relaxed);
    let rf = c.sh.match_progress[1].load(Ordering::Relaxed);
    ui.label(RichText::new("1. Play your mix and press Learn Source.  2. Play the reference (or load a WAV) and press Learn Reference.  3. Enable the module and set Amount.").color(TEXT_DIM).size(11.0));
    ui.horizontal(|ui| {
        let b = if cap == CAP_SOURCE { "Stop learning source" } else { "Learn Source" };
        if ui.button(b).clicked() {
            c.sh.capture.store(if cap == CAP_SOURCE { CAP_OFF } else { CAP_SOURCE }, Ordering::Relaxed);
        }
        let b = if cap == CAP_REFERENCE { "Stop learning reference" } else { "Learn Reference (live)" };
        if ui.button(b).clicked() {
            c.sh.capture.store(if cap == CAP_REFERENCE { CAP_OFF } else { CAP_REFERENCE }, Ordering::Relaxed);
        }
        if ui.button("Clear Source").clicked() {
            c.sh.clear_source.store(true, Ordering::Relaxed);
        }
        if ui.button("Clear Reference").clicked() {
            c.sh.clear_reference.store(true, Ordering::Relaxed);
        }
    });
    ui.label(format!(
        "Source: {} frames ({})   |   Reference: {} frames ({})   {}",
        src,
        if src > 8 { "ready" } else { "needs more" },
        rf,
        if rf > 8 { "ready" } else { "needs more" },
        c.p.match_data.lock().ref_name
    ));
    ui.horizontal(|ui| {
        ui.label("Reference WAV path:");
        ui.add(egui::TextEdit::singleline(&mut st.ref_path).desired_width(360.0).hint_text("C:\\Music\\reference.wav"));
        if ui.button("Analyze file").clicked() {
            st.start_reference_analysis();
        }
    });
    if !st.status.is_empty() {
        ui.label(RichText::new(&st.status).color(ACCENT2));
    }
    // correction curve
    let curve = c.sh.match_curve.lock().clone();
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width().min(760.0), (ui.available_height() * 0.35).clamp(90.0, 170.0)), Sense::hover());
    draw_freq_grid(ui, rect);
    let mid = rect.center().y;
    ui.painter_at(rect).line_segment([Pos2::new(rect.left(), mid), Pos2::new(rect.right(), mid)], Stroke::new(1.0_f32, TEXT_DIM));
    let pts: Vec<Pos2> = curve
        .iter()
        .enumerate()
        .map(|(i, db)| {
            let x = rect.left() + rect.width() * i as f32 / (curve.len().max(2) - 1) as f32;
            Pos2::new(x, mid - db * (rect.height() * 0.5 / 15.0))
        })
        .collect();
    if pts.len() > 1 {
        polyline(ui, rect, pts, ACCENT, 2.0);
    }
}

fn band_of(p: &EqBandP, sr: f32) -> (EqBand, f32) {
    use airzone_dsp::biquad::FilterKind;
    let kind = match p.kind.value() {
        KindP::Bell => FilterKind::Bell,
        KindP::LowShelf => FilterKind::LowShelf,
        KindP::HighShelf => FilterKind::HighShelf,
        KindP::HighPass => FilterKind::HighPass,
        KindP::LowPass => FilterKind::LowPass,
        KindP::Notch => FilterKind::Notch,
    };
    let order = 2 + 2 * p.slope.value().to_index();
    (EqBand { enabled: p.on.value(), kind, freq: p.freq.value(), gain_db: p.gain.value(), q: p.q.value(), order, ..Default::default() }, sr)
}

fn equalizer(ui: &mut Ui, c: &Ctx) {
    let sr = Shared::load(&c.sh.sample_rate).max(8000.0);
    let w = ui.available_width().min(1000.0);
    let (rect, _) = ui.allocate_exact_size(vec2(w, (ui.available_height() * 0.45).clamp(130.0, 240.0)), Sense::hover());
    draw_freq_grid(ui, rect);
    let range = 18.0;
    let y_of = |db: f32| rect.center().y - db * (rect.height() * 0.5 / range);
    let painter = ui.painter_at(rect);
    for db in [-12.0, -6.0, 0.0, 6.0, 12.0] {
        let y = y_of(db);
        painter.line_segment([Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)], Stroke::new(1.0_f32, if db == 0.0 { TEXT_DIM } else { BG2 }));
        painter.text(Pos2::new(rect.left() + 3.0, y - 10.0), Align2::LEFT_TOP, format!("{:+}", db), FontId::proportional(9.0), TEXT_DIM);
    }
    let bands: Vec<EqBand> = c.p.eq.bands.iter().map(|b| band_of(b, sr).0).collect();
    let n = 400;
    let pts: Vec<Pos2> = (0..n)
        .map(|i| {
            let x = rect.left() + rect.width() * i as f32 / (n - 1) as f32;
            let f = x_to_freq(x, rect);
            let db: f32 = bands.iter().map(|b| b.magnitude_db(sr, f)).sum();
            Pos2::new(x, y_of(db.clamp(-range, range)))
        })
        .collect();
    polyline(ui, rect, pts, ACCENT, 2.0);
    // draggable nodes
    for (i, bp) in c.p.eq.bands.iter().enumerate() {
        if !bp.on.value() {
            continue;
        }
        let b = &bands[i];
        let flat = matches!(bp.kind.value(), KindP::HighPass | KindP::LowPass);
        let pos = Pos2::new(freq_to_x(b.freq, rect), y_of(if flat { 0.0 } else { b.gain_db }));
        let node = Rect::from_center_size(pos, vec2(16.0, 16.0));
        let r = ui.interact(node, ui.id().with(("eqnode", i)), Sense::click_and_drag());
        painter.circle_filled(pos, if r.hovered() || r.dragged() { 7.0 } else { 5.5 }, ACCENT2);
        painter.text(pos + vec2(8.0, -14.0), Align2::LEFT_TOP, format!("{}", i + 1), FontId::proportional(10.0), Color32::WHITE);
        if r.drag_started() {
            c.setter.begin_set_parameter(&bp.freq);
            c.setter.begin_set_parameter(&bp.gain);
        }
        if r.dragged() {
            if let Some(pp) = r.interact_pointer_pos() {
                c.setter.set_parameter(&bp.freq, x_to_freq(pp.x, rect).clamp(20.0, 20000.0));
                if !flat {
                    let g = ((rect.center().y - pp.y) / (rect.height() * 0.5 / range)).clamp(-range, range);
                    c.setter.set_parameter(&bp.gain, g);
                }
            }
        }
        if r.drag_stopped() {
            c.setter.end_set_parameter(&bp.freq);
            c.setter.end_set_parameter(&bp.gain);
        }
        if r.hovered() {
            let scroll = ui.input(|i| i.raw_scroll_delta.y);
            if scroll != 0.0 {
                c.setter.begin_set_parameter(&bp.q);
                c.setter.set_parameter(&bp.q, (bp.q.value() * (1.0 + scroll * 0.002)).clamp(0.1, 12.0));
                c.setter.end_set_parameter(&bp.q);
            }
        }
    }
    ui.label(RichText::new("Drag nodes to set frequency/gain, scroll over a node to change Q.").color(TEXT_DIM).size(10.0));
    egui::ScrollArea::horizontal().id_salt("eqgrid_scroll").show(ui, |ui| {
    egui::Grid::new("eqgrid").num_columns(8).spacing([8.0, 3.0]).show(ui, |ui| {
        for h in ["Band", "On", "Type", "Freq", "Gain", "Q", "Slope", "Place"] {
            ui.label(RichText::new(h).color(TEXT_DIM).size(11.0));
        }
        ui.end_row();
        for (i, b) in c.p.eq.bands.iter().enumerate() {
            ui.label(format!("{}", i + 1));
            toggle_small(ui, c.setter, &b.on);
            combo_small(ui, c.setter, &b.kind, format!("eqk{i}"), 90.0);
            ui.add(nih_plug_egui::widgets::ParamSlider::for_param(&b.freq, c.setter).with_width(90.0));
            ui.add(nih_plug_egui::widgets::ParamSlider::for_param(&b.gain, c.setter).with_width(80.0));
            ui.add(nih_plug_egui::widgets::ParamSlider::for_param(&b.q, c.setter).with_width(60.0));
            combo_small(ui, c.setter, &b.slope, format!("eqs{i}"), 90.0);
            combo_small(ui, c.setter, &b.place, format!("eqp{i}"), 70.0);
            ui.end_row();
        }
    });
    });
}

fn toggle_small(ui: &mut Ui, setter: &ParamSetter, p: &BoolParam) {
    let mut v = p.value();
    if ui.checkbox(&mut v, "").changed() {
        setter.begin_set_parameter(p);
        setter.set_parameter(p, v);
        setter.end_set_parameter(p);
    }
}

fn dyneq(ui: &mut Ui, c: &Ctx) {
    for (i, b) in c.p.dyneq.bands.iter().enumerate() {
        ui.collapsing(format!("Band {}", i + 1), |ui| {
            toggle(ui, c.setter, &b.on);
            sl!(ui, c.setter, b.freq, b.q, b.gain, b.threshold, b.range, b.ratio, b.attack, b.release);
        });
    }
    ui.label(RichText::new("Negative Dynamic Range cuts when the band is loud (de-essing, de-muddying); positive boosts.").color(TEXT_DIM).size(11.0));
}

fn stabilizer(ui: &mut Ui, c: &Ctx) {
    let p = &c.p.stab;
    sl!(ui, c.setter, p.amount, p.range, p.slope, p.speed);
    toggle(ui, c.setter, &p.use_ref);
    ui.label(RichText::new("Applied correction per band").color(TEXT_DIM).size(11.0));
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width().min(760.0), 110.0), Sense::hover());
    draw_freq_grid(ui, rect);
    let mid = rect.center().y;
    let painter = ui.painter_at(rect);
    painter.line_segment([Pos2::new(rect.left(), mid), Pos2::new(rect.right(), mid)], Stroke::new(1.0_f32, TEXT_DIM));
    let n = c.sh.stab_curve.len();
    for i in 0..n {
        let g = Shared::load(&c.sh.stab_curve[i]);
        let x0 = rect.left() + rect.width() * i as f32 / n as f32;
        let x1 = rect.left() + rect.width() * (i + 1) as f32 / n as f32 - 2.0;
        let y = mid - g * (rect.height() * 0.5 / 9.0);
        painter.rect_filled(Rect::from_two_pos(Pos2::new(x0, mid), Pos2::new(x1, y)), 1.0, if g >= 0.0 { ACCENT } else { ACCENT2 });
    }
}

fn shaper(ui: &mut Ui, c: &Ctx) {
    let p = &c.p.shaper;
    sl!(ui, c.setter, p.depth, p.threshold, p.width, p.low, p.high, p.tilt);
    toggle(ui, c.setter, &p.delta);
}

fn bass(ui: &mut Ui, c: &Ctx) {
    let p = &c.p.bass;
    let lv = [Shared::load(&c.sh.bass_levels[0]), Shared::load(&c.sh.bass_levels[1]), Shared::load(&c.sh.bass_levels[2])];
    ui.label(RichText::new("Region levels").color(TEXT_DIM));
    bar(ui, "Sub", &format!("{:.1} dB", lv[0]), lv[0], -80.0, -10.0, ACCENT, 260.0);
    bar(ui, "Bass", &format!("{:.1} dB", lv[1]), lv[1], -80.0, -10.0, ACCENT, 260.0);
    bar(ui, "Rest", &format!("{:.1} dB", lv[2]), lv[2], -80.0, -10.0, TEXT_DIM, 260.0);
    ui.label(format!("Sub - Bass: {:+.1} dB", lv[0] - lv[1]));
    ui.separator();
    sl!(ui, c.setter, p.sub_freq, p.bass_freq, p.sub_gain, p.bass_gain, p.sub_mono, p.bass_mono, p.punch, p.tighten, p.target_ratio, p.mix);
    toggle(ui, c.setter, &p.auto_balance);
}

fn lef(ui: &mut Ui, c: &Ctx) {
    let p = &c.p.lef;
    sl!(ui, c.setter, p.freq, p.punch, p.sustain, p.contrast, p.mix);
}

fn dynamics(ui: &mut Ui, c: &Ctx) {
    let p = &c.p.dynamics;
    ui.horizontal(|ui| {
        for i in 0..3 {
            gr_meter(ui, &format!("B{}", i + 1), Shared::load(&c.sh.gr[GR_DYN_BASE + i]), 12.0);
        }
    });
    sl!(ui, c.setter, p.xo1, p.xo2, p.link, p.output, p.mix);
    toggle(ui, c.setter, &p.mid_side);
    columns(ui, 3, 230.0, |i, ui| {
        let b = &p.bands[i];
        ui.label(RichText::new(["Low", "Mid", "High"][i]).strong().color(ACCENT));
        toggle(ui, c.setter, &b.on);
        sl!(ui, c.setter, b.threshold, b.ratio, b.knee, b.attack, b.release, b.gain, b.exp_threshold, b.exp_ratio);
    });
}

fn vcomp(ui: &mut Ui, c: &Ctx) {
    let p = &c.p.vcomp;
    gr_meter(ui, "GR", Shared::load(&c.sh.gr[GR_VCOMP]), 12.0);
    sl!(ui, c.setter, p.input, p.threshold, p.ratio, p.attack, p.release, p.makeup, p.warmth, p.mix);
}

fn veq(ui: &mut Ui, c: &Ctx) {
    let p = &c.p.veq;
    combo(ui, c.setter, &p.low_freq, "ve_lf");
    sl!(ui, c.setter, p.low_boost, p.low_atten);
    combo(ui, c.setter, &p.high_freq, "ve_hf");
    sl!(ui, c.setter, p.high_boost, p.bandwidth);
    combo(ui, c.setter, &p.atten_freq, "ve_af");
    sl!(ui, c.setter, p.high_atten, p.tube);
}

fn vtape(ui: &mut Ui, c: &Ctx) {
    let p = &c.p.vtape;
    combo(ui, c.setter, &p.speed, "vt_speed");
    sl!(ui, c.setter, p.drive, p.saturation, p.wow, p.noise, p.age, p.output);
}

fn exciter(ui: &mut Ui, c: &Ctx) {
    let p = &c.p.exciter;
    combo(ui, c.setter, &p.mode, "ex_mode");
    sl!(ui, c.setter, p.xo1, p.xo2, p.xo3, p.mix);
    columns(ui, 4, 200.0, |i, ui| {
        let b = &p.bands[i];
        ui.label(RichText::new(format!("Band {}", i + 1)).strong().color(ACCENT));
        toggle(ui, c.setter, &b.on);
        sl!(ui, c.setter, b.drive, b.amount, b.gain);
    });
}

fn impact(ui: &mut Ui, c: &Ctx) {
    let p = &c.p.impact;
    sl!(ui, c.setter, p.xo1, p.xo2, p.mix);
    columns(ui, 3, 230.0, |i, ui| {
        ui.label(RichText::new(["Low", "Mid", "High"][i]).strong().color(ACCENT));
        sl!(ui, c.setter, p.bands[i].attack, p.bands[i].sustain);
    });
}

fn imager(ui: &mut Ui, c: &Ctx) {
    let p = &c.p.imager;
    let w = Shared::load(&c.sh.width);
    bar(ui, "Width", &format!("{:.0}%", w * 100.0), w, 0.0, 1.0, ACCENT, 220.0);
    let cor = Shared::load(&c.sh.correlation);
    bar(ui, "Corr.", &format!("{:+.2}", cor), (cor + 1.0) * 0.5, 0.0, 1.0, if cor < 0.0 { RED } else { GREEN }, 220.0);
    vectorscope(ui, c.sh);
    sl!(ui, c.setter, p.xo1, p.xo2, p.xo3, p.stereoize, p.balance);
    for i in 0..4 {
        let (lw, cw) = row_widths(ui);
        ui.horizontal(|ui| {
            ui.add_sized([lw, 18.0], egui::Label::new(RichText::new(format!("Band {} width", i + 1)).color(TEXT_DIM).size(12.0)).truncate());
            ui.add(nih_plug_egui::widgets::ParamSlider::for_param(&p.bands[i].width, c.setter).with_width(cw));
        });
    }
}

/// Mid/side vectorscope of the last few hundred output samples (vertical = mid, horizontal = side).
fn vectorscope(ui: &mut Ui, sh: &Shared) {
    let side = ui.available_width().min(190.0).max(100.0);
    let (rect, _) = ui.allocate_exact_size(vec2(side, side), Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 4.0, BG0);
    let ctr = rect.center();
    let half = side * 0.5 - 8.0;
    let grid = Stroke::new(1.0_f32, BG2);
    p.line_segment([Pos2::new(rect.left(), ctr.y), Pos2::new(rect.right(), ctr.y)], grid);
    p.line_segment([Pos2::new(ctr.x, rect.top()), Pos2::new(ctr.x, rect.bottom())], grid);
    p.line_segment([Pos2::new(ctr.x - half, ctr.y + half), Pos2::new(ctr.x + half, ctr.y - half)], grid);
    p.line_segment([Pos2::new(ctr.x - half, ctr.y - half), Pos2::new(ctr.x + half, ctr.y + half)], grid);
    let font = FontId::proportional(10.0);
    p.text(Pos2::new(ctr.x + 3.0, rect.top() + 2.0), Align2::LEFT_TOP, "M", font.clone(), TEXT_DIM);
    p.text(Pos2::new(rect.left() + 3.0, rect.top() + 2.0), Align2::LEFT_TOP, "L", font.clone(), TEXT_DIM);
    p.text(Pos2::new(rect.right() - 10.0, rect.top() + 2.0), Align2::LEFT_TOP, "R", font, TEXT_DIM);
    let n = sh.scope_l.len();
    for i in 0..n {
        let l = Shared::load(&sh.scope_l[i]);
        let r = Shared::load(&sh.scope_r[i]);
        let x = (ctr.x + (r - l) * 0.5 * half).clamp(rect.left() + 1.0, rect.right() - 1.0);
        let y = (ctr.y - (l + r) * 0.5 * half).clamp(rect.top() + 1.0, rect.bottom() - 1.0);
        let alpha = (40 + (i * 200) / n.max(1)).min(240) as u8;
        p.circle_filled(Pos2::new(x, y), 1.3, Color32::from_rgba_unmultiplied(64, 214, 190, alpha));
    }
}

fn delay(ui: &mut Ui, c: &Ctx) {
    let p = &c.p.delay;
    sl!(ui, c.setter, p.time_l, p.time_r, p.feedback, p.lowcut, p.highcut, p.mix);
    toggle(ui, c.setter, &p.ping_pong);
    toggle(ui, c.setter, &p.wet_solo);
}

fn reverb(ui: &mut Ui, c: &Ctx) {
    let p = &c.p.reverb;
    sl!(ui, c.setter, p.room, p.damping, p.width, p.predelay, p.lowcut, p.mix);
    toggle(ui, c.setter, &p.wet_solo);
    ui.label(RichText::new("Solo Wet removes the dry signal so you hear only the reverb. Remember to switch it off again before bouncing.").color(TEXT_DIM).size(11.0));
}

fn vlim(ui: &mut Ui, c: &Ctx) {
    let p = &c.p.vlimiter;
    gr_meter(ui, "GR", Shared::load(&c.sh.gr[GR_VLIM]), 12.0);
    sl!(ui, c.setter, p.input, p.output, p.attack, p.release, p.character);
}

fn maximizer(ui: &mut Ui, c: &Ctx) {
    let p = &c.p.max;
    ui.horizontal(|ui| {
        gr_meter(ui, "GR", Shared::load(&c.sh.gr[GR_MAXIMIZER]), 12.0);
        ui.vertical(|ui| {
            combo(ui, c.setter, &p.mode, "mx_mode");
            sl!(ui, c.setter, p.gain, p.ceiling, p.release, p.character, p.independence);
            toggle(ui, c.setter, &p.true_peak);
        });
    });
    ui.label(RichText::new("AZ-1 clean | AZ-2 program-dependent | AZ-3 3-band | AZ-4 4-band + saturation | AZ-5 crest-adaptive | AZ-6 soft clip + limit | AZ-7 smooth, long lookahead. Latency is reported to the host.").color(TEXT_DIM).size(11.0));
}

fn pitch(ui: &mut Ui, c: &Ctx) {
    let p = &c.p.pitch;
    combo(ui, c.setter, &p.mode, "pc_mode");
    let full = p.mode.value() == PitchModeP::FullSong;
    let mix = p.mode.value() == PitchModeP::MixVocal;
    if !c.p.on.pitch.value() {
        ui.colored_label(RED, "This module is switched OFF. Tick the check box next to the title (or in the chain list) to hear the correction.");
    } else if !full {
        let ratio = Shared::load(&c.sh.pitch_voiced_ratio);
        if ratio < 0.15 {
            ui.colored_label(
                ACCENT2,
                format!(
                    "No clear single pitch found yet (voiced {:.0}% of the time). Play a solo vocal or instrument, lower Voicing Confidence, or use 'Vocal in Full Mix' / 'Full Song' for complete mixes.",
                    ratio * 100.0
                ),
            );
        } else {
            ui.label(RichText::new(format!("Pitch found in {:.0}% of the audio; current correction {:+.0} cents.", ratio * 100.0, Shared::load(&c.sh.pitch_shift))).size(11.0).color(GREEN));
        }
    }
    if !full {
        let help = if mix {
            "Vocal in Full Mix: the lead vocal is estimated from the mix (harmonic + centre + voice-band analysis), tuned on its own, and added back to the rest of the mix. Separation is approximate: keep corrections moderate (Correction Amount 50-80%, slow Retune Speed). For a clean acapella use Vocal mode."
        } else {
            "Use this on a solo vocal or instrument (or a bounced stem), first in the chain. It tracks the pitch, chooses the nearest note of your scale and moves the pitch with a formant-preserving PSOLA engine. Slow retune speed and high Preserve Vibrato keep it natural; Retune Speed 0 gives the hard effect."
        };
        ui.label(RichText::new(help).color(TEXT_DIM).size(11.0));
        // tuner readout
        let voiced = c.sh.pitch_voiced.load(Ordering::Relaxed);
        let midi = Shared::load(&c.sh.pitch_midi);
        let target = Shared::load(&c.sh.pitch_target);
        let shift = Shared::load(&c.sh.pitch_shift);
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width().min(520.0).max(300.0), 70.0), Sense::hover());
            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, 6.0, BG0);
            let (txt, sub) = if voiced {
                let note = airzone_dsp::pitch::midi_name(midi.round() as i32);
                let dev = (midi - midi.round()) * 100.0;
                (note, format!("{:+.0} cents   {:.1} Hz", dev, Shared::load(&c.sh.pitch_f0)))
            } else {
                ("--".to_string(), "no pitch".to_string())
            };
            painter.text(Pos2::new(rect.left() + 14.0, rect.top() + 6.0), Align2::LEFT_TOP, txt, FontId::proportional(34.0), if voiced { ACCENT } else { TEXT_DIM });
            painter.text(Pos2::new(rect.left() + 14.0, rect.top() + 48.0), Align2::LEFT_TOP, sub, FontId::proportional(12.0), TEXT_DIM);
            if voiced {
                painter.text(
                    Pos2::new(rect.left() + 170.0, rect.top() + 6.0),
                    Align2::LEFT_TOP,
                    format!("-> {}", airzone_dsp::pitch::midi_name(target as i32)),
                    FontId::proportional(24.0),
                    ACCENT2,
                );
                painter.text(Pos2::new(rect.left() + 170.0, rect.top() + 40.0), Align2::LEFT_TOP, format!("shift {:+.0} cents", shift), FontId::proportional(12.0), TEXT_DIM);
            }
            // cents needle
            let cx = rect.right() - 120.0;
            let y = rect.center().y;
            painter.line_segment([Pos2::new(cx - 100.0, y), Pos2::new(cx + 100.0, y)], Stroke::new(1.0_f32, TEXT_DIM));
            painter.line_segment([Pos2::new(cx, y - 12.0), Pos2::new(cx, y + 12.0)], Stroke::new(1.0_f32, TEXT_DIM));
            if voiced {
                let dev = ((midi - target) * 100.0).clamp(-50.0, 50.0);
                painter.circle_filled(Pos2::new(cx + dev * 2.0, y), 6.0, RED);
                let out = (dev + shift).clamp(-50.0, 50.0);
                painter.circle_filled(Pos2::new(cx + out * 2.0, y), 5.0, GREEN);
            }
        });
        ui.label(RichText::new("Red dot = detected pitch, green dot = pitch after correction (relative to the target note)").color(TEXT_DIM).size(10.0));
        ui.separator();
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                combo(ui, c.setter, &p.root, "pc_root");
                combo(ui, c.setter, &p.scale, "pc_scale");
            });
            ui.vertical(|ui| {
                if ui.button("Detect key from audio").clicked() {
                    let mut kd = airzone_dsp::pitch::KeyDetector::default();
                    for i in 0..12 {
                        kd.hist[i] = Shared::load(&c.sh.key_hist[i]);
                    }
                    if let Some((key, corr)) = kd.best() {
                        c.setter.begin_set_parameter(&p.root);
                        c.setter.set_parameter(&p.root, NoteP::from_index(key.root as usize));
                        c.setter.end_set_parameter(&p.root);
                        c.setter.begin_set_parameter(&p.scale);
                        c.setter.set_parameter(&p.scale, if key.scale == airzone_dsp::pitch::ScaleKind::Major { ScaleP::Major } else { ScaleP::Minor });
                        c.setter.end_set_parameter(&p.scale);
                        let _ = corr;
                    }
                }
                if ui.button("Clear key memory").clicked() {
                    c.sh.clear_key_hist.store(true, Ordering::Relaxed);
                }
                let mut kd = airzone_dsp::pitch::KeyDetector::default();
                for i in 0..12 {
                    kd.hist[i] = Shared::load(&c.sh.key_hist[i]);
                }
                match kd.best() {
                    Some((k, r)) => ui.label(RichText::new(format!("Likely key: {} {} (r = {:.2})", airzone_dsp::pitch::NOTE_NAMES[k.root as usize], if k.scale == airzone_dsp::pitch::ScaleKind::Major { "major" } else { "minor" }, r)).size(11.0)),
                    None => ui.label(RichText::new("Play some singing/playing to learn the key.").color(TEXT_DIM).size(11.0)),
                };
            });
        });
        sl!(ui, c.setter, p.retune, p.amount, p.preserve, p.tolerance, p.max_shift, p.min_hz, p.max_hz, p.confidence, p.a4);
    } else {
        ui.label(RichText::new("Full-song mode corrects the overall tuning of a mixed track (for example a recording that sits 20 cents sharp, or a 432 Hz master you want at 440 Hz). It measures where the music sits against the 12-tone grid and gently shifts the whole spectrum - it does not retune individual notes. For single voices or instruments switch to Vocal or Instrument mode.").color(TEXT_DIM).size(11.0));
        let det = Shared::load(&c.sh.tune_detected);
        let app = Shared::load(&c.sh.tune_applied);
        let conf = Shared::load(&c.sh.tune_conf);
        bar(ui, "Detected", &format!("{:+.1} cents vs A=440", det), (det + 50.0) / 100.0, 0.0, 1.0, ACCENT, 260.0);
        bar(ui, "Applied", &format!("{:+.1} cents", app), (app + 100.0) / 200.0, 0.0, 1.0, ACCENT2, 260.0);
        bar(ui, "Confid.", &format!("{:.0}%", conf * 100.0), conf, 0.0, 1.0, GREEN, 260.0);
        toggle(ui, c.setter, &p.tune_auto);
        sl!(ui, c.setter, p.tune_target, p.tune_amount, p.tune_manual);
        ui.label(RichText::new("Auto: corrects slowly (about 10 cents per second) so there is no audible pitch movement. Manual: applies the Manual Tuning offset directly.").color(TEXT_DIM).size(11.0));
    }
}

fn clarity(ui: &mut Ui, c: &Ctx) {
    let p = &c.p.clarity;
    bar(ui, "Boost", &format!("{:+.1} dB", Shared::load(&c.sh.clarity_boost)), Shared::load(&c.sh.clarity_boost), 0.0, 8.0, ACCENT2, 220.0);
    sl!(ui, c.setter, p.amount, p.freq, p.harmonics, p.warmth, p.mix);
    ui.label(RichText::new("Detail raises quiet material above Lower Edge (loud passages are untouched). Harmonics adds a little saturated air. Warmth is a +/-3 dB low shelf to balance a brighter result.").color(TEXT_DIM).size(11.0));
}

fn codec(ui: &mut Ui, c: &Ctx) {
    let p = &c.p.codec;
    combo(ui, c.setter, &p.format, "cp_fmt");
    sl!(ui, c.setter, p.kbps);
    toggle(ui, c.setter, &p.delta);
    let cut = airzone_dsp::spectral::codec_cutoff_hz(p.format.value().to_index(), p.kbps.value());
    ui.label(format!("Approximate encoder low-pass: {:.1} kHz", cut / 1000.0));
    ui.colored_label(
        ACCENT2,
        "Monitoring tool: this is a simulation of lossy-codec artefacts (band limiting, joint-stereo simplification, quantisation holes, pre-echo smearing), not a real encoder. Switch it off before you render or export your master.",
    );
    ui.label(RichText::new("Place it last in the chain. 'Listen to Difference' plays only what the simulated encode removes - if that sounds like music, the encode will be audible.").color(TEXT_DIM).size(11.0));
}

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
    ui.horizontal(|ui| {
        ui.heading(RichText::new(m.name()).color(ACCENT));
        let sw = c.p.on.by_index()[m.index()];
        toggle(ui, c.setter, sw);
        ui.label(RichText::new(description(m)).color(TEXT_DIM).size(11.0));
    });
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
    ui.columns(4, |cols| {
        let names = ["Bass", "Drums", "Vocals", "Other"];
        let gains = [&p.bass_gain, &p.drums_gain, &p.vocals_gain, &p.other_gain];
        let tilts = [&p.bass_tilt, &p.drums_tilt, &p.vocals_tilt, &p.other_tilt];
        for i in 0..4 {
            let ui = &mut cols[i];
            ui.label(RichText::new(names[i]).strong().color(ACCENT));
            let e = Shared::load(&c.sh.stem_energy[i]);
            bar(ui, "Energy", &format!("{:.0}%", e * 100.0), e, 0.0, 1.0, ACCENT, 90.0);
            slider(ui, c.setter, gains[i]);
            slider(ui, c.setter, tilts[i]);
        }
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
        let b = if cap == CAP_SOURCE { "■ Stop learning source" } else { "● Learn Source" };
        if ui.button(b).clicked() {
            c.sh.capture.store(if cap == CAP_SOURCE { CAP_OFF } else { CAP_SOURCE }, Ordering::Relaxed);
        }
        let b = if cap == CAP_REFERENCE { "■ Stop learning reference" } else { "● Learn Reference (live)" };
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
    ui.label(format!("Source: {} frames {}   |   Reference: {} frames {}   {}", src, if src > 8 { "✔" } else { "…" }, rf, if rf > 8 { "✔" } else { "…" }, c.p.match_data.lock().ref_name));
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
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width().min(760.0), 160.0), Sense::hover());
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
    let (rect, _) = ui.allocate_exact_size(vec2(w, 230.0), Sense::hover());
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
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width().min(760.0), 120.0), Sense::hover());
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
    ui.label(format!("Sub − Bass: {:+.1} dB", lv[0] - lv[1]));
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
    ui.columns(3, |cols| {
        for i in 0..3 {
            let ui = &mut cols[i];
            let b = &p.bands[i];
            ui.label(RichText::new(["Low", "Mid", "High"][i]).strong().color(ACCENT));
            toggle(ui, c.setter, &b.on);
            sl!(ui, c.setter, b.threshold, b.ratio, b.knee, b.attack, b.release, b.gain, b.exp_threshold, b.exp_ratio);
        }
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
    ui.columns(4, |cols| {
        for i in 0..4 {
            let ui = &mut cols[i];
            let b = &p.bands[i];
            ui.label(RichText::new(format!("Band {}", i + 1)).strong().color(ACCENT));
            toggle(ui, c.setter, &b.on);
            sl!(ui, c.setter, b.drive, b.amount, b.gain);
        }
    });
}

fn impact(ui: &mut Ui, c: &Ctx) {
    let p = &c.p.impact;
    sl!(ui, c.setter, p.xo1, p.xo2, p.mix);
    ui.columns(3, |cols| {
        for i in 0..3 {
            let ui = &mut cols[i];
            ui.label(RichText::new(["Low", "Mid", "High"][i]).strong().color(ACCENT));
            sl!(ui, c.setter, p.bands[i].attack, p.bands[i].sustain);
        }
    });
}

fn imager(ui: &mut Ui, c: &Ctx) {
    let p = &c.p.imager;
    let w = Shared::load(&c.sh.width);
    bar(ui, "Width", &format!("{:.0}%", w * 100.0), w, 0.0, 1.0, ACCENT, 220.0);
    let cor = Shared::load(&c.sh.correlation);
    bar(ui, "Corr.", &format!("{:+.2}", cor), (cor + 1.0) * 0.5, 0.0, 1.0, if cor < 0.0 { RED } else { GREEN }, 220.0);
    sl!(ui, c.setter, p.xo1, p.xo2, p.xo3, p.stereoize, p.balance);
    for i in 0..4 {
        ui.horizontal(|ui| {
            ui.add_sized([LABEL_W, 18.0], egui::Label::new(RichText::new(format!("Band {} width", i + 1)).color(TEXT_DIM)));
            ui.add(nih_plug_egui::widgets::ParamSlider::for_param(&p.bands[i].width, c.setter).with_width(170.0));
        });
    }
}

fn delay(ui: &mut Ui, c: &Ctx) {
    let p = &c.p.delay;
    sl!(ui, c.setter, p.time_l, p.time_r, p.feedback, p.lowcut, p.highcut, p.mix);
    toggle(ui, c.setter, &p.ping_pong);
}

fn reverb(ui: &mut Ui, c: &Ctx) {
    let p = &c.p.reverb;
    sl!(ui, c.setter, p.room, p.damping, p.width, p.predelay, p.lowcut, p.mix);
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
    ui.label(RichText::new("AZ-1 clean · AZ-2 program-dependent · AZ-3 3-band · AZ-4 4-band + saturation · AZ-5 crest-adaptive. Latency is reported to the host.").color(TEXT_DIM).size(11.0));
}

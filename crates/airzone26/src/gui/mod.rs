//! egui editor.

mod assistant;
mod modules;
mod widgets;

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use airzone_dsp::chain::{ModuleId, MODULE_COUNT};
use airzone_dsp::meter::SPEC_POINTS;
use airzone_dsp::spectral::SpectrumProfile;
use nih_plug::prelude::*;
use nih_plug_egui::egui::{self, vec2, Color32, Pos2, RichText, Sense, Stroke};
use nih_plug_egui::create_egui_editor;
use parking_lot::Mutex;

use crate::params::AirZoneParams;
use crate::presets::{self, Preset};
use crate::shared::*;
use widgets::*;

type RefJob = Arc<Mutex<Option<Result<(SpectrumProfile, String), String>>>>;

pub struct State {
    params: Arc<AirZoneParams>,
    shared: Arc<Shared>,
    pub selected: ModuleId,
    presets: Vec<Preset>,
    user_count: usize,
    current: String,
    save_name: String,
    show_save: bool,
    spec_tilt: bool,
    pub ref_path: String,
    pub status: String,
    ref_job: Option<RefJob>,
    assist: assistant::AssistUi,
    themed: bool,
}

impl State {
    fn new(params: Arc<AirZoneParams>, shared: Arc<Shared>) -> Self {
        let mut presets = presets::factory();
        let user = presets::load_user();
        let user_count = user.len();
        presets.extend(user);
        let current = params.ui_prefs.lock().preset_name.clone();
        Self {
            params,
            shared,
            selected: ModuleId::Equalizer,
            presets,
            user_count,
            current,
            save_name: String::new(),
            show_save: false,
            spec_tilt: true,
            ref_path: String::new(),
            status: String::new(),
            ref_job: None,
            assist: assistant::AssistUi::default(),
            themed: false,
        }
    }

    /// Decode and analyse the reference WAV on a background thread.
    pub fn start_reference_analysis(&mut self) {
        let path = std::path::PathBuf::from(self.ref_path.trim().trim_matches('"'));
        let job: RefJob = Arc::new(Mutex::new(None));
        self.ref_job = Some(job.clone());
        self.status = "Analysing reference…".into();
        std::thread::spawn(move || {
            *job.lock() = Some(crate::analysis::analyze_wav(&path));
        });
    }

    fn poll_jobs(&mut self) {
        let done = self.ref_job.as_ref().and_then(|j| j.lock().take());
        if let Some(res) = done {
            self.ref_job = None;
            match res {
                Ok((profile, name)) => {
                    *self.shared.pending_reference.lock() = Some((profile, name.clone()));
                    self.status = format!("Reference loaded: {name}");
                }
                Err(e) => self.status = format!("Could not load reference: {e}"),
            }
        }
    }

    fn set_current(&mut self, name: &str) {
        self.current = name.to_string();
        self.params.ui_prefs.lock().preset_name = name.to_string();
    }
}

pub fn create(params: Arc<AirZoneParams>, shared: Arc<Shared>) -> Option<Box<dyn Editor>> {
    let state = State::new(params.clone(), shared);
    create_egui_editor(params.editor_state.clone(), state, |_ctx, _st| {}, |ctx, setter, st| {
        draw(ctx, setter, st);
    })
}

fn apply_theme(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = BG1;
    v.window_fill = BG1;
    v.extreme_bg_color = BG0;
    v.widgets.inactive.bg_fill = BG2;
    v.widgets.hovered.bg_fill = Color32::from_rgb(48, 54, 64);
    v.selection.bg_fill = Color32::from_rgb(30, 110, 100);
    v.hyperlink_color = ACCENT;
    ctx.set_visuals(v);
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = vec2(8.0, 4.0);
    ctx.set_style(style);
}

fn draw(ctx: &egui::Context, setter: &ParamSetter, st: &mut State) {
    if !st.themed {
        apply_theme(ctx);
        st.themed = true;
    }
    ctx.request_repaint_after(Duration::from_millis(33));
    st.poll_jobs();
    let params = st.params.clone();
    let shared = st.shared.clone();

    top_bar(ctx, setter, st, &params);
    bottom_bar(ctx, setter, &params, &shared);
    rail(ctx, setter, st, &params, &shared);

    egui::CentralPanel::default().show(ctx, |ui| {
        spectrum(ui, &shared, st.spec_tilt);
        ui.checkbox(&mut st.spec_tilt, "Spectrum slope compensation (+4.5 dB/oct)");
        ui.separator();
        let c = modules::Ctx { p: &params, setter, sh: &shared };
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let sel = st.selected;
            modules::show(ui, sel, &c, st);
        });
    });

    assistant::show(ctx, &mut st.assist, &params, &shared, setter);
}

fn top_bar(ctx: &egui::Context, setter: &ParamSetter, st: &mut State, p: &AirZoneParams) {
    egui::TopBottomPanel::top("top").exact_height(46.0).show(ctx, |ui| {
        ui.horizontal_centered(|ui| {
            ui.label(RichText::new("AIRZONE 26").size(20.0).strong().color(ACCENT));
            ui.separator();
            // preset navigation
            let n = st.presets.len();
            let idx = st.presets.iter().position(|x| x.name == st.current);
            if ui.button("◀").clicked() && n > 0 {
                let i = idx.map(|i| (i + n - 1) % n).unwrap_or(0);
                let pr = st.presets[i].clone();
                presets::apply_gui(&pr, p, setter);
                st.set_current(&pr.name);
            }
            let label = if st.current.is_empty() { "— Presets —".to_string() } else { st.current.clone() };
            egui::ComboBox::from_id_salt("presets").selected_text(label).width(260.0).height(520.0).show_ui(ui, |ui| {
                let mut last_cat = String::new();
                let mut chosen: Option<Preset> = None;
                for pr in &st.presets {
                    if pr.category != last_cat {
                        ui.label(RichText::new(pr.category.to_uppercase()).size(10.0).color(ACCENT));
                        last_cat = pr.category.clone();
                    }
                    if ui.selectable_label(pr.name == st.current, &pr.name).on_hover_text(&pr.description).clicked() {
                        chosen = Some(pr.clone());
                    }
                }
                if let Some(pr) = chosen {
                    presets::apply_gui(&pr, p, setter);
                    st.set_current(&pr.name);
                }
            });
            if ui.button("▶").clicked() && n > 0 {
                let i = idx.map(|i| (i + 1) % n).unwrap_or(0);
                let pr = st.presets[i].clone();
                presets::apply_gui(&pr, p, setter);
                st.set_current(&pr.name);
            }
            if ui.button("💾 Save").clicked() {
                st.show_save = !st.show_save;
            }
            ui.separator();
            if ui.button("🎛 Master Assistant").clicked() {
                st.assist.open = true;
            }
            ui.separator();
            toggle(ui, setter, &p.bypass);
            ui.add(nih_plug_egui::widgets::ParamSlider::for_param(&p.input_gain, setter).with_width(110.0));
            ui.label("→");
            ui.add(nih_plug_egui::widgets::ParamSlider::for_param(&p.output_gain, setter).with_width(110.0));
            combo_small(ui, setter, &p.dither, "dither".into(), 70.0);
        });
    });
    if st.show_save {
        egui::TopBottomPanel::top("save").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Preset name:");
                ui.text_edit_singleline(&mut st.save_name);
                if ui.button("Save to user presets").clicked() && !st.save_name.trim().is_empty() {
                    let pr = presets::capture(st.save_name.trim(), "User", p);
                    match presets::save_user(&pr) {
                        Ok(path) => {
                            st.status = format!("Saved {}", path.display());
                            st.presets.retain(|x| !(x.category == "User" && x.name == pr.name));
                            st.presets.push(Preset { category: "User".into(), ..pr.clone() });
                            st.user_count += 1;
                            st.set_current(&pr.name);
                            st.show_save = false;
                        }
                        Err(e) => st.status = format!("Save failed: {e}"),
                    }
                }
                if let Some(d) = presets::user_dir() {
                    ui.label(RichText::new(d.display().to_string()).size(10.0).color(TEXT_DIM));
                }
            });
        });
    }
}

fn rail(ctx: &egui::Context, setter: &ParamSetter, st: &mut State, p: &AirZoneParams, sh: &Shared) {
    egui::SidePanel::left("rail").exact_width(210.0).show(ctx, |ui| {
        ui.label(RichText::new("SIGNAL CHAIN").size(10.0).color(TEXT_DIM));
        let mut order: Vec<u8> = p.order.read().clone();
        let mut swap: Option<(usize, usize)> = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            for pos in 0..MODULE_COUNT.min(order.len()) {
                let Some(m) = ModuleId::from_index(order[pos] as usize) else { continue };
                ui.horizontal(|ui| {
                    let sw = p.on.by_index()[m.index()];
                    let mut on = sw.value();
                    if ui.checkbox(&mut on, "").changed() {
                        setter.begin_set_parameter(sw);
                        setter.set_parameter(sw, on);
                        setter.end_set_parameter(sw);
                    }
                    let text = if m.is_spectral() { format!("{} ·fft", m.name()) } else { m.name().to_string() };
                    let rt = RichText::new(text).color(if on { Color32::WHITE } else { TEXT_DIM });
                    if ui.selectable_label(st.selected == m, rt).clicked() {
                        st.selected = m;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("▼").clicked() && pos + 1 < order.len() {
                            swap = Some((pos, pos + 1));
                        }
                        if ui.small_button("▲").clicked() && pos > 0 {
                            swap = Some((pos, pos - 1));
                        }
                    });
                });
            }
        });
        if let Some((a, b)) = swap {
            order.swap(a, b);
            *p.order.write() = order;
        }
        ui.separator();
        let lat = sh.latency.load(Ordering::Relaxed);
        let sr = Shared::load(&sh.sample_rate).max(1.0);
        ui.label(RichText::new(format!("Latency: {} smp ({:.1} ms)", lat, lat as f32 / sr * 1000.0)).size(11.0).color(TEXT_DIM));
        if ui.small_button("Reset chain order").clicked() {
            *p.order.write() = airzone_dsp::chain::default_order().iter().map(|m| m.index() as u8).collect();
        }
    });
}

fn lufs_text(v: f32) -> String {
    if v < -69.0 { "−∞".into() } else { format!("{:.1}", v) }
}

fn bottom_bar(ctx: &egui::Context, _setter: &ParamSetter, p: &AirZoneParams, sh: &Shared) {
    egui::TopBottomPanel::bottom("meters").exact_height(96.0).show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                let m = Shared::load(&sh.out_momentary);
                let s = Shared::load(&sh.out_short);
                let i = Shared::load(&sh.out_integrated);
                bar(ui, "Moment.", &lufs_text(m), m, -40.0, 0.0, ACCENT, 200.0);
                bar(ui, "Short", &lufs_text(s), s, -40.0, 0.0, ACCENT, 200.0);
                bar(ui, "Integr.", &lufs_text(i), i, -40.0, 0.0, ACCENT, 200.0);
            });
            ui.separator();
            ui.vertical(|ui| {
                let tp = Shared::load(&sh.true_peak);
                let ceil = p.max.ceiling.value();
                let col = if tp > ceil + 0.1 && p.on.maximizer.value() { RED } else { GREEN };
                bar(ui, "True pk", &format!("{:.1} dBTP", tp.max(-99.0)), tp, -40.0, 3.0, col, 200.0);
                let cor = Shared::load(&sh.correlation);
                bar(ui, "Corr.", &format!("{:+.2}", cor), (cor + 1.0) * 0.5, 0.0, 1.0, if cor < 0.0 { RED } else { GREEN }, 200.0);
                ui.label(RichText::new(format!("LRA {:.1} LU   In: {} LUFS", Shared::load(&sh.out_lra), lufs_text(Shared::load(&sh.in_integrated)))).size(11.0).color(TEXT_DIM));
            });
            ui.separator();
            gr_meter(ui, "MAX", Shared::load(&sh.gr[GR_MAXIMIZER]), 12.0);
            for b in 0..3 {
                gr_meter(ui, &format!("D{}", b + 1), Shared::load(&sh.gr[GR_DYN_BASE + b]), 12.0);
            }
            gr_meter(ui, "COMP", Shared::load(&sh.gr[GR_VCOMP]), 12.0);
            ui.separator();
            if ui.button("Reset meters").clicked() {
                sh.reset_meters.store(true, Ordering::Relaxed);
            }
        });
    });
}

fn spectrum(ui: &mut egui::Ui, sh: &Shared, tilt: bool) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 190.0), Sense::hover());
    draw_freq_grid(ui, rect);
    let painter = ui.painter_at(rect);
    for db in [-20.0, -40.0, -60.0, -80.0] {
        let y = rect.top() + rect.height() * (0.0 - db) / 100.0;
        painter.line_segment([Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)], Stroke::new(1.0_f32, Color32::from_rgb(26, 29, 35)));
    }


    for (arr, color, w) in [(&sh.spectrum_in, Color32::from_rgb(110, 120, 135), 1.2), (&sh.spectrum_out, ACCENT, 2.0)] {
        let pts: Vec<Pos2> = (0..SPEC_POINTS)
            .map(|i| {
                let f = airzone_dsp::util::log_freq(i, SPEC_POINTS, 20.0, 20000.0);
                let mut db = Shared::load(&arr[i]);
                if tilt {
                    db += 4.5 * (f / 1000.0).log2();
                }
                let x = freq_to_x(f, rect);
                let y = rect.top() + rect.height() * ((0.0 - db) / 100.0).clamp(0.0, 1.0);
                Pos2::new(x, y)
            })
            .collect();
        polyline(ui, rect, pts, color, w);
    }
}

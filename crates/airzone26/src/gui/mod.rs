//! egui editor.

mod assistant;
mod modules;
mod widgets;

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use airzone_dsp::assistant::Genre;
use airzone_dsp::chain::{sanitize_order, ModuleId, MODULE_COUNT};
use airzone_dsp::meter::SPEC_POINTS;
use airzone_dsp::spectral::SpectrumProfile;
use nih_plug::prelude::*;
use nih_plug_egui::create_egui_editor;
use nih_plug_egui::egui::{self, vec2, Color32, Pos2, RichText, Sense, Stroke};
use nih_plug::params::persist::PersistentField;
use nih_plug_egui::EguiState;
use parking_lot::Mutex;

use crate::module_presets::{self, ModulePreset};
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
    show_target: bool,
    target_genre: usize,
    pub ref_path: String,
    pub status: String,
    ref_job: Option<RefJob>,
    assist: assistant::AssistUi,
    themed: bool,
    /// A/B compare slots and which one is live.
    ab: [Option<Preset>; 2],
    ab_active: usize,
    /// Factory module presets (built once) and cached user module presets.
    mod_factory: Vec<ModulePreset>,
    mod_user_cache: HashMap<usize, Vec<ModulePreset>>,
    /// Name of the last applied preset per module.
    mod_current: HashMap<usize, String>,
    pub mod_save_name: String,
    /// Reference A/B player.
    refp_path: String,
    refp_job: Option<RefTrackJob>,
    show_about: bool,
    /// Loaded reference tracks are kept alive here so the audio thread never frees the audio.
    ref_tracks: Vec<Arc<airzone_dsp::reference::ReferenceTrack>>,
}

type RefTrackJob = Arc<Mutex<Option<Result<airzone_dsp::reference::ReferenceTrack, String>>>>;

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
            show_target: false,
            target_genre: 0,
            ref_path: String::new(),
            status: String::new(),
            ref_job: None,
            assist: assistant::AssistUi::default(),
            themed: false,
            ab: [None, None],
            ab_active: 0,
            mod_factory: module_presets::factory(),
            mod_user_cache: HashMap::new(),
            mod_current: HashMap::new(),
            mod_save_name: String::new(),
            refp_path: String::new(),
            refp_job: None,
            show_about: false,
            ref_tracks: Vec::new(),
        }
    }

    fn start_reference_load(&mut self) {
        let path = std::path::PathBuf::from(self.refp_path.trim().trim_matches('"'));
        let sr = Shared::load(&self.shared.sample_rate).max(8000.0);
        let job: RefTrackJob = Arc::new(Mutex::new(None));
        self.refp_job = Some(job.clone());
        self.status = "Loading reference track...".into();
        std::thread::spawn(move || {
            *job.lock() = Some(crate::analysis::load_reference(&path, sr));
        });
    }

    /// Decode and analyse the reference WAV on a background thread.
    pub fn start_reference_analysis(&mut self) {
        let path = std::path::PathBuf::from(self.ref_path.trim().trim_matches('"'));
        let job: RefJob = Arc::new(Mutex::new(None));
        self.ref_job = Some(job.clone());
        self.status = "Analysing reference...".into();
        std::thread::spawn(move || {
            *job.lock() = Some(crate::analysis::analyze_wav(&path));
        });
    }

    fn poll_jobs(&mut self) {
        let loaded = self.refp_job.as_ref().and_then(|j| j.lock().take());
        if let Some(res) = loaded {
            self.refp_job = None;
            match res {
                Ok(track) => {
                    let arc = Arc::new(track);
                    self.status = format!("Reference loaded: {} ({:.1} LUFS)", arc.name, arc.lufs);
                    self.ref_tracks.push(arc.clone());
                    if self.ref_tracks.len() > 2 {
                        self.ref_tracks.remove(0);
                    }
                    *self.shared.pending_ref_track.lock() = Some(arc);
                }
                Err(e) => self.status = format!("Could not load reference: {e}"),
            }
        }
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

    /// Factory + user presets for one module.
    pub fn module_preset_list(&mut self, m: ModuleId) -> Vec<ModulePreset> {
        let user = self.mod_user_cache.entry(m.index()).or_insert_with(|| module_presets::load_user_modules(m)).clone();
        let mut v: Vec<ModulePreset> = self.mod_factory.iter().filter(|p| p.module as usize == m.index()).cloned().collect();
        v.extend(user);
        v
    }

    pub fn module_current(&self, m: ModuleId) -> String {
        self.mod_current.get(&m.index()).cloned().unwrap_or_default()
    }

    pub fn set_module_current(&mut self, m: ModuleId, name: &str) {
        self.mod_current.insert(m.index(), name.to_string());
    }

    pub fn forget_user_modules(&mut self, m: ModuleId) {
        self.mod_user_cache.remove(&m.index());
    }
}

/// Selectable window sizes (applied when the plug-in window is next opened).
pub const WINDOW_SIZES: [(&str, u32, u32); 4] = [
    ("Compact 900 x 600", 900, 600),
    ("Standard 1040 x 680", 1040, 680),
    ("Large 1280 x 820", 1280, 820),
    ("Extra large 1600 x 1000", 1600, 1000),
];

/// Store a new window size; the host picks it up the next time the editor window is opened.
pub fn set_window_size(p: &AirZoneParams, w: u32, h: u32) {
    if let Ok(state) = Arc::try_unwrap(EguiState::from_size(w, h)) {
        p.editor_state.set(state);
    }
}

/// Saved sizes from older versions (or from a resize that went wrong) can be absurd, for example
/// very wide and very low. Anything implausible is reset to the standard size.
fn sanitize_window_size(p: &AirZoneParams) {
    let (w, h) = p.editor_state.size();
    let ratio = w as f32 / h.max(1) as f32;
    if w < 640 || h < 420 || w > 3200 || h > 2200 || !(1.2..=2.0).contains(&ratio) {
        set_window_size(p, 1040, 680);
    }
}

pub fn create(params: Arc<AirZoneParams>, shared: Arc<Shared>) -> Option<Box<dyn Editor>> {
    sanitize_window_size(&params);
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

    // NOTE: the window is deliberately NOT resized from inside the plug-in. Host resizing through
    // the plug-in API is unreliable (REAPER flickered and resized itself). Instead the window size
    // is chosen from the "Window" menu and applied the next time the plug-in window is opened;
    // the layout adapts to whatever size the host gives us.
    egui::CentralPanel::default().frame(egui::Frame::default()).show(ctx, |ui| {
        let narrow = ui.available_width() < 820.0;
        top_bar(ui, setter, st, &params, narrow);
        bottom_bar(ui, &params, &shared);
        if !narrow {
            rail(ui, setter, st, &params, &shared);
        }
        egui::CentralPanel::default().frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(6, 4))).show_inside(ui, |ui| {
            if narrow {
                compact_selector(ui, setter, st, &params);
            }
            let h = ui.available_height();
            let target = if st.show_target { Some(Genre::ALL[st.target_genre.min(Genre::ALL.len() - 1)].target_shape()) } else { None };
            spectrum(ui, &shared, st.spec_tilt, (h * 0.26).clamp(70.0, 220.0), target);
            ui.horizontal_wrapped(|ui| {
                ui.checkbox(&mut st.spec_tilt, "Slope compensation (+4.5 dB/oct)");
                ui.checkbox(&mut st.show_target, "Show tonal target");
                if st.show_target {
                    egui::ComboBox::from_id_salt("target_genre").selected_text(Genre::ALL[st.target_genre.min(Genre::ALL.len() - 1)].name()).show_ui(ui, |ui| {
                        for (i, g) in Genre::ALL.iter().enumerate() {
                            ui.selectable_value(&mut st.target_genre, i, g.name());
                        }
                    });
                }
                if !st.status.is_empty() {
                    ui.label(RichText::new(st.status.clone()).size(11.0).color(ACCENT2));
                }
            });
            reference_strip(ui, st);
            ui.separator();
            let c = modules::Ctx { p: &params, setter, sh: &shared };
            egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
                let sel = st.selected;
                modules::show(ui, sel, &c, st);
            });
        });
    });

    assistant::show(ctx, &mut st.assist, &params, &shared, setter);
    about_window(ctx, st);
}

/// Plug-in version (from Cargo.toml).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const AUTHOR: &str = "crowelian";
pub const YEAR: &str = "2026";

fn about_window(ctx: &egui::Context, st: &mut State) {
    if !st.show_about {
        return;
    }
    let mut open = st.show_about;
    let sh = st.shared.clone();
    egui::Window::new("About AirZone 26").open(&mut open).collapsible(false).resizable(false).default_width(380.0).show(ctx, |ui| {
        ui.vertical_centered(|ui| {
            ui.label(RichText::new("AIRZONE 26").size(30.0).strong().color(ACCENT));
            ui.label(RichText::new(format!("Version {}", VERSION)).size(14.0));
            ui.label(RichText::new(format!("by {} {}", AUTHOR, YEAR)).size(14.0).color(ACCENT2));
        });
        ui.separator();
        ui.label("Full mastering suite for REAPER and other VST3 / CLAP hosts: 21 modules, Master Assistant, reference A/B player, pitch correction and 200+ presets.");
        ui.add_space(4.0);
        let sr = Shared::load(&sh.sample_rate);
        let lat = sh.latency.load(Ordering::Relaxed);
        ui.label(RichText::new(format!("Sample rate {:.0} Hz | total latency {} samples ({:.1} ms)", sr, lat, lat as f32 / sr.max(1.0) * 1000.0)).size(11.0).color(TEXT_DIM));
        ui.label(RichText::new(format!("Build: {} {}", std::env::consts::OS, std::env::consts::ARCH)).size(11.0).color(TEXT_DIM));
        ui.add_space(4.0);
        ui.label(RichText::new("Licence: GPL-3.0-or-later. Built with Rust, NIH-plug and egui.").size(11.0).color(TEXT_DIM));
        ui.label(
            RichText::new("All algorithms and presets are original. Other products mentioned in the manual are trademarks of their owners; AirZone 26 is not affiliated with them.")
                .size(10.0)
                .color(TEXT_DIM),
        );
        ui.add_space(6.0);
        ui.vertical_centered(|ui| {
            ui.label(RichText::new(format!("(c) {} {}", YEAR, AUTHOR)).size(11.0));
        });
    });
    st.show_about = open;
}

/// Reference A/B player: load a WAV, switch between the master and the reference (level matched),
/// transport, loop and trim.
fn reference_strip(ui: &mut egui::Ui, st: &mut State) {
    let sh = st.shared.clone();
    egui::CollapsingHeader::new("Reference A/B player").default_open(false).show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.add(egui::TextEdit::singleline(&mut st.refp_path).desired_width(300.0).hint_text("path to a WAV reference track"));
            if ui.button("Load").clicked() {
                st.start_reference_load();
            }
        });
        let len = Shared::load(&sh.ref_len_s);
        if len <= 0.0 {
            ui.label(RichText::new("Load an uncompressed WAV (up to 6 minutes). Then switch between your master and the reference at matched loudness.").size(11.0).color(TEXT_DIM));
            return;
        }
        ui.horizontal_wrapped(|ui| {
            let mon = sh.ref_monitor.load(Ordering::Relaxed);
            if ui.selectable_label(!mon, "  Master  ").clicked() {
                sh.ref_monitor.store(false, Ordering::Relaxed);
            }
            if ui.selectable_label(mon, "  Reference  ").clicked() {
                sh.ref_monitor.store(true, Ordering::Relaxed);
            }
            let playing = sh.ref_playing.load(Ordering::Relaxed);
            if ui.button(if playing { "Pause" } else { "Play" }).clicked() {
                sh.ref_playing.store(!playing, Ordering::Relaxed);
            }
            if ui.button("Restart").clicked() {
                sh.ref_seek.store(0.0, Ordering::Relaxed);
            }
            let mut pos = Shared::load(&sh.ref_pos);
            if ui.add(egui::Slider::new(&mut pos, 0.0..=1.0).show_value(false)).changed() {
                sh.ref_seek.store(pos, Ordering::Relaxed);
            }
            ui.label(RichText::new(format!("{:.0}s / {:.0}s", pos * len, len)).size(11.0).color(TEXT_DIM));
        });
        ui.horizontal_wrapped(|ui| {
            let mut lp = sh.ref_loop.load(Ordering::Relaxed);
            if ui.checkbox(&mut lp, "Loop").changed() {
                sh.ref_loop.store(lp, Ordering::Relaxed);
            }
            let mut lm = sh.ref_level_match.load(Ordering::Relaxed);
            if ui.checkbox(&mut lm, "Match loudness to master").changed() {
                sh.ref_level_match.store(lm, Ordering::Relaxed);
            }
            let mut trim = Shared::load(&sh.ref_trim_db);
            if ui.add(egui::Slider::new(&mut trim, -12.0..=12.0).suffix(" dB").text("Trim")).changed() {
                sh.ref_trim_db.store(trim, Ordering::Relaxed);
            }
            ui.label(
                RichText::new(format!("reference {:.1} LUFS, playing at {:+.1} dB", Shared::load(&sh.ref_lufs), Shared::load(&sh.ref_gain_db)))
                    .size(11.0)
                    .color(TEXT_DIM),
            );
        });
    });
}

/// The chain order with every module present exactly once (old saved states may lack newer modules).
fn full_order(p: &AirZoneParams) -> Vec<u8> {
    let saved: Vec<u8> = p.order.read().clone();
    sanitize_order(&saved).iter().map(|m| m.index() as u8).collect()
}

/// Module picker used instead of the side rail when the window is narrow.
fn compact_selector(ui: &mut egui::Ui, setter: &ParamSetter, st: &mut State, p: &AirZoneParams) {
    let order = full_order(p);
    ui.horizontal_wrapped(|ui| {
        egui::ComboBox::from_id_salt("compact_module").selected_text(st.selected.name()).height(420.0).show_ui(ui, |ui| {
            for o in &order {
                if let Some(m) = ModuleId::from_index(*o as usize) {
                    let on = p.on.by_index()[m.index()].value();
                    let label = if on { format!("[on]  {}", m.name()) } else { format!("[off] {}", m.name()) };
                    ui.selectable_value(&mut st.selected, m, label);
                }
            }
        });
        let sw = p.on.by_index()[st.selected.index()];
        let mut on = sw.value();
        if ui.checkbox(&mut on, "On").changed() {
            setter.begin_set_parameter(sw);
            setter.set_parameter(sw, on);
            setter.end_set_parameter(sw);
        }
        let lat = st.shared.latency.load(Ordering::Relaxed);
        ui.label(RichText::new(format!("latency {} smp", lat)).size(10.0).color(TEXT_DIM));
    });
}

fn top_bar(ui: &mut egui::Ui, setter: &ParamSetter, st: &mut State, p: &AirZoneParams, narrow: bool) {
    egui::TopBottomPanel::top("top").show_inside(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("AIRZONE 26").size(20.0).strong().color(ACCENT));
            if ui.add(egui::Label::new(RichText::new(format!("v{}", VERSION)).size(11.0).color(TEXT_DIM)).sense(Sense::click())).on_hover_text("About AirZone 26").clicked() {
                st.show_about = true;
            }
            ui.separator();
            // preset navigation
            let n = st.presets.len();
            let idx = st.presets.iter().position(|x| x.name == st.current);
            if arrow_button(ui, Dir::Left).on_hover_text("Previous preset").clicked() && n > 0 {
                let i = idx.map(|i| (i + n - 1) % n).unwrap_or(0);
                let pr = st.presets[i].clone();
                presets::apply_gui(&pr, p, setter);
                st.set_current(&pr.name);
            }
            let label = if st.current.is_empty() { "Presets".to_string() } else { st.current.clone() };
            let pw = if narrow { 170.0 } else { 260.0 };
            egui::ComboBox::from_id_salt("presets").selected_text(label).width(pw).height(520.0).show_ui(ui, |ui| {
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
            if arrow_button(ui, Dir::Right).on_hover_text("Next preset").clicked() && n > 0 {
                let i = idx.map(|i| (i + 1) % n).unwrap_or(0);
                let pr = st.presets[i].clone();
                presets::apply_gui(&pr, p, setter);
                st.set_current(&pr.name);
            }
            if ui.button("Save").on_hover_text("Save the whole chain as a user preset").clicked() {
                st.show_save = !st.show_save;
            }
            ui.separator();
            if ui.button("Master Assistant").clicked() {
                st.assist.open = true;
            }
            if ui.button("About").clicked() {
                st.show_about = true;
            }
            ui.separator();
            // A/B compare
            ui.label(RichText::new("A/B").size(11.0).color(TEXT_DIM));
            for slot in 0..2usize {
                let name = if slot == 0 { "A" } else { "B" };
                if ui.selectable_label(st.ab_active == slot, name).on_hover_text("Switch between two complete settings").clicked() && st.ab_active != slot {
                    let cur = presets::capture("ab", "User", p);
                    let active = st.ab_active;
                    st.ab[active] = Some(cur);
                    if let Some(other) = st.ab[slot].clone() {
                        presets::apply_gui(&other, p, setter);
                    }
                    st.ab_active = slot;
                }
            }
            if ui.small_button("Copy").on_hover_text("Copy the current settings into the other A/B slot").clicked() {
                let cur = presets::capture("ab", "User", p);
                let other = 1 - st.ab_active;
                st.ab[other] = Some(cur);
            }
            ui.separator();
            toggle(ui, setter, &p.bypass);
            toggle(ui, setter, &p.gain_match);
            let gw = if narrow { 84.0 } else { 110.0 };
            ui.add(nih_plug_egui::widgets::ParamSlider::for_param(&p.input_gain, setter).with_width(gw));
            ui.label("to");
            ui.add(nih_plug_egui::widgets::ParamSlider::for_param(&p.output_gain, setter).with_width(gw));
            combo_small(ui, setter, &p.dither, "dither".into(), 70.0);
            let cur = p.editor_state.size();
            let cur_label = WINDOW_SIZES.iter().find(|(_, w, h)| (*w, *h) == cur).map(|(n, _, _)| *n).unwrap_or("Custom size");
            egui::ComboBox::from_id_salt("window_size").selected_text("Window").width(64.0).show_ui(ui, |ui| {
                ui.label(RichText::new(format!("Now: {cur_label}")).size(11.0).color(TEXT_DIM));
                for (n, w, h) in WINDOW_SIZES {
                    if ui.selectable_label(cur == (w, h), n).clicked() {
                        set_window_size(p, w, h);
                        st.status = "Window size saved. Close and reopen the plug-in window to apply it.".into();
                    }
                }
            });
        });
    });
    if st.show_save {
        egui::TopBottomPanel::top("save").show_inside(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
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

fn rail(ui: &mut egui::Ui, setter: &ParamSetter, st: &mut State, p: &AirZoneParams, sh: &Shared) {
    egui::SidePanel::left("rail").resizable(false).exact_width(196.0).show_inside(ui, |ui| {
        ui.label(RichText::new("SIGNAL CHAIN").size(10.0).color(TEXT_DIM));
        let mut order: Vec<u8> = full_order(p);
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
                    let text = if m.is_spectral() { format!("{} (fft)", m.name()) } else { m.name().to_string() };
                    let rt = RichText::new(text).size(12.5).color(if on { Color32::WHITE } else { TEXT_DIM });
                    if ui.selectable_label(st.selected == m, rt).clicked() {
                        st.selected = m;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if arrow_button(ui, Dir::Down).on_hover_text("Move down in the chain").clicked() && pos + 1 < order.len() {
                            swap = Some((pos, pos + 1));
                        }
                        if arrow_button(ui, Dir::Up).on_hover_text("Move up in the chain").clicked() && pos > 0 {
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
    if v < -69.0 {
        "-inf".into()
    } else {
        format!("{:.1}", v)
    }
}

fn bottom_bar(ui: &mut egui::Ui, p: &AirZoneParams, sh: &Shared) {
    egui::TopBottomPanel::bottom("meters").show_inside(ui, |ui| {
        egui::ScrollArea::horizontal().id_salt("meters_scroll").show(ui, |ui| {
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
    });
}

fn spectrum(ui: &mut egui::Ui, sh: &Shared, tilt: bool, height: f32, target: Option<[f32; airzone_dsp::spectral::MATCH_BANDS]>) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    draw_freq_grid(ui, rect);
    let painter = ui.painter_at(rect);
    for db in [-20.0, -40.0, -60.0, -80.0] {
        let y = rect.top() + rect.height() * (0.0 - db) / 100.0;
        painter.line_segment([Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)], Stroke::new(1.0_f32, Color32::from_rgb(26, 29, 35)));
    }
    let tilt_db = |f: f32| if tilt { 4.5 * (f / 1000.0).log2() } else { 0.0 };
    // measured curves (db value after optional slope compensation)
    let mut out_curve: Vec<(f32, f32)> = Vec::with_capacity(SPEC_POINTS);
    for (arr, color, w) in [(&sh.spectrum_in, Color32::from_rgb(110, 120, 135), 1.2_f32), (&sh.spectrum_out, ACCENT, 2.0_f32)] {
        let mut pts: Vec<Pos2> = Vec::with_capacity(SPEC_POINTS);
        for i in 0..SPEC_POINTS {
            let f = airzone_dsp::util::log_freq(i, SPEC_POINTS, 20.0, 20000.0);
            let db = Shared::load(&arr[i]) + tilt_db(f);
            if color == ACCENT {
                out_curve.push((f, db));
            }
            let x = freq_to_x(f, rect);
            let y = rect.top() + rect.height() * ((0.0 - db) / 100.0).clamp(0.0, 1.0);
            pts.push(Pos2::new(x, y));
        }
        polyline(ui, rect, pts, color, w);
    }
    // genre target (aligned to the measured curve between 100 Hz and 8 kHz)
    if let Some(shape) = target {
        let mb = airzone_dsp::spectral::MATCH_BANDS;
        let sample = |f: f32| -> f32 {
            let pos = ((f / 20.0).ln() / 1000f32.ln() * (mb - 1) as f32).clamp(0.0, (mb - 1) as f32);
            let i = pos.floor() as usize;
            let fr = pos - i as f32;
            let a = shape[i];
            let b = shape[(i + 1).min(mb - 1)];
            a + (b - a) * fr
        };
        let (mut acc, mut n) = (0.0f32, 0.0f32);
        for (f, db) in &out_curve {
            if *f >= 100.0 && *f <= 8000.0 && *db > -90.0 {
                acc += db - (sample(*f) + tilt_db(*f));
                n += 1.0;
            }
        }
        let offset = if n > 0.0 { acc / n } else { -50.0 };
        let pts: Vec<Pos2> = (0..SPEC_POINTS)
            .map(|i| {
                let f = airzone_dsp::util::log_freq(i, SPEC_POINTS, 20.0, 20000.0);
                let db = sample(f) + tilt_db(f) + offset;
                Pos2::new(freq_to_x(f, rect), rect.top() + rect.height() * ((0.0 - db) / 100.0).clamp(0.0, 1.0))
            })
            .collect();
        polyline(ui, rect, pts, ACCENT2, 1.5);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nih_plug::context::gui::GuiContext;
    use nih_plug::wrapper::state::PluginState;

    struct Mock;
    impl GuiContext for Mock {
        fn plugin_api(&self) -> PluginApi {
            PluginApi::Clap
        }
        fn request_resize(&self) -> bool {
            true
        }
        unsafe fn raw_begin_set_parameter(&self, _p: ParamPtr) {}
        unsafe fn raw_set_parameter_normalized(&self, _p: ParamPtr, _n: f32) {}
        unsafe fn raw_end_set_parameter(&self, _p: ParamPtr) {}
        fn get_state(&self) -> PluginState {
            PluginState { version: String::new(), params: Default::default(), fields: Default::default() }
        }
        fn set_state(&self, _s: PluginState) {}
    }

    /// Lay the whole editor out at many window sizes with every module selected: nothing may panic.
    #[test]
    fn editor_lays_out_at_all_sizes_for_all_modules() {
        let params = Arc::new(AirZoneParams::default());
        let shared = Arc::new(Shared::new());
        let mock = Mock;
        let setter = ParamSetter::new(&mock);
        let ctx = egui::Context::default();
        let mut st = State::new(params, shared);
        st.show_target = true;
        for (w, h) in [(480.0, 340.0), (640.0, 420.0), (800.0, 560.0), (1040.0, 680.0), (1600.0, 900.0), (2400.0, 1400.0)] {
            for m in ModuleId::ALL {
                st.selected = m;
                for _ in 0..2 {
                    let input = egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, vec2(w, h))),
                        ..Default::default()
                    };
                    let _ = ctx.run(input, |ctx| draw(ctx, &setter, &mut st));
                }
            }
        }
        st.assist.open = true;
        let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, vec2(640.0, 420.0))), ..Default::default() };
        let _ = ctx.run(input, |ctx| draw(ctx, &setter, &mut st));
    }
}

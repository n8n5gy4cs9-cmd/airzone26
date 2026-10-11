//! AirZone 26 Tonal Balance: a pass-through meter that compares the tonal balance of what you
//! play with a genre target or a reference of your own (captured live or loaded from a WAV file).
//! The audio is never changed, so it can stay on the master bus (put it last, after the limiter).

use std::num::NonZeroU32;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use airzone_dsp::assistant::{Analyzer, Genre};
use airzone_dsp::spectral::MATCH_BANDS;
use airzone_dsp::tonal::{region_levels, TonalMeter, REGIONS};
use nih_plug::prelude::*;
use nih_plug_egui::egui::{self, vec2, Color32, Pos2, RichText, Sense, Stroke};
use nih_plug_egui::{create_egui_editor, EguiState};
use parking_lot::Mutex;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

// ---------------------------------------------------------------------------------- parameters

#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetModeP {
    #[name = "Genre target"]
    Genre,
    #[name = "Custom target (captured / reference)"]
    Custom,
}

#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenreP {
    #[name = "Universal"]
    Universal,
    #[name = "Pop"]
    Pop,
    #[name = "Rock"]
    Rock,
    #[name = "Hip-Hop / R&B"]
    HipHop,
    #[name = "Electronic / EDM"]
    Electronic,
    #[name = "Acoustic / Folk / Jazz"]
    Acoustic,
    #[name = "Metal / Hard Rock"]
    Metal,
    #[name = "Classical / Cinematic"]
    Classical,
}

impl GenreP {
    pub fn genre(self) -> Genre {
        Genre::ALL[self as usize]
    }
}

#[derive(Params)]
pub struct TonalParams {
    #[persist = "editor-size-v2"]
    pub editor_state: Arc<EguiState>,
    /// 64-band zero-mean shape of the custom target (empty = none).
    #[persist = "custom-target"]
    pub custom: Arc<Mutex<Vec<f32>>>,
    #[id = "mode"]
    pub mode: EnumParam<TargetModeP>,
    #[id = "genre"]
    pub genre: EnumParam<GenreP>,
    /// +- tolerance of the target range in dB.
    #[id = "tol"]
    pub tolerance: FloatParam,
    /// Analysis memory in seconds.
    #[id = "memory"]
    pub memory: FloatParam,
}

impl Default for TonalParams {
    fn default() -> Self {
        Self {
            editor_state: EguiState::from_size(760, 640),
            custom: Arc::new(Mutex::new(Vec::new())),
            mode: EnumParam::new("Target", TargetModeP::Genre),
            genre: EnumParam::new("Genre", GenreP::Universal),
            tolerance: FloatParam::new("Tolerance", 3.0, FloatRange::Linear { min: 1.0, max: 8.0 }).with_unit(" dB").with_step_size(0.1),
            memory: FloatParam::new("Analysis Memory", 6.0, FloatRange::Linear { min: 1.0, max: 30.0 }).with_unit(" s").with_step_size(0.5),
        }
    }
}

// ---------------------------------------------------------------------------------- shared state

pub struct Shared {
    /// smoothed zero-mean shape published by the audio thread
    pub shape: Vec<nih_plug::prelude::AtomicF32>,
    pub ready: AtomicBool,
    pub reset: AtomicBool,
}

impl Shared {
    fn new() -> Self {
        Self {
            shape: (0..MATCH_BANDS).map(|_| nih_plug::prelude::AtomicF32::new(0.0)).collect(),
            ready: AtomicBool::new(false),
            reset: AtomicBool::new(false),
        }
    }
}

// ---------------------------------------------------------------------------------- plug-in

pub struct TonalBalance {
    params: Arc<TonalParams>,
    shared: Arc<Shared>,
    meter: Option<TonalMeter>,
    sr: f32,
    since_publish: usize,
}

impl Default for TonalBalance {
    fn default() -> Self {
        Self { params: Arc::new(TonalParams::default()), shared: Arc::new(Shared::new()), meter: None, sr: 44100.0, since_publish: 0 }
    }
}

impl Plugin for TonalBalance {
    const NAME: &'static str = "AirZone 26 Tonal Balance";
    const VENDOR: &'static str = "crowelian";
    const URL: &'static str = "https://github.com/n8n5gy4cs9-cmd/airzone26";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: NonZeroU32::new(2),
        main_output_channels: NonZeroU32::new(2),
        ..AudioIOLayout::const_default()
    }];

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        let state = GuiState { params: self.params.clone(), shared: self.shared.clone(), status: String::new(), ref_path: String::new(), job: None, themed: false };
        create_egui_editor(self.params.editor_state.clone(), state, |_ctx, _st| {}, |ctx, setter, st| draw(ctx, setter, st))
    }

    fn initialize(&mut self, _layout: &AudioIOLayout, cfg: &BufferConfig, _context: &mut impl InitContext<Self>) -> bool {
        self.sr = cfg.sample_rate;
        self.meter = Some(TonalMeter::new(cfg.sample_rate));
        true
    }

    fn reset(&mut self) {
        if let Some(m) = self.meter.as_mut() {
            m.reset();
        }
    }

    fn process(&mut self, buffer: &mut Buffer, _aux: &mut AuxiliaryBuffers, _context: &mut impl ProcessContext<Self>) -> ProcessStatus {
        let n = buffer.samples();
        let Some(meter) = self.meter.as_mut() else { return ProcessStatus::Normal };
        if self.shared.reset.swap(false, Ordering::Relaxed) {
            meter.reset();
            self.shared.ready.store(false, Ordering::Relaxed);
        }
        // blend per analysis frame (hop = 1024 samples) so that the memory is `memory` seconds
        let frame_s = 1024.0 / self.sr.max(1.0);
        meter.blend = (frame_s / self.params.memory.value().max(0.5)).clamp(0.0005, 0.5);
        let ch = buffer.as_slice();
        if ch.len() >= 2 {
            let (a, b) = ch.split_at_mut(1);
            meter.process(&a[0][..n], &b[0][..n]);
        }
        self.since_publish += n;
        if self.since_publish >= 2048 {
            self.since_publish = 0;
            if meter.is_ready() {
                let shape = meter.shape_db();
                for (i, v) in shape.iter().enumerate() {
                    self.shared.shape[i].store(*v, Ordering::Relaxed);
                }
                self.shared.ready.store(true, Ordering::Relaxed);
            }
        }
        ProcessStatus::Normal
    }
}

impl ClapPlugin for TonalBalance {
    const CLAP_ID: &'static str = "com.crowelian.airzone26.tonalbalance";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("Tonal balance meter with genre and reference targets");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[ClapFeature::AudioEffect, ClapFeature::Analyzer, ClapFeature::Mastering, ClapFeature::Stereo];
}

impl Vst3Plugin for TonalBalance {
    const VST3_CLASS_ID: [u8; 16] = *b"AirZone26TonalBl";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::Analyzer];
}

nih_export_clap!(TonalBalance);
nih_export_vst3!(TonalBalance);

// ---------------------------------------------------------------------------------- editor

const BG0: Color32 = Color32::from_rgb(16, 18, 22);
const BG1: Color32 = Color32::from_rgb(24, 27, 33);
const BG2: Color32 = Color32::from_rgb(34, 38, 46);
const ACCENT: Color32 = Color32::from_rgb(64, 214, 190);
const AMBER: Color32 = Color32::from_rgb(255, 170, 70);
const DIM: Color32 = Color32::from_rgb(140, 148, 160);
const RED: Color32 = Color32::from_rgb(235, 80, 80);
const GREEN: Color32 = Color32::from_rgb(90, 210, 120);

type ShapeJob = Arc<Mutex<Option<Result<Vec<f32>, String>>>>;

struct GuiState {
    params: Arc<TonalParams>,
    shared: Arc<Shared>,
    status: String,
    ref_path: String,
    job: Option<ShapeJob>,
    themed: bool,
}

/// Decode a WAV file and return its zero-mean tonal shape.
fn shape_from_wav(path: &Path) -> Result<Vec<f32>, String> {
    let mut reader = hound::WavReader::open(path).map_err(|e| format!("cannot open: {e}"))?;
    let spec = reader.spec();
    let ch = spec.channels.max(1) as usize;
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().filter_map(|s| s.ok()).collect(),
        hound::SampleFormat::Int => {
            let scale = 1.0 / (1u64 << (spec.bits_per_sample - 1)) as f32;
            reader.samples::<i32>().filter_map(|s| s.ok()).map(|v| v as f32 * scale).collect()
        }
    };
    let frames = samples.len() / ch;
    if frames < spec.sample_rate as usize {
        return Err("file is too short (need at least 1 s)".into());
    }
    let mut l = Vec::with_capacity(frames);
    let mut r = Vec::with_capacity(frames);
    for f in 0..frames {
        let a = samples[f * ch];
        l.push(a);
        r.push(if ch > 1 { samples[f * ch + 1] } else { a });
    }
    let mut an = Analyzer::new(spec.sample_rate as f32);
    for (a, b) in l.chunks(4096).zip(r.chunks(4096)) {
        an.process(a, b);
    }
    if !an.profile().is_valid() {
        return Err("not enough audio to analyse".into());
    }
    Ok(an.profile().shape_db().to_vec())
}

fn band_freq(b: usize) -> f32 {
    20.0 * 1000f32.powf(b as f32 / (MATCH_BANDS - 1) as f32)
}

fn median_offset(shape: &[f32; MATCH_BANDS], target: &[f32; MATCH_BANDS]) -> f32 {
    let mut d: Vec<f32> = (0..MATCH_BANDS).filter(|b| (90.0..=12000.0).contains(&band_freq(*b))).map(|b| shape[b] - target[b]).collect();
    if d.is_empty() {
        return 0.0;
    }
    d.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    d[d.len() / 2]
}

fn to_array(v: &[f32]) -> Option<[f32; MATCH_BANDS]> {
    if v.len() != MATCH_BANDS {
        return None;
    }
    let mut a = [0.0f32; MATCH_BANDS];
    a.copy_from_slice(v);
    Some(a)
}

fn draw(ctx: &egui::Context, setter: &ParamSetter, st: &mut GuiState) {
    if !st.themed {
        let mut v = egui::Visuals::dark();
        v.panel_fill = BG1;
        v.window_fill = BG1;
        v.extreme_bg_color = BG0;
        v.widgets.inactive.bg_fill = BG2;
        ctx.set_visuals(v);
        st.themed = true;
    }
    ctx.request_repaint_after(Duration::from_millis(50));
    // finished WAV analysis
    let done = st.job.as_ref().and_then(|j| j.lock().take());
    if let Some(res) = done {
        st.job = None;
        match res {
            Ok(shape) => {
                *st.params.custom.lock() = shape;
                st.status = "Reference loaded as custom target.".into();
                setter.begin_set_parameter(&st.params.mode);
                setter.set_parameter(&st.params.mode, TargetModeP::Custom);
                setter.end_set_parameter(&st.params.mode);
            }
            Err(e) => st.status = format!("Could not load reference: {e}"),
        }
    }

    let params = st.params.clone();
    let shared = st.shared.clone();
    egui::CentralPanel::default().show(ctx, |ui| {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("AIRZONE 26 TONAL BALANCE").size(18.0).strong().color(ACCENT));
                ui.label(RichText::new(format!("v{}  by crowelian 2026", VERSION)).size(11.0).color(DIM));
            });
            ui.label(RichText::new("Pass-through meter: your audio is not changed. Play the loudest part of the song and compare each region with the target.").size(11.0).color(DIM));
            ui.separator();

            // ---- target selection
            ui.horizontal_wrapped(|ui| {
                combo_enum(ui, setter, &params.mode, "tb_mode");
                if params.mode.value() == TargetModeP::Genre {
                    combo_enum(ui, setter, &params.genre, "tb_genre");
                }
            });
            ui.horizontal_wrapped(|ui| {
                ui.add(nih_plug_egui::widgets::ParamSlider::for_param(&params.tolerance, setter).with_width(150.0));
                ui.add(nih_plug_egui::widgets::ParamSlider::for_param(&params.memory, setter).with_width(150.0));
                if ui.button("Restart analysis").clicked() {
                    shared.reset.store(true, Ordering::Relaxed);
                }
            });
            ui.horizontal_wrapped(|ui| {
                if ui.button("Capture current balance as custom target").clicked() {
                    if shared.ready.load(Ordering::Relaxed) {
                        let v: Vec<f32> = shared.shape.iter().map(|a| a.load(Ordering::Relaxed)).collect();
                        *params.custom.lock() = v;
                        st.status = "Captured the current balance as the custom target.".into();
                        setter.begin_set_parameter(&params.mode);
                        setter.set_parameter(&params.mode, TargetModeP::Custom);
                        setter.end_set_parameter(&params.mode);
                    } else {
                        st.status = "Nothing to capture yet - play some audio first.".into();
                    }
                }
                if ui.button("Clear custom target").clicked() {
                    params.custom.lock().clear();
                    st.status = "Custom target cleared.".into();
                }
            });
            ui.horizontal_wrapped(|ui| {
                ui.add(egui::TextEdit::singleline(&mut st.ref_path).desired_width(300.0).hint_text("path to a reference WAV file"));
                if ui.button("Load reference").clicked() {
                    let path = std::path::PathBuf::from(st.ref_path.trim().trim_matches('"'));
                    let job: ShapeJob = Arc::new(Mutex::new(None));
                    st.job = Some(job.clone());
                    st.status = "Analysing reference...".into();
                    std::thread::spawn(move || {
                        *job.lock() = Some(shape_from_wav(&path));
                    });
                }
            });
            if !st.status.is_empty() {
                ui.label(RichText::new(st.status.clone()).size(11.0).color(AMBER));
            }
            ui.separator();

            // ---- analysis
            if !shared.ready.load(Ordering::Relaxed) {
                ui.label(RichText::new("Waiting for audio... start playback.").color(DIM));
                return;
            }
            let measured_raw: Vec<f32> = shared.shape.iter().map(|a| a.load(Ordering::Relaxed)).collect();
            let Some(measured) = to_array(&measured_raw) else { return };
            let target: [f32; MATCH_BANDS] = match params.mode.value() {
                TargetModeP::Genre => params.genre.value().genre().target_shape(),
                TargetModeP::Custom => match to_array(&params.custom.lock()) {
                    Some(t) => t,
                    None => {
                        ui.colored_label(AMBER, "No custom target yet: capture the current balance or load a reference WAV. Showing the Universal genre target meanwhile.");
                        Genre::Universal.target_shape()
                    }
                },
            };
            let off = median_offset(&measured, &target);
            let mut aligned = measured;
            aligned.iter_mut().for_each(|x| *x -= off);
            let tol = params.tolerance.value();
            let lm = region_levels(&aligned);
            let lt = region_levels(&target);

            ui.label(RichText::new("Balance per region (0 dB = on target, grey band = tolerance)").size(11.0).color(DIM));
            let mut advice: Vec<String> = Vec::new();
            for (i, (name, lo, hi)) in REGIONS.iter().enumerate() {
                let dev = lm[i] - lt[i];
                gauge(ui, &format!("{} ({:.0}-{:.0} Hz)", name, lo, hi), dev, tol);
                if dev.abs() > tol {
                    advice.push(format!(
                        "{}: {:.1} dB {} the target - consider {} around {:.0}-{:.0} Hz.",
                        name,
                        dev.abs(),
                        if dev > 0.0 { "above" } else { "below" },
                        if dev > 0.0 { "reducing" } else { "lifting" },
                        lo,
                        hi
                    ));
                }
            }
            ui.add_space(4.0);
            if advice.is_empty() {
                ui.colored_label(GREEN, "All regions are inside the target range.");
            } else {
                for a in advice {
                    ui.colored_label(AMBER, a);
                }
            }
            ui.add_space(6.0);
            curve(ui, &aligned, &target, tol);
        });
    });
}

fn combo_enum<E: Enum + PartialEq + Copy + 'static>(ui: &mut egui::Ui, setter: &ParamSetter, p: &EnumParam<E>, id: &str) {
    let names = E::variants();
    let cur = p.value().to_index();
    egui::ComboBox::from_id_salt(id).selected_text(names[cur]).width(220.0).show_ui(ui, |ui| {
        for (i, n) in names.iter().enumerate() {
            if ui.selectable_label(i == cur, *n).clicked() {
                setter.begin_set_parameter(p);
                setter.set_parameter(p, E::from_index(i));
                setter.end_set_parameter(p);
            }
        }
    });
}

/// One region gauge: deviation from the target in dB between -9 and +9.
fn gauge(ui: &mut egui::Ui, label: &str, dev: f32, tol: f32) {
    ui.horizontal(|ui| {
        ui.add_sized([150.0, 20.0], egui::Label::new(RichText::new(label).size(12.0)));
        let w = (ui.available_width() - 70.0).clamp(120.0, 520.0);
        let (rect, _) = ui.allocate_exact_size(vec2(w, 22.0), Sense::hover());
        let p = ui.painter_at(rect);
        p.rect_filled(rect, 3.0, BG0);
        let x_of = |d: f32| rect.center().x + (d.clamp(-9.0, 9.0) / 9.0) * rect.width() * 0.5;
        let band = egui::Rect::from_min_max(Pos2::new(x_of(-tol), rect.top()), Pos2::new(x_of(tol), rect.bottom()));
        p.rect_filled(band, 2.0, Color32::from_rgb(48, 70, 62));
        p.line_segment([Pos2::new(rect.center().x, rect.top()), Pos2::new(rect.center().x, rect.bottom())], Stroke::new(1.0_f32, DIM));
        let col = if dev.abs() <= tol {
            GREEN
        } else if dev.abs() <= tol * 2.0 {
            AMBER
        } else {
            RED
        };
        p.circle_filled(Pos2::new(x_of(dev), rect.center().y), 7.0, col);
        ui.label(RichText::new(format!("{:+.1} dB", dev)).monospace().size(12.0));
    });
}

/// Measured shape (teal) against the target (amber) with its tolerance band.
fn curve(ui: &mut egui::Ui, measured: &[f32; MATCH_BANDS], target: &[f32; MATCH_BANDS], tol: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 190.0), Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 4.0, BG0);
    let range = 20.0;
    let y_of = |db: f32| rect.center().y - (db.clamp(-range, range) / range) * rect.height() * 0.5;
    let x_of = |i: usize| rect.left() + rect.width() * i as f32 / (MATCH_BANDS - 1) as f32;
    for f in [50.0, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0, 10000.0] {
        let t = (f / 20.0f32).ln() / 1000f32.ln();
        let x = rect.left() + rect.width() * t;
        p.line_segment([Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())], Stroke::new(1.0_f32, BG2));
        let label = if f >= 1000.0 { format!("{}k", f / 1000.0) } else { format!("{}", f) };
        p.text(Pos2::new(x + 2.0, rect.bottom() - 12.0), egui::Align2::LEFT_TOP, label, egui::FontId::proportional(9.0), DIM);
    }
    p.line_segment([Pos2::new(rect.left(), rect.center().y), Pos2::new(rect.right(), rect.center().y)], Stroke::new(1.0_f32, BG2));
    for i in 0..MATCH_BANDS - 1 {
        let r = egui::Rect::from_min_max(Pos2::new(x_of(i), y_of(target[i] + tol)), Pos2::new(x_of(i + 1), y_of(target[i] - tol)));
        p.rect_filled(r, 0.0, Color32::from_rgba_unmultiplied(255, 170, 70, 36));
    }
    let tgt: Vec<Pos2> = (0..MATCH_BANDS).map(|i| Pos2::new(x_of(i), y_of(target[i]))).collect();
    let mea: Vec<Pos2> = (0..MATCH_BANDS).map(|i| Pos2::new(x_of(i), y_of(measured[i]))).collect();
    p.add(egui::Shape::line(tgt, Stroke::new(1.5_f32, AMBER)));
    p.add(egui::Shape::line(mea, Stroke::new(2.0_f32, ACCENT)));
    p.text(Pos2::new(rect.left() + 6.0, rect.top() + 4.0), egui::Align2::LEFT_TOP, "teal = your mix, amber = target (shaded = tolerance)", egui::FontId::proportional(10.0), DIM);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn params_are_consistent() {
        let p = TonalParams::default();
        assert_eq!(p.mode.value(), TargetModeP::Genre);
        assert_eq!(GenreP::HipHop.genre(), Genre::HipHop);
        assert_eq!(<GenreP as Enum>::variants().len(), Genre::ALL.len());
    }

    #[test]
    fn array_conversion_checks_length() {
        assert!(to_array(&[0.0; 3]).is_none());
        assert!(to_array(&[0.0; MATCH_BANDS]).is_some());
    }
}

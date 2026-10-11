//! Small reusable egui widgets and drawing helpers.

use nih_plug::prelude::*;
use nih_plug_egui::egui::{self, Align2, Color32, FontId, Pos2, Rect, RichText, Sense, Stroke, Ui};
use nih_plug_egui::widgets::ParamSlider;

pub const ACCENT: Color32 = Color32::from_rgb(64, 214, 190);
pub const ACCENT2: Color32 = Color32::from_rgb(255, 170, 70);
pub const BG0: Color32 = Color32::from_rgb(16, 18, 22);
pub const BG1: Color32 = Color32::from_rgb(24, 27, 33);
pub const BG2: Color32 = Color32::from_rgb(34, 38, 46);
pub const TEXT_DIM: Color32 = Color32::from_rgb(140, 148, 160);
pub const RED: Color32 = Color32::from_rgb(235, 80, 80);
pub const GREEN: Color32 = Color32::from_rgb(90, 210, 120);

pub const LABEL_W: f32 = 118.0;

pub fn slider<P: Param>(ui: &mut Ui, setter: &ParamSetter, p: &P) {
    ui.horizontal(|ui| {
        ui.add_sized([LABEL_W, 18.0], egui::Label::new(RichText::new(p.name()).color(TEXT_DIM)));
        ui.add(ParamSlider::for_param(p, setter).with_width(170.0));
    });
}

pub fn toggle(ui: &mut Ui, setter: &ParamSetter, p: &BoolParam) {
    let mut v = p.value();
    if ui.checkbox(&mut v, p.name()).changed() {
        setter.begin_set_parameter(p);
        setter.set_parameter(p, v);
        setter.end_set_parameter(p);
    }
}

pub fn combo<E: Enum + PartialEq + Copy + 'static>(ui: &mut Ui, setter: &ParamSetter, p: &EnumParam<E>, id: &str) {
    let names = E::variants();
    let cur = p.value().to_index();
    ui.horizontal(|ui| {
        ui.add_sized([LABEL_W, 18.0], egui::Label::new(RichText::new(p.name()).color(TEXT_DIM)));
        egui::ComboBox::from_id_salt(id).selected_text(names[cur]).width(150.0).show_ui(ui, |ui| {
            for (i, n) in names.iter().enumerate() {
                if ui.selectable_label(i == cur, *n).clicked() {
                    setter.begin_set_parameter(p);
                    setter.set_parameter(p, E::from_index(i));
                    setter.end_set_parameter(p);
                }
            }
        });
    });
}

/// Compact combo without the label column (for tables).
pub fn combo_small<E: Enum + PartialEq + Copy + 'static>(ui: &mut Ui, setter: &ParamSetter, p: &EnumParam<E>, id: String, w: f32) {
    let names = E::variants();
    let cur = p.value().to_index();
    egui::ComboBox::from_id_salt(id).selected_text(names[cur]).width(w).show_ui(ui, |ui| {
        for (i, n) in names.iter().enumerate() {
            if ui.selectable_label(i == cur, *n).clicked() {
                setter.begin_set_parameter(p);
                setter.set_parameter(p, E::from_index(i));
                setter.end_set_parameter(p);
            }
        }
    });
}

pub fn set_float(setter: &ParamSetter, p: &FloatParam, v: f32) {
    setter.set_parameter(p, v.clamp(p_min(p), p_max(p)));
}

fn p_min(p: &FloatParam) -> f32 {
    p.preview_plain(0.0)
}
fn p_max(p: &FloatParam) -> f32 {
    p.preview_plain(1.0)
}

/// Horizontal level bar. `value`/`lo`/`hi` in the same unit (usually dB).
pub fn bar(ui: &mut Ui, label: &str, text: &str, value: f32, lo: f32, hi: f32, color: Color32, width: f32) {
    ui.horizontal(|ui| {
        ui.add_sized([46.0, 14.0], egui::Label::new(RichText::new(label).size(11.0).color(TEXT_DIM)));
        let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 12.0), Sense::hover());
        let p = ui.painter();
        p.rect_filled(rect, 2.0, BG0);
        let t = ((value - lo) / (hi - lo)).clamp(0.0, 1.0);
        let fill = Rect::from_min_max(rect.min, Pos2::new(rect.min.x + rect.width() * t, rect.max.y));
        p.rect_filled(fill, 2.0, color);
        ui.label(RichText::new(text).size(11.0).monospace());
    });
}

/// Vertical gain-reduction style meter (fills downward from the top).
pub fn gr_meter(ui: &mut Ui, label: &str, gr_db: f32, max_db: f32) {
    ui.vertical(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(16.0, 60.0), Sense::hover());
        let p = ui.painter();
        p.rect_filled(rect, 2.0, BG0);
        let t = (gr_db / max_db).clamp(0.0, 1.0);
        let fill = Rect::from_min_max(rect.min, Pos2::new(rect.max.x, rect.min.y + rect.height() * t));
        p.rect_filled(fill, 2.0, ACCENT2);
        ui.label(RichText::new(label).size(10.0).color(TEXT_DIM));
        ui.label(RichText::new(format!("{:.1}", gr_db)).size(10.0).monospace());
    });
}

pub fn freq_to_x(f: f32, rect: Rect) -> f32 {
    let t = (f.max(20.0) / 20.0).ln() / 1000f32.ln();
    rect.left() + rect.width() * t.clamp(0.0, 1.0)
}

pub fn x_to_freq(x: f32, rect: Rect) -> f32 {
    let t = ((x - rect.left()) / rect.width()).clamp(0.0, 1.0);
    20.0 * 1000f32.powf(t)
}

pub fn draw_freq_grid(ui: &Ui, rect: Rect) {
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 4.0, BG0);
    for f in [30.0, 50.0, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0, 10000.0, 20000.0] {
        let x = freq_to_x(f, rect);
        let major = matches!(f as i32, 100 | 1000 | 10000);
        p.line_segment(
            [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
            Stroke::new(1.0_f32, if major { BG2 } else { Color32::from_rgb(26, 29, 35) }),
        );
        if major || f == 50.0 || f == 500.0 || f == 5000.0 || f == 20000.0 {
            let label = if f >= 1000.0 { format!("{}k", f / 1000.0) } else { format!("{}", f) };
            p.text(Pos2::new(x + 2.0, rect.bottom() - 11.0), Align2::LEFT_TOP, label, FontId::proportional(9.0), TEXT_DIM);
        }
    }
}

pub fn polyline(ui: &Ui, rect: Rect, pts: Vec<Pos2>, color: Color32, width: f32) {
    ui.painter_at(rect).add(egui::Shape::line(pts, Stroke::new(width, color)));
}

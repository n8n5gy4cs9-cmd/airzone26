//! Generates `docs/manual.html` from `docs/manual.template.html`, filling in the preset catalogue
//! and the complete parameter reference straight from the code so the manual cannot drift.
//!
//! Run: `cargo run -p airzone26 --example gen_docs`

use airzone26::params::AirZoneParams;
use airzone26::presets;
use nih_plug::prelude::*;

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn main() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs");
    let template = std::fs::read_to_string(root.join("manual.template.html")).expect("template");

    // ---- presets
    let presets = presets::factory();
    let mut cats: Vec<String> = Vec::new();
    for p in &presets {
        if !cats.contains(&p.category) {
            cats.push(p.category.clone());
        }
    }
    let mut ph = String::new();
    for c in &cats {
        ph.push_str(&format!("<h3>{}</h3>\n<table class=\"t\"><tr><th>Preset</th><th>What it does</th></tr>\n", esc(c)));
        for p in presets.iter().filter(|p| &p.category == c) {
            ph.push_str(&format!("<tr><td>{}</td><td>{}</td></tr>\n", esc(&p.name), esc(&p.description)));
        }
        ph.push_str("</table>\n");
    }

    // ---- parameters
    let params = AirZoneParams::default();
    let mut groups: Vec<(String, Vec<String>)> = Vec::new();
    for (id, ptr, group) in params.param_map() {
        let g = if group.is_empty() { "Global".to_string() } else { group.split('/').next().unwrap_or("").to_string() };
        let (name, unit, steps, min, max, def, def_s) = unsafe {
            let steps = ptr.step_count();
            let min = ptr.preview_plain(0.0);
            let max = ptr.preview_plain(1.0);
            let def = ptr.default_plain_value();
            let dn = ptr.default_normalized_value();
            (ptr.name().to_string(), ptr.unit().to_string(), steps, min, max, def, ptr.normalized_value_to_string(dn, true))
        };
        let range = match steps {
            Some(1) => "Off / On".to_string(),
            Some(n) if n > 1 => {
                let names: Vec<String> = (0..=n).map(|i| unsafe { ptr.normalized_value_to_string(i as f32 / n as f32, false) }).collect();
                names.join(" · ")
            }
            _ => format!("{} … {}{}", fmt(min), fmt(max), unit),
        };
        let _ = def;
        let row = format!("<tr><td>{}</td><td><code>{}</code></td><td>{}</td><td>{}</td></tr>\n", esc(&name), esc(&id), esc(&range), esc(&def_s));
        match groups.iter_mut().find(|(n, _)| *n == g) {
            Some((_, rows)) => rows.push(row),
            None => groups.push((g, vec![row])),
        }
    }
    let mut pa = String::new();
    for (g, rows) in &groups {
        pa.push_str(&format!("<details><summary>{} <span class=\"n\">{} parameters</span></summary>\n<table class=\"t\"><tr><th>Parameter</th><th>ID</th><th>Range</th><th>Default</th></tr>\n", esc(g), rows.len()));
        for r in rows {
            pa.push_str(r);
        }
        pa.push_str("</table></details>\n");
    }
    let total: usize = groups.iter().map(|(_, r)| r.len()).sum();

    let out = template
        .replace("<!--PRESETS-->", &ph)
        .replace("<!--PARAMS-->", &pa)
        .replace("{{PRESET_COUNT}}", &presets.len().to_string())
        .replace("{{PARAM_COUNT}}", &total.to_string())
        .replace("{{VERSION}}", env!("CARGO_PKG_VERSION"));
    std::fs::write(root.join("manual.html"), out).expect("write manual");
    println!("wrote docs/manual.html ({} presets, {} parameters)", presets.len(), total);
}

fn fmt(v: f32) -> String {
    if v.abs() >= 100.0 {
        format!("{:.0}", v)
    } else if (v - v.round()).abs() < 0.005 {
        format!("{:.0}", v)
    } else {
        format!("{:.2}", v)
    }
}

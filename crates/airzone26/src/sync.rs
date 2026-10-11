//! Copies parameter values into the DSP engine once per block.
//!
//! The mapping reads plain values by parameter id from a [`Values`] snapshot, so exactly the same
//! code path serves the real plugin and the headless preset tests.

use std::collections::HashMap;
use std::sync::Arc;

use airzone_dsp::biquad::FilterKind;
use airzone_dsp::chain::{sanitize_order, Engine};
use airzone_dsp::eq::Placement;
use airzone_dsp::maximizer::LimiterMode;
use airzone_dsp::pitch::{Key, PitchMode, ScaleKind};
use airzone_dsp::saturation::ShaperKind;
use airzone_dsp::vintage::TapeSpeed;
use nih_plug::prelude::*;

use crate::params::AirZoneParams;
use crate::presets::Preset;

/// Snapshot of every parameter's plain value, addressable by id.
pub struct Values {
    _params: Arc<AirZoneParams>,
    idx: HashMap<String, usize>,
    ptrs: Vec<ParamPtr>,
    pub vals: Vec<f32>,
}

// ParamPtr is a raw pointer into `_params`, which we keep alive.
unsafe impl Send for Values {}

impl Values {
    pub fn new(params: Arc<AirZoneParams>) -> Self {
        let map = params.param_map();
        let mut idx = HashMap::new();
        let mut ptrs = Vec::new();
        let mut vals = Vec::new();
        for (i, (id, ptr, _)) in map.into_iter().enumerate() {
            idx.insert(id, i);
            vals.push(unsafe { ptr.default_plain_value() });
            ptrs.push(ptr);
        }
        Self { _params: params, idx, ptrs, vals }
    }

    /// Re-read live values from the parameters (audio thread safe: no allocation).
    pub fn refresh(&mut self) {
        for (v, p) in self.vals.iter_mut().zip(self.ptrs.iter()) {
            *v = unsafe { p.modulated_plain_value() };
        }
    }

    /// Start from defaults and overlay a preset (values are clamped to each parameter's range).
    pub fn from_preset(params: Arc<AirZoneParams>, preset: &Preset) -> Self {
        let mut v = Self::new(params);
        for (id, plain) in &preset.values {
            if let Some(&i) = v.idx.get(id) {
                let n = unsafe { v.ptrs[i].preview_normalized(*plain) }.clamp(0.0, 1.0);
                v.vals[i] = unsafe { v.ptrs[i].preview_plain(n) };
            }
        }
        v
    }

    #[inline]
    pub fn f(&self, id: &str) -> f32 {
        match self.idx.get(id) {
            Some(&i) => self.vals[i],
            None => {
                debug_assert!(false, "unknown parameter id {id}");
                0.0
            }
        }
    }
    #[inline]
    pub fn b(&self, id: &str) -> bool {
        self.f(id) > 0.5
    }
    #[inline]
    pub fn i(&self, id: &str) -> usize {
        self.f(id).round().max(0.0) as usize
    }
}

fn kind(i: usize) -> FilterKind {
    match i {
        0 => FilterKind::Bell,
        1 => FilterKind::LowShelf,
        2 => FilterKind::HighShelf,
        3 => FilterKind::HighPass,
        4 => FilterKind::LowPass,
        _ => FilterKind::Notch,
    }
}

fn place(i: usize) -> Placement {
    match i {
        0 => Placement::Stereo,
        1 => Placement::Mid,
        2 => Placement::Side,
        3 => Placement::Left,
        _ => Placement::Right,
    }
}

pub fn limiter_mode(i: usize) -> LimiterMode {
    LimiterMode::ALL[i.min(4)]
}

const SWITCH_IDS: [&str; 21] = [
    "on_unlimiter",
    "on_stem",
    "on_match",
    "on_eq",
    "on_dyneq",
    "on_stab",
    "on_shaper",
    "on_bass",
    "on_lef",
    "on_dyn",
    "on_vcomp",
    "on_veq",
    "on_vtape",
    "on_exciter",
    "on_impact",
    "on_imager",
    "on_delay",
    "on_reverb",
    "on_vlim",
    "on_max",
    "on_pitch",
];

pub fn apply(v: &Values, order: Option<&[u8]>, e: &mut Engine) {
    e.bypass = v.b("bypass");
    e.input_gain_db = v.f("in_gain");
    e.output_gain_db = v.f("out_gain");
    e.dither.bits = [0, 16, 20, 24][v.i("dither").min(3)];
    for (i, id) in SWITCH_IDS.iter().enumerate() {
        e.enabled[i] = v.b(id);
    }
    if let Some(o) = order {
        e.order = sanitize_order(o);
    }

    e.unlimiter.amount = v.f("un_amount");
    e.unlimiter.max_boost_db = v.f("un_boost");
    e.unlimiter.threshold_db = v.f("un_thresh");
    e.unlimiter.release_ms = v.f("un_release");
    e.unlimiter.compensate = v.b("un_comp");

    let st = &mut e.hub.stem;
    st.stems[0].gain_db = v.f("st_g0");
    st.stems[1].gain_db = v.f("st_g1");
    st.stems[2].gain_db = v.f("st_g2");
    st.stems[3].gain_db = v.f("st_g3");
    st.stems[0].tilt_db = v.f("st_t0");
    st.stems[1].tilt_db = v.f("st_t1");
    st.stems[2].tilt_db = v.f("st_t2");
    st.stems[3].tilt_db = v.f("st_t3");
    st.focus = v.f("st_focus");
    st.solo = match v.i("st_solo") {
        0 => None,
        n => Some(n - 1),
    };

    let me = &mut e.hub.match_eq;
    if me.amount != v.f("mq_amount") || me.smoothing != v.f("mq_smooth") || me.max_gain_db != v.f("mq_maxgain") {
        me.mark_dirty();
    }
    me.amount = v.f("mq_amount");
    me.smoothing = v.f("mq_smooth");
    me.max_gain_db = v.f("mq_maxgain");

    for (i, b) in e.eq.bands.iter_mut().enumerate() {
        let n = i + 1;
        b.enabled = v.b(&format!("eq_on_{n}"));
        b.kind = kind(v.i(&format!("eq_kind_{n}")));
        b.freq = v.f(&format!("eq_freq_{n}"));
        b.gain_db = v.f(&format!("eq_gain_{n}"));
        b.q = v.f(&format!("eq_q_{n}"));
        b.order = 2 + 2 * v.i(&format!("eq_slope_{n}")).min(3);
        b.placement = place(v.i(&format!("eq_place_{n}")));
    }
    for (i, b) in e.dyneq.bands.iter_mut().enumerate() {
        let n = i + 1;
        b.enabled = v.b(&format!("dq_dq_on_{n}"));
        b.freq = v.f(&format!("dq_dq_freq_{n}"));
        b.q = v.f(&format!("dq_dq_q_{n}"));
        b.gain_db = v.f(&format!("dq_dq_gain_{n}"));
        b.threshold_db = v.f(&format!("dq_dq_thresh_{n}"));
        b.range_db = v.f(&format!("dq_dq_range_{n}"));
        b.ratio = v.f(&format!("dq_dq_ratio_{n}"));
        b.attack_ms = v.f(&format!("dq_dq_att_{n}"));
        b.release_ms = v.f(&format!("dq_dq_rel_{n}"));
    }

    let s = &mut e.hub.stabilizer;
    s.amount = v.f("sb_amount");
    s.range_db = v.f("sb_range");
    s.slope_db_oct = v.f("sb_slope");
    s.speed = v.f("sb_speed");
    s.use_reference = v.b("sb_useref");

    let s = &mut e.hub.shaper;
    s.depth = v.f("ss_depth");
    s.threshold_db = v.f("ss_thresh");
    s.width = v.f("ss_width");
    s.low_hz = v.f("ss_low");
    s.high_hz = v.f("ss_high");
    s.tilt_db_oct = v.f("ss_tilt");
    s.delta = v.b("ss_delta");

    e.bass.sub_freq = v.f("bc_subf");
    e.bass.bass_freq = v.f("bc_bassf");
    e.bass.sub_gain_db = v.f("bc_subg");
    e.bass.bass_gain_db = v.f("bc_bassg");
    e.bass.sub_mono = v.f("bc_submono");
    e.bass.bass_mono = v.f("bc_bassmono");
    e.bass.punch_db = v.f("bc_punch");
    e.bass.tighten = v.f("bc_tight");
    e.bass.auto_balance = v.b("bc_auto");
    e.bass.target_ratio_db = v.f("bc_ratio");
    e.bass.mix = v.f("bc_mix");

    e.lef.freq = v.f("lf_freq");
    e.lef.punch_db = v.f("lf_punch");
    e.lef.sustain_db = v.f("lf_sustain");
    e.lef.contrast = v.f("lf_contrast");
    e.lef.mix = v.f("lf_mix");

    e.dynamics.crossovers = [v.f("dy_xo1"), v.f("dy_xo2")];
    e.dynamics.link = v.f("dy_link");
    e.dynamics.mid_side = v.b("dy_ms");
    e.dynamics.output_db = v.f("dy_out");
    e.dynamics.mix = v.f("dy_mix");
    for (i, b) in e.dynamics.bands.iter_mut().enumerate() {
        let n = i + 1;
        b.enabled = v.b(&format!("dy_dy_on_{n}"));
        b.threshold_db = v.f(&format!("dy_dy_thresh_{n}"));
        b.ratio = v.f(&format!("dy_dy_ratio_{n}"));
        b.knee_db = v.f(&format!("dy_dy_knee_{n}"));
        b.attack_ms = v.f(&format!("dy_dy_att_{n}"));
        b.release_ms = v.f(&format!("dy_dy_rel_{n}"));
        b.gain_db = v.f(&format!("dy_dy_gain_{n}"));
        b.expand_threshold_db = v.f(&format!("dy_dy_expth_{n}"));
        b.expand_ratio = v.f(&format!("dy_dy_exprat_{n}"));
    }

    e.vcomp.input_db = v.f("vc_in");
    e.vcomp.threshold_db = v.f("vc_thresh");
    e.vcomp.ratio = v.f("vc_ratio");
    e.vcomp.attack_ms = v.f("vc_att");
    e.vcomp.release_ms = v.f("vc_rel");
    e.vcomp.makeup_db = v.f("vc_makeup");
    e.vcomp.warmth = v.f("vc_warm");
    e.vcomp.mix = v.f("vc_mix");

    e.veq.low_freq = v.i("ve_lf");
    e.veq.low_boost = v.f("ve_lb");
    e.veq.low_atten = v.f("ve_la");
    e.veq.high_freq = v.i("ve_hf");
    e.veq.high_boost = v.f("ve_hb");
    e.veq.bandwidth = v.f("ve_bw");
    e.veq.atten_freq = v.i("ve_af");
    e.veq.high_atten = v.f("ve_ha");
    e.veq.tube = v.f("ve_tube");

    e.vtape.drive_db = v.f("vt_drive");
    e.vtape.speed = [TapeSpeed::Ips7_5, TapeSpeed::Ips15, TapeSpeed::Ips30][v.i("vt_speed").min(2)];
    e.vtape.saturation = v.f("vt_sat");
    e.vtape.wow_flutter = v.f("vt_wow");
    e.vtape.noise = v.f("vt_noise");
    e.vtape.age = v.f("vt_age");
    e.vtape.output_db = v.f("vt_out");

    e.exciter.crossovers = [v.f("ex_xo1"), v.f("ex_xo2"), v.f("ex_xo3")];
    e.exciter.kind = [ShaperKind::Tape, ShaperKind::Tube, ShaperKind::Warm, ShaperKind::Retro][v.i("ex_mode").min(3)];
    e.exciter.mix = v.f("ex_mix");
    for (i, b) in e.exciter.bands.iter_mut().enumerate() {
        let n = i + 1;
        b.enabled = v.b(&format!("ex_ex_on_{n}"));
        b.drive_db = v.f(&format!("ex_ex_drive_{n}"));
        b.amount = v.f(&format!("ex_ex_amount_{n}"));
        b.gain_db = v.f(&format!("ex_ex_gain_{n}"));
    }

    e.impact.crossovers = [v.f("im_xo1"), v.f("im_xo2")];
    e.impact.mix = v.f("im_mix");
    for (i, b) in e.impact.bands.iter_mut().enumerate() {
        b.attack_db = v.f(&format!("im_im_att_{}", i + 1));
        b.sustain_db = v.f(&format!("im_im_sus_{}", i + 1));
    }

    e.imager.crossovers = [v.f("ig_xo1"), v.f("ig_xo2"), v.f("ig_xo3")];
    e.imager.stereoize = v.f("ig_stz");
    e.imager.balance = v.f("ig_bal");
    for (i, w) in e.imager.widths.iter_mut().enumerate() {
        *w = v.f(&format!("ig_ig_width_{}", i + 1));
    }

    e.delay.time_l_ms = v.f("dl_tl");
    e.delay.time_r_ms = v.f("dl_tr");
    e.delay.feedback = v.f("dl_fb");
    e.delay.ping_pong = v.b("dl_pp");
    e.delay.lowcut_hz = v.f("dl_lc");
    e.delay.highcut_hz = v.f("dl_hc");
    e.delay.mix = v.f("dl_mix");

    e.reverb.room_size = v.f("rv_room");
    e.reverb.damping = v.f("rv_damp");
    e.reverb.width = v.f("rv_width");
    e.reverb.predelay_ms = v.f("rv_pre");
    e.reverb.lowcut_hz = v.f("rv_lc");
    e.reverb.mix = v.f("rv_mix");

    e.vlimiter.input_db = v.f("vl_in");
    e.vlimiter.output_db = v.f("vl_out");
    e.vlimiter.attack_ms = v.f("vl_att");
    e.vlimiter.release_ms = v.f("vl_rel");
    e.vlimiter.character = v.f("vl_char");

    e.maximizer.mode = limiter_mode(v.i("mx_mode"));
    e.maximizer.gain_db = v.f("mx_gain");
    e.maximizer.ceiling_db = v.f("mx_ceil");
    e.maximizer.release_ms = v.f("mx_rel");
    e.maximizer.character = v.f("mx_char");
    e.maximizer.independence = v.f("mx_indep");
    e.maximizer.true_peak = v.b("mx_tp");

    let mode = v.i("pc_mode");
    e.pitch_full_song = mode >= 2;
    let ps = &mut e.pitch.settings;
    ps.mode = if mode == 1 { PitchMode::Instrument } else { PitchMode::Vocal };
    ps.key = Key { root: v.i("pc_root").min(11) as u8, scale: ScaleKind::ALL[v.i("pc_scale").min(8)] };
    ps.a4 = v.f("pc_a4");
    ps.retune_ms = v.f("pc_retune");
    ps.amount = v.f("pc_amount");
    ps.preserve = v.f("pc_preserve");
    ps.tolerance_cents = v.f("pc_tol");
    ps.max_shift_cents = v.f("pc_maxshift");
    ps.min_hz = v.f("pc_minhz");
    ps.max_hz = v.f("pc_maxhz");
    ps.confidence = v.f("pc_conf");
    let t = &mut e.hub.tuning;
    t.auto = v.b("pc_tauto");
    t.manual_cents = v.f("pc_tmanual");
    t.target_a4 = v.f("pc_ttarget");
    t.amount = v.f("pc_tamount");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_id_used_by_sync_exists() {
        // debug_assert in Values::f panics on unknown ids
        let params = Arc::new(AirZoneParams::default());
        let v = Values::new(params);
        let mut e = Engine::new(48000.0, 512);
        apply(&v, None, &mut e);
        // defaults must keep the chain neutral
        assert!(e.enabled.iter().all(|x| !*x));
    }
}

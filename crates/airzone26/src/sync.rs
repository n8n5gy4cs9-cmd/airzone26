//! Copies parameter values into the DSP engine once per block.

use airzone_dsp::biquad::FilterKind;
use airzone_dsp::chain::{sanitize_order, Engine, ModuleId};
use airzone_dsp::eq::Placement;
use airzone_dsp::maximizer::LimiterMode;
use airzone_dsp::saturation::ShaperKind;
use airzone_dsp::vintage::TapeSpeed;

use crate::params::*;

fn kind(k: KindP) -> FilterKind {
    match k {
        KindP::Bell => FilterKind::Bell,
        KindP::LowShelf => FilterKind::LowShelf,
        KindP::HighShelf => FilterKind::HighShelf,
        KindP::HighPass => FilterKind::HighPass,
        KindP::LowPass => FilterKind::LowPass,
        KindP::Notch => FilterKind::Notch,
    }
}

fn slope_order(s: SlopeP) -> usize {
    match s {
        SlopeP::S12 => 2,
        SlopeP::S24 => 4,
        SlopeP::S36 => 6,
        SlopeP::S48 => 8,
    }
}

fn place(p: PlaceP) -> Placement {
    match p {
        PlaceP::Stereo => Placement::Stereo,
        PlaceP::Mid => Placement::Mid,
        PlaceP::Side => Placement::Side,
        PlaceP::Left => Placement::Left,
        PlaceP::Right => Placement::Right,
    }
}

pub fn limiter_mode(m: LimiterP) -> LimiterMode {
    match m {
        LimiterP::Clean => LimiterMode::Clean,
        LimiterP::Balanced => LimiterMode::Balanced,
        LimiterP::Multiband => LimiterMode::Multiband,
        LimiterP::Aggressive => LimiterMode::Aggressive,
        LimiterP::Adaptive => LimiterMode::Adaptive,
    }
}

pub fn apply(p: &AirZoneParams, e: &mut Engine) {
    e.bypass = p.bypass.value();
    e.input_gain_db = p.input_gain.value();
    e.output_gain_db = p.output_gain.value();
    e.dither.bits = match p.dither.value() {
        DitherP::Off => 0,
        DitherP::B16 => 16,
        DitherP::B20 => 20,
        DitherP::B24 => 24,
    };
    for (i, s) in p.on.by_index().iter().enumerate() {
        e.enabled[i] = s.value();
    }
    if let Some(order) = p.order.try_read() {
        e.order = sanitize_order(&order);
    }

    let u = &p.unlimiter;
    e.unlimiter.amount = u.amount.value();
    e.unlimiter.max_boost_db = u.max_boost.value();
    e.unlimiter.threshold_db = u.threshold.value();
    e.unlimiter.release_ms = u.release.value();
    e.unlimiter.compensate = u.compensate.value();

    let s = &p.stem;
    let st = &mut e.hub.stem;
    st.stems[0].gain_db = s.bass_gain.value();
    st.stems[1].gain_db = s.drums_gain.value();
    st.stems[2].gain_db = s.vocals_gain.value();
    st.stems[3].gain_db = s.other_gain.value();
    st.stems[0].tilt_db = s.bass_tilt.value();
    st.stems[1].tilt_db = s.drums_tilt.value();
    st.stems[2].tilt_db = s.vocals_tilt.value();
    st.stems[3].tilt_db = s.other_tilt.value();
    st.focus = s.focus.value();
    st.solo = match s.solo.value() {
        SoloP::Off => None,
        SoloP::Bass => Some(0),
        SoloP::Drums => Some(1),
        SoloP::Vocals => Some(2),
        SoloP::Other => Some(3),
    };

    let m = &p.match_eq;
    let me = &mut e.hub.match_eq;
    if me.amount != m.amount.value() || me.smoothing != m.smoothing.value() || me.max_gain_db != m.max_gain.value() {
        me.mark_dirty();
    }
    me.amount = m.amount.value();
    me.smoothing = m.smoothing.value();
    me.max_gain_db = m.max_gain.value();

    for (b, bp) in e.eq.bands.iter_mut().zip(p.eq.bands.iter()) {
        b.enabled = bp.on.value();
        b.kind = kind(bp.kind.value());
        b.freq = bp.freq.value();
        b.gain_db = bp.gain.value();
        b.q = bp.q.value();
        b.order = slope_order(bp.slope.value());
        b.placement = place(bp.place.value());
    }
    for (b, bp) in e.dyneq.bands.iter_mut().zip(p.dyneq.bands.iter()) {
        b.enabled = bp.on.value();
        b.freq = bp.freq.value();
        b.q = bp.q.value();
        b.gain_db = bp.gain.value();
        b.threshold_db = bp.threshold.value();
        b.range_db = bp.range.value();
        b.ratio = bp.ratio.value();
        b.attack_ms = bp.attack.value();
        b.release_ms = bp.release.value();
    }

    let sb = &p.stab;
    let st = &mut e.hub.stabilizer;
    st.amount = sb.amount.value();
    st.range_db = sb.range.value();
    st.slope_db_oct = sb.slope.value();
    st.speed = sb.speed.value();
    st.use_reference = sb.use_ref.value();

    let ss = &p.shaper;
    let sh = &mut e.hub.shaper;
    sh.depth = ss.depth.value();
    sh.threshold_db = ss.threshold.value();
    sh.width = ss.width.value();
    sh.low_hz = ss.low.value();
    sh.high_hz = ss.high.value();
    sh.tilt_db_oct = ss.tilt.value();
    sh.delta = ss.delta.value();

    let b = &p.bass;
    e.bass.sub_freq = b.sub_freq.value();
    e.bass.bass_freq = b.bass_freq.value();
    e.bass.sub_gain_db = b.sub_gain.value();
    e.bass.bass_gain_db = b.bass_gain.value();
    e.bass.sub_mono = b.sub_mono.value();
    e.bass.bass_mono = b.bass_mono.value();
    e.bass.punch_db = b.punch.value();
    e.bass.tighten = b.tighten.value();
    e.bass.auto_balance = b.auto_balance.value();
    e.bass.target_ratio_db = b.target_ratio.value();
    e.bass.mix = b.mix.value();

    let l = &p.lef;
    e.lef.freq = l.freq.value();
    e.lef.punch_db = l.punch.value();
    e.lef.sustain_db = l.sustain.value();
    e.lef.contrast = l.contrast.value();
    e.lef.mix = l.mix.value();

    let d = &p.dynamics;
    e.dynamics.crossovers = [d.xo1.value(), d.xo2.value()];
    e.dynamics.link = d.link.value();
    e.dynamics.mid_side = d.mid_side.value();
    e.dynamics.output_db = d.output.value();
    e.dynamics.mix = d.mix.value();
    for (b, bp) in e.dynamics.bands.iter_mut().zip(d.bands.iter()) {
        b.enabled = bp.on.value();
        b.threshold_db = bp.threshold.value();
        b.ratio = bp.ratio.value();
        b.knee_db = bp.knee.value();
        b.attack_ms = bp.attack.value();
        b.release_ms = bp.release.value();
        b.gain_db = bp.gain.value();
        b.expand_threshold_db = bp.exp_threshold.value();
        b.expand_ratio = bp.exp_ratio.value();
    }

    let v = &p.vcomp;
    e.vcomp.input_db = v.input.value();
    e.vcomp.threshold_db = v.threshold.value();
    e.vcomp.ratio = v.ratio.value();
    e.vcomp.attack_ms = v.attack.value();
    e.vcomp.release_ms = v.release.value();
    e.vcomp.makeup_db = v.makeup.value();
    e.vcomp.warmth = v.warmth.value();
    e.vcomp.mix = v.mix.value();

    let v = &p.veq;
    e.veq.low_freq = v.low_freq.value() as usize;
    e.veq.low_boost = v.low_boost.value();
    e.veq.low_atten = v.low_atten.value();
    e.veq.high_freq = v.high_freq.value() as usize;
    e.veq.high_boost = v.high_boost.value();
    e.veq.bandwidth = v.bandwidth.value();
    e.veq.atten_freq = v.atten_freq.value() as usize;
    e.veq.high_atten = v.high_atten.value();
    e.veq.tube = v.tube.value();

    let v = &p.vtape;
    e.vtape.drive_db = v.drive.value();
    e.vtape.speed = match v.speed.value() {
        TapeSpeedP::Ips7 => TapeSpeed::Ips7_5,
        TapeSpeedP::Ips15 => TapeSpeed::Ips15,
        TapeSpeedP::Ips30 => TapeSpeed::Ips30,
    };
    e.vtape.saturation = v.saturation.value();
    e.vtape.wow_flutter = v.wow.value();
    e.vtape.noise = v.noise.value();
    e.vtape.age = v.age.value();
    e.vtape.output_db = v.output.value();

    let x = &p.exciter;
    e.exciter.crossovers = [x.xo1.value(), x.xo2.value(), x.xo3.value()];
    e.exciter.kind = match x.mode.value() {
        SatModeP::Tape => ShaperKind::Tape,
        SatModeP::Tube => ShaperKind::Tube,
        SatModeP::Warm => ShaperKind::Warm,
        SatModeP::Retro => ShaperKind::Retro,
    };
    e.exciter.mix = x.mix.value();
    for (b, bp) in e.exciter.bands.iter_mut().zip(x.bands.iter()) {
        b.enabled = bp.on.value();
        b.drive_db = bp.drive.value();
        b.amount = bp.amount.value();
        b.gain_db = bp.gain.value();
    }

    let i = &p.impact;
    e.impact.crossovers = [i.xo1.value(), i.xo2.value()];
    e.impact.mix = i.mix.value();
    for (b, bp) in e.impact.bands.iter_mut().zip(i.bands.iter()) {
        b.attack_db = bp.attack.value();
        b.sustain_db = bp.sustain.value();
    }

    let g = &p.imager;
    e.imager.crossovers = [g.xo1.value(), g.xo2.value(), g.xo3.value()];
    e.imager.stereoize = g.stereoize.value();
    e.imager.balance = g.balance.value();
    for (w, bp) in e.imager.widths.iter_mut().zip(g.bands.iter()) {
        *w = bp.width.value();
    }

    let d = &p.delay;
    e.delay.time_l_ms = d.time_l.value();
    e.delay.time_r_ms = d.time_r.value();
    e.delay.feedback = d.feedback.value();
    e.delay.ping_pong = d.ping_pong.value();
    e.delay.lowcut_hz = d.lowcut.value();
    e.delay.highcut_hz = d.highcut.value();
    e.delay.mix = d.mix.value();

    let r = &p.reverb;
    e.reverb.room_size = r.room.value();
    e.reverb.damping = r.damping.value();
    e.reverb.width = r.width.value();
    e.reverb.predelay_ms = r.predelay.value();
    e.reverb.lowcut_hz = r.lowcut.value();
    e.reverb.mix = r.mix.value();

    let v = &p.vlimiter;
    e.vlimiter.input_db = v.input.value();
    e.vlimiter.output_db = v.output.value();
    e.vlimiter.attack_ms = v.attack.value();
    e.vlimiter.release_ms = v.release.value();
    e.vlimiter.character = v.character.value();

    let m = &p.max;
    e.maximizer.mode = limiter_mode(m.mode.value());
    e.maximizer.gain_db = m.gain.value();
    e.maximizer.ceiling_db = m.ceiling.value();
    e.maximizer.release_ms = m.release.value();
    e.maximizer.character = m.character.value();
    e.maximizer.independence = m.independence.value();
    e.maximizer.true_peak = m.true_peak.value();

    let _ = ModuleId::ALL;
}

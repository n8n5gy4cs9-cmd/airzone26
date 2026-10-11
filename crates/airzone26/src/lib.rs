//! AirZone 26 - full mastering suite (VST3 / CLAP).

use std::num::NonZeroU32;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use airzone_dsp::assistant::Analyzer;
use airzone_dsp::chain::Engine;
use airzone_dsp::spectral::{Capture, SpectrumProfile, MATCH_BANDS};
use nih_plug::prelude::*;

pub mod analysis;
pub mod gui;
pub mod module_presets;
pub mod params;
pub mod presets;
pub mod shared;
pub mod sync;

use params::{AirZoneParams, MatchData};
use shared::*;

pub struct AirZone {
    params: Arc<AirZoneParams>,
    values: Option<sync::Values>,
    shared: Arc<Shared>,
    engine: Option<Engine>,
    analyzer: Option<Analyzer>,
    last_latency: u32,
    meter_tick: usize,
    match_save_tick: usize,
    restored: bool,
}

impl Default for AirZone {
    fn default() -> Self {
        Self {
            params: Arc::new(AirZoneParams::default()),
            values: None,
            shared: Arc::new(Shared::new()),
            engine: None,
            analyzer: None,
            last_latency: 0,
            meter_tick: 0,
            match_save_tick: 0,
            restored: false,
        }
    }
}

fn profile_from(power: &[f32], frames: u32) -> SpectrumProfile {
    let mut p = SpectrumProfile::default();
    if power.len() == MATCH_BANDS {
        p.power.copy_from_slice(power);
        p.frames = frames;
    }
    p
}

impl AirZone {
    fn restore_match(&mut self) {
        let Some(e) = self.engine.as_mut() else { return };
        let d = self.params.match_data.lock().clone();
        e.hub.match_eq.source = profile_from(&d.source_power, d.source_frames);
        e.hub.match_eq.reference = profile_from(&d.ref_power, d.ref_frames);
        e.hub.match_eq.mark_dirty();
        self.shared.match_progress[0].store(d.source_frames, Ordering::Relaxed);
        self.shared.match_progress[1].store(d.ref_frames, Ordering::Relaxed);
    }

    fn save_match(&self) {
        let Some(e) = self.engine.as_ref() else { return };
        if let Some(mut d) = self.params.match_data.try_lock() {
            let name = d.ref_name.clone();
            *d = MatchData {
                source_power: e.hub.match_eq.source.power.to_vec(),
                source_frames: e.hub.match_eq.source.frames,
                ref_power: e.hub.match_eq.reference.power.to_vec(),
                ref_frames: e.hub.match_eq.reference.frames,
                ref_name: name,
            };
        }
    }

    fn handle_commands(&mut self) {
        let sh = self.shared.clone();
        let Some(e) = self.engine.as_mut() else { return };
        e.hub.match_eq.capture = match sh.capture.load(Ordering::Relaxed) {
            CAP_SOURCE => Capture::Source,
            CAP_REFERENCE => Capture::Reference,
            _ => Capture::Off,
        };
        if sh.clear_source.swap(false, Ordering::Relaxed) {
            e.hub.match_eq.clear(Capture::Source);
        }
        if sh.clear_reference.swap(false, Ordering::Relaxed) {
            e.hub.match_eq.clear(Capture::Reference);
        }
        if sh.clear_key_hist.swap(false, Ordering::Relaxed) {
            e.pitch.key_hist.clear();
            e.vocal_focus.pitch.key_hist.clear();
        }
        if sh.reset_meters.swap(false, Ordering::Relaxed) {
            e.in_meter.reset();
            e.out_meter.reset();
        }
        // reference A/B player
        e.reference.monitor = sh.ref_monitor.load(Ordering::Relaxed);
        e.reference.playing = sh.ref_playing.load(Ordering::Relaxed);
        e.reference.looping = sh.ref_loop.load(Ordering::Relaxed);
        e.reference.level_match = sh.ref_level_match.load(Ordering::Relaxed);
        e.reference.trim_db = sh.ref_trim_db.load(Ordering::Relaxed);
        let seek = sh.ref_seek.load(Ordering::Relaxed);
        if seek >= 0.0 {
            e.reference.seek(seek);
            sh.ref_seek.store(-1.0, Ordering::Relaxed);
        }
        // (the GUI keeps its own Arc of every loaded track, so dropping the old one here never frees)
        let new_track = sh.pending_ref_track.try_lock().and_then(|mut p| p.take());
        if let Some(t) = new_track {
            e.reference.set_track(Some(t));
        }

        let taken = sh.pending_reference.try_lock().and_then(|mut p| p.take());
        if let Some((profile, name)) = taken {
            e.hub.match_eq.set_reference(profile);
            if let Some(mut d) = self.params.match_data.try_lock() {
                d.ref_name = name;
            }
        }
    }

    fn publish_meters(&mut self) {
        let sh = &self.shared;
        let Some(e) = self.engine.as_mut() else { return };
        let st = |a: &AtomicF32, v: f32| a.store(v, Ordering::Relaxed);
        st(&sh.in_momentary, e.in_meter.momentary);
        st(&sh.in_integrated, e.in_meter.integrated);
        st(&sh.out_momentary, e.out_meter.momentary);
        st(&sh.out_short, e.out_meter.short_term);
        st(&sh.out_integrated, e.out_meter.integrated);
        st(&sh.out_lra, e.out_meter.lra);
        st(&sh.true_peak, e.out_meter.true_peak_db);
        st(&sh.correlation, e.stereo_meter.correlation);
        st(&sh.width, e.imager.meter_width);
        st(&sh.gr[GR_MAXIMIZER], e.maximizer.meter_gr);
        for b in 0..3 {
            st(&sh.gr[GR_DYN_BASE + b], e.dynamics.meter_gr[b]);
        }
        st(&sh.gr[GR_VCOMP], e.vcomp.meter_gr);
        st(&sh.gr[GR_VLIM], e.vlimiter.meter_gr);
        st(&sh.gr[GR_UNLIM], e.unlimiter.meter_boost_db);
        st(&sh.ref_pos, e.reference.position());
        st(&sh.ref_len_s, e.reference.length_seconds());
        st(&sh.ref_lufs, e.reference.track_lufs());
        st(&sh.ref_gain_db, e.reference.applied_gain_db());
        sh.ref_playing.store(e.reference.playing, Ordering::Relaxed);
        for i in 0..airzone_dsp::meter::SPEC_POINTS {
            st(&sh.spectrum_in[i], e.spectrum_in.out[i]);
            st(&sh.spectrum_out[i], e.spectrum_out.out[i]);
        }
        for i in 0..4 {
            st(&sh.stem_energy[i], e.hub.stem.meter[i]);
        }
        for i in 0..32 {
            st(&sh.stab_curve[i], e.hub.stabilizer.meter_gain_db[i]);
        }
        for i in 0..3 {
            st(&sh.bass_levels[i], e.bass.meter_db[i]);
        }
        // the vocal-in-mix mode runs its own corrector instance
        let pc = if e.pitch_mix_vocal { &e.vocal_focus.pitch } else { &e.pitch };
        sh.pitch_voiced.store(pc.meter.voiced, Ordering::Relaxed);
        st(&sh.pitch_midi, pc.meter.midi);
        st(&sh.pitch_target, pc.meter.target_note as f32);
        st(&sh.pitch_shift, pc.meter.shift_cents);
        st(&sh.pitch_f0, pc.meter.f0);
        st(&sh.pitch_voiced_ratio, pc.meter.voiced_ratio);
        for i in 0..12 {
            st(&sh.key_hist[i], pc.key_hist.hist[i]);
        }
        for i in 0..airzone_dsp::chain::SCOPE_N {
            st(&sh.scope_l[i], e.scope_l[i]);
            st(&sh.scope_r[i], e.scope_r[i]);
        }
        st(&sh.tune_detected, e.hub.tuning.detected_cents);
        st(&sh.tune_applied, e.hub.tuning.applied_cents);
        st(&sh.tune_conf, e.hub.tuning.confidence);
        sh.match_progress[0].store(e.hub.match_eq.source.frames, Ordering::Relaxed);
        sh.match_progress[1].store(e.hub.match_eq.reference.frames, Ordering::Relaxed);
        if let Some(mut c) = sh.match_curve.try_lock() {
            *c = e.hub.match_eq.band_curve();
        }
        sh.latency.store(e.latency() as u32, Ordering::Relaxed);
    }
}

impl Plugin for AirZone {
    const NAME: &'static str = "AirZone 26";
    const VENDOR: &'static str = "crowelian";
    const URL: &'static str = "https://github.com/n8n5gy4cs9-cmd/airzone26";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: NonZeroU32::new(2),
        main_output_channels: NonZeroU32::new(2),
        ..AudioIOLayout::const_default()
    }];

    const MIDI_INPUT: MidiConfig = MidiConfig::None;
    const SAMPLE_ACCURATE_AUTOMATION: bool = false;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        gui::create(self.params.clone(), self.shared.clone())
    }

    fn initialize(
        &mut self,
        _layout: &AudioIOLayout,
        cfg: &BufferConfig,
        context: &mut impl InitContext<Self>,
    ) -> bool {
        let sr = cfg.sample_rate;
        self.shared.sample_rate.store(sr, Ordering::Relaxed);
        self.engine = Some(Engine::new(sr, cfg.max_buffer_size as usize));
        self.analyzer = Some(Analyzer::new(sr));
        self.restore_match();
        let mut vals = sync::Values::new(self.params.clone());
        vals.refresh();
        if let Some(e) = self.engine.as_mut() {
            let order = self.params.order.read().clone();
            sync::apply(&vals, Some(&order), e);
            let lat = e.refresh_latency() as u32;
            self.last_latency = lat;
            context.set_latency_samples(lat);
        }
        self.values = Some(vals);
        true
    }

    fn reset(&mut self) {
        if let Some(e) = self.engine.as_mut() {
            let (src, rf) = (e.hub.match_eq.source.clone(), e.hub.match_eq.reference.clone());
            e.reset();
            e.hub.match_eq.source = src;
            e.hub.match_eq.reference = rf;
            e.hub.match_eq.mark_dirty();
        }
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        if !self.restored {
            self.restored = true;
            self.restore_match();
        }
        self.handle_commands();
        let Some(engine) = self.engine.as_mut() else { return ProcessStatus::Normal };
        if let Some(vals) = self.values.as_mut() {
            vals.refresh();
            let guard = self.params.order.try_read();
            sync::apply(vals, guard.as_deref().map(|v| v.as_slice()), engine);
        }
        let lat = engine.refresh_latency() as u32;
        if lat != self.last_latency {
            self.last_latency = lat;
            context.set_latency_samples(lat);
        }

        let ch = buffer.as_slice();
        if ch.len() >= 2 {
            let (a, b) = ch.split_at_mut(1);
            let (l, r) = (&mut *a[0], &mut *b[0]);

            // Master Assistant listens to the *input* of the chain.
            let cmd = self.shared.assist_cmd.load(Ordering::Relaxed);
            if let Some(an) = self.analyzer.as_mut() {
                match cmd {
                    ASSIST_LISTEN => {
                        an.process(l, r);
                        self.shared.assist_seconds.store(an.seconds(), Ordering::Relaxed);
                    }
                    ASSIST_FINISH => {
                        if let Some(res) = an.result() {
                            if let Some(mut slot) = self.shared.assist_result.try_lock() {
                                *slot = Some(res);
                            }
                        }
                        an.reset();
                        self.shared.assist_cmd.store(ASSIST_IDLE, Ordering::Relaxed);
                    }
                    _ => {}
                }
            }
            engine.process(l, r);
        }

        self.meter_tick += buffer.samples();
        if self.meter_tick >= 1024 {
            self.meter_tick = 0;
            self.publish_meters();
        }
        self.match_save_tick += buffer.samples();
        let sr = self.shared.sample_rate.load(Ordering::Relaxed);
        if self.match_save_tick as f32 > sr * 2.0 {
            self.match_save_tick = 0;
            self.save_match();
        }
        ProcessStatus::Normal
    }
}

impl ClapPlugin for AirZone {
    const CLAP_ID: &'static str = "com.airzone.airzone26";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("Full mastering suite with 20 modules, assistant and presets");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] =
        &[ClapFeature::AudioEffect, ClapFeature::Mastering, ClapFeature::Stereo, ClapFeature::Limiter];
}

impl Vst3Plugin for AirZone {
    const VST3_CLASS_ID: [u8; 16] = *b"AirZone26MstrSte";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::Dynamics];
}

nih_export_clap!(AirZone);
nih_export_vst3!(AirZone);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parameter_ids_are_unique() {
        let p = AirZoneParams::default();
        let mut ids: Vec<String> = p.param_map().into_iter().map(|(id, _, _)| id).collect();
        let n = ids.len();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), n, "duplicate parameter ids");
    }
}

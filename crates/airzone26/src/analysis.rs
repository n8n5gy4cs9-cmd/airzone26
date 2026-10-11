//! Offline analysis helpers (reference-track loading).

use std::path::Path;

use airzone_dsp::assistant::Analyzer;
use airzone_dsp::spectral::SpectrumProfile;

/// Decode a WAV file and build its averaged log-band spectrum profile.
pub fn analyze_wav(path: &Path) -> Result<(SpectrumProfile, String), String> {
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
    if samples.len() < spec.sample_rate as usize / 2 * ch {
        return Err("file is too short (need at least 0.5 s)".into());
    }
    let frames = samples.len() / ch;
    let mut l = Vec::with_capacity(frames);
    let mut r = Vec::with_capacity(frames);
    for f in 0..frames {
        let a = samples[f * ch];
        let b = if ch > 1 { samples[f * ch + 1] } else { a };
        l.push(a);
        r.push(b);
    }
    let mut an = Analyzer::new(spec.sample_rate as f32);
    for (a, b) in l.chunks(4096).zip(r.chunks(4096)) {
        an.process(a, b);
    }
    let profile = an.profile().clone();
    if !profile.is_valid() {
        return Err("not enough audio to analyse".into());
    }
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    Ok((profile, name))
}

/// Maximum reference length kept in memory (6 minutes of stereo f32 is about 140 MB at 48 kHz).
const MAX_REFERENCE_SECONDS: f32 = 360.0;

/// Decode a WAV file for the A/B reference player: stereo at the host sample rate, with its
/// integrated loudness measured so it can be level-matched to the master.
pub fn load_reference(path: &Path, host_sr: f32) -> Result<airzone_dsp::reference::ReferenceTrack, String> {
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
    let src_sr = spec.sample_rate as f32;
    let max_src = (MAX_REFERENCE_SECONDS * src_sr) as usize;
    let frames = frames.min(max_src);
    let ratio = host_sr / src_sr;
    let out_len = ((frames as f32) * ratio) as usize;
    let mut l = Vec::with_capacity(out_len);
    let mut r = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let pos = i as f32 / ratio;
        let i0 = (pos.floor() as usize).min(frames - 1);
        let i1 = (i0 + 1).min(frames - 1);
        let f = pos - i0 as f32;
        let get = |fr: usize, c: usize| samples[fr * ch + c.min(ch - 1)];
        l.push(get(i0, 0) * (1.0 - f) + get(i1, 0) * f);
        r.push(get(i0, 1) * (1.0 - f) + get(i1, 1) * f);
    }
    let mut meter = airzone_dsp::meter::LoudnessMeter::new(host_sr);
    for (a, b) in l.chunks(4096).zip(r.chunks(4096)) {
        meter.process(a, b);
    }
    let lufs = if meter.integrated > -69.0 { meter.integrated } else { meter.momentary };
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    Ok(airzone_dsp::reference::ReferenceTrack { l, r, lufs, name })
}

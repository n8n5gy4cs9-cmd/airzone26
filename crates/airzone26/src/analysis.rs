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

# AirZone 26 — Task Tracker

Legend: ✅ done · 🚧 in progress · ⬜ not started
_Updated after every task; each task is committed to `main`._

## Foundation
- ✅ T01 Cargo workspace, NIH-plug (VST3 + CLAP), xtask bundler, `.cargo` alias
- ✅ T02 DSP core: utils, biquads, Butterworth, LR4 crossovers (2–4 band)
- ✅ T03 Equalizer (8 band, M/S/L/R placement) + Dynamic EQ (4 band)
- ✅ T04 Dynamics (3-band compressor/expander, M/S, link)
- ✅ T05 Exciter (4-band, ADAA tape/tube/warm/retro)
- ✅ T06 Impact (3-band transient) + Low End Focus
- ✅ T07 Imager (4-band width, stereoize) + Bass Control
- ✅ T08 Maximizer (5 limiter algorithms, true-peak, lookahead) + Dither
- ✅ T09 STFT engine (2048, 75 % overlap, tested as identity)

## Remaining DSP
- ✅ T10 Spectral modules: Stem EQ, Match EQ, Stabilizer, Spectral Shaper (single FFT hub)
- ✅ T11 Unlimiter (upward peak/transient restoration)
- ✅ T12 Vintage Compressor, Vintage EQ (Pultec-style), Vintage Limiter, Vintage Tape
- ✅ T13 Delay + Reverb
- ✅ T14 Meters: LUFS (M/S/I/LRA), true peak, correlation, spectrum analyzer
- ✅ T15 Master Assistant (analysis → suggested settings, genre targets)
- ✅ T16 Mastering chain engine (module order, latency, bypass)

## Plugin
- ✅ T17 nih-plug parameters for every module + mapping to engine
- ✅ T18 Presets: 60+ factory presets (embedded JSON) + user preset save/load
- ✅ T19 Preset validation tests (params exist, render finite, ceiling respected)
- ✅ T20 egui GUI: module rail, editors, spectrum, meters, preset browser
- ✅ T21 Reference-track loading (WAV) for Match EQ / comparison

## Build & docs
- 🚧 T22 GitHub Actions workflow written (Windows x64 + macOS arm64 VST3/CLAP) GitHub Actions: Windows x64 (10/11) + macOS arm64 (M1 Max) VST3/CLAP, artifacts
- ✅ T23 Full HTML manual (`docs/manual.html`)
- ✅ T24 README, licensing/trademark notes
- 🚧 T25 Verify CI green on all platforms
  - ✅ Linux: `cargo test --workspace` green, `cargo xtask bundle` produces VST3 + CLAP
  - ✅ `cargo check` passes for `x86_64-pc-windows-msvc` and `aarch64-apple-darwin` (type-check only; linking needs the real OS)
  - ⚠️ GitHub Actions jobs fail within ~3 s with no steps executed (runner never starts) – looks like an account/billing/Actions-settings issue, not a code issue. Needs your attention in the repo's Settings → Actions / billing, then re-run the workflow.

## Optional (do last)
- 🚧 T26 Pitch Correction section: natural-sounding auto-tune for vocals, single instruments or full songs
  - ⬜ T26a YIN pitch tracker (accurate to a few cents) + tests
  - ⬜ T26b Key/scale model, auto key detection, note targeting with hysteresis, humanize (vibrato-preserving), tolerance, retune speed
  - ⬜ T26c PSOLA formant-preserving shifter for Vocal / Instrument modes + tests
  - ⬜ T26d Full-song mode: global tuning-offset detection + phase-vocoder shifter inside the shared FFT hub + tests
  - ⬜ T26e Plugin integration: 21st module, parameters, latency reporting
  - ⬜ T26f GUI panel: detected note / cents / target display, key + scale, auto-key button
  - ⬜ T26g Pitch presets, manual section, final tests

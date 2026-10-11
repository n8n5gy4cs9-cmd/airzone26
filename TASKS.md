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
- 🚧 T22 GitHub Actions: Windows x64 (10/11) + macOS arm64 (M1 Max) VST3/CLAP artifacts – workflow written, blocked by runner start (see T25)
- ✅ T23 Full HTML manual (`docs/manual.html`)
- ✅ T24 README, licensing/trademark notes
- 🚧 T25 Verify CI green on all platforms
  - ✅ Linux: `cargo test --workspace` green, `cargo xtask bundle` produces VST3 + CLAP
  - ✅ `cargo check` passes for `x86_64-pc-windows-msvc` and `aarch64-apple-darwin` (type-check only; linking needs the real OS)
  - ⚠️ GitHub Actions jobs fail within ~3 s with no steps executed (runner never starts) – looks like an account/billing/Actions-settings issue, not a code issue. Needs your attention in the repo's Settings → Actions / billing, then re-run the workflow.

- ✅ T27 `build.sh` – builds Windows (native or cross via cargo-xwin) and macOS arm64 (+ optional universal) bundles into dist/

## Betterment run (round 2)
> ⚠️ Everything in this section was written **without compiling or running tests** (requested). The
> first thing to do is `cargo test --workspace`; compile errors, if any, are small and local.

- 🧪 T28 Responsive UI: resizable window (drag the corner), adaptive widths/columns, collapsing chain rail on narrow windows, scroll areas everywhere, layout smoke-test at 6 window sizes × 21 modules
- 🧪 T29 Per-module presets: every module has a preset menu with prev/next, Reset and Save (user module presets in `Presets/Modules/`)
- 🧪 T30 Module preset library: 130 presets across all 21 modules + tests (ids exist, belong to the module, in range, each renders finite)
- 🧪 T31 Icons: all emoji / special glyphs removed from the UI; arrows are painted shapes, so nothing depends on the font
- 🧪 T32 Master Assistant fixes (see below) + new tests
- 🧪 T33 Pitch correction: diagnostics banner (module off / no pitch found), friendlier defaults (YIN threshold 0.2, voicing confidence 0.6), new mode **Vocal in Full Mix** (stem-estimated vocal tuned on its own and recombined)
- 🧪 T34 Ozone-parity additions: A/B compare slots, tonal-target overlay on the spectrum, vectorscope, loudness-matched bypass
- 🧪 T35 Manual template updated (module presets, A/B, vectorscope, assistant logic, resizing); run `cargo run -p airzone26 --example gen_docs` to regenerate docs/manual.html
- 🧪 T37 Window handling: in-plugin resize removed (it made REAPER flicker and resize itself); old saved sizes ignored/sanitised; window size chosen from the "Window" menu, applied on reopen
- 🧪 T38 Reference A/B player (WAV, level-matched, loop, seek, trim) + Reverb/Delay "Solo Wet"
- 🧪 T39 Version 0.2.0, About window "by crowelian 2026"; 41 extra "Classic" full-chain presets (original, in the classic genre/source/style/loudness layout)
- 🧪 T36a Clarity module (detail lift + harmonics + warmth) — 22nd module
- 🧪 T36b Codec Preview module (MP3/AAC/Opus-style lossy simulation, delta listen) — 23rd module, in the shared FFT hub
- 🧪 T36c AirZone 26 Tonal Balance: separate companion plug-in (5 region gauges, curve, genre/custom/WAV targets); new crate `crates/airzone-tonal`, bundled by build.sh and CI
- 🧪 T36d Maximizer AZ-6 (clip + limit) and AZ-7 (smooth) algorithms
- ❌ T36e ML stem separation: not shipped (no trained model can be bundled/tested here; Stem EQ stays DSP-based)
- ❌ T36f Proprietary "IRC" limiter algorithms: cannot be reproduced; AirZone has seven original algorithms instead
- ⬜ T36 (old note) Still missing vs. Ozone: Clarity module, Codec Preview, separate Tonal Balance Control plug-in, reference-track A/B player, ML stem separation, Maximizer "IRC" algorithms (ours are original AZ-1..5)

Legend 🧪 = written, not compiled/tested yet.

### What was wrong with the Master Assistant (fixed in T32)
1. **Empty low bands**: spectrum bands below ~100 Hz often contained no FFT bin and were read as −140 dB, so the assistant "saw" a huge lack of bass and suggested low-end boosts. They are now interpolated, and tonal advice only uses 90 Hz – 12 kHz.
2. **Level offset**: the offset between measurement and target used a mean over all bands including broken ones; now a median over the reliable range.
3. **Many fighting bells**: replaced by one broad tilt shelf for overall brightness plus *cuts only* for prominent narrow excesses (max 4 bands, ±3.5 dB).
4. **Crest factor from raw peak/RMS** was skewed by quiet intros; decisions now use peak-to-loudness ratio (PLR).
5. **Sub vs. bass** compared sums over different numbers of FFT bins (so "sub is weak" fired almost always); now energy per octave over 30–80 Hz vs 80–250 Hz.
6. **Correlation** used the instantaneous meter value at the end of the capture; now long-term over the whole capture.
7. **Compressor threshold** was a fixed −20 dB regardless of level; now derived from the measured loudness.
8. **Too little / too quiet audio** now produces "play at least 5 s of the loudest section" instead of nonsense.

## Optional (do last)
- ✅ T26 Pitch Correction section: natural-sounding auto-tune for vocals, single instruments or full songs
  - ✅ T26a YIN pitch tracker (accurate to a few cents) + tests
  - ✅ T26b Key/scale model, auto key detection, note targeting with hysteresis, humanize (vibrato-preserving), tolerance, retune speed
  - ✅ T26c PSOLA formant-preserving shifter for Vocal / Instrument modes + tests
  - ✅ T26d Full-song mode: global tuning-offset detection + phase-vocoder shifter inside the shared FFT hub + tests
  - ✅ T26e Plugin integration: 21st module, parameters, latency reporting
  - ✅ T26f GUI panel: detected note / cents / target display, key + scale, auto-key button
  - ✅ T26g Pitch presets, manual section, final tests

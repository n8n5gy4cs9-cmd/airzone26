# AirZone 26

Full mastering suite for **REAPER** (and any VST3 / CLAP host) — written in **Rust** with
[NIH-plug](https://github.com/robbert-vdh/nih-plug) and egui.

* Windows 10 / 11 (x64) and macOS Apple Silicon (arm64, M1 Max etc.)
* 20 modules · 68 factory presets · Master Assistant · LUFS / true-peak metering
* **Full manual:** [`docs/manual.html`](docs/manual.html) (open in a browser)
* **Progress tracker:** [`TASKS.md`](TASKS.md)

## Modules
Unlimiter · Stem EQ · Match EQ · Equalizer · Dynamic EQ · Stabilizer · Spectral Shaper · Bass Control ·
Low End Focus · Dynamics (3-band) · Vintage Comp · Vintage EQ · Vintage Tape · Exciter · Impact · Imager ·
Delay · Reverb · Vintage Limiter · Maximizer (5 algorithms, true-peak)

## Build
```
cargo test --workspace
cargo xtask bundle airzone26 --release     # -> target/bundled/AirZone 26.vst3 / .clap
```
CI (`.github/workflows/build.yml`) builds Windows x64 and macOS arm64 bundles and uploads them as artifacts.

## Layout
| Path | Purpose |
|------|---------|
| `crates/airzone-dsp` | plug-in-independent DSP engine + tests |
| `crates/airzone26` | NIH-plug wrapper: params, presets, GUI |
| `docs/` | manual template + generated `manual.html` (`cargo run -p airzone26 --example gen_docs`) |

## Notes
Inspired by the workflow of all-in-one mastering suites; all algorithms are original. Stem EQ is a DSP
approximation (not ML). See the manual's *Legal & credits* section. License: GPL-3.0-or-later.

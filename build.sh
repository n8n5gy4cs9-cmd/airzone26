#!/usr/bin/env bash
# AirZone 26 – build VST3 + CLAP bundles for Windows and macOS.
#
#   ./build.sh                 build everything this machine can build
#   ./build.sh --windows       Windows x64 (10/11)
#   ./build.sh --macos         macOS Apple Silicon (arm64, M1/M2/M3 ...)
#   ./build.sh --macos-intel   also build macOS x86_64 and a universal bundle (macOS host only)
#   ./build.sh --test          run the test-suite first
#   ./build.sh --clean         remove dist/ and target/bundled before building
#
# Host support:
#   macOS    -> macOS arm64 natively (xtask) and Windows via cargo-xwin
#   Windows  -> Windows natively (run from Git Bash / MSYS2 / WSL-less shell)
#   Linux    -> Windows via cargo-xwin; macOS CANNOT be built here (needs Apple's SDK) –
#               use a Mac, or the GitHub Actions workflow in .github/workflows/build.yml.
#
# Output: dist/AirZone26-Windows-x64/ and dist/AirZone26-macOS-arm64/ (+ .zip files).
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")"
ROOT="$PWD"
NAME="AirZone 26"
TONAL="AirZone 26 Tonal Balance"
TONAL_PKG="airzone-tonal"
PKG="airzone26"
WIN_TARGET="x86_64-pc-windows-msvc"
MAC_ARM="aarch64-apple-darwin"
MAC_X86="x86_64-apple-darwin"

want_win=0 want_mac=0 want_intel=0 run_tests=0 clean=0 explicit=0
for a in "$@"; do
  case "$a" in
    --windows)     want_win=1; explicit=1 ;;
    --macos)       want_mac=1; explicit=1 ;;
    --macos-intel) want_mac=1; want_intel=1; explicit=1 ;;
    --all)         want_win=1; want_mac=1; explicit=1 ;;
    --test)        run_tests=1 ;;
    --clean)       clean=1 ;;
    -h|--help)     sed -n '2,20p' "$0"; exit 0 ;;
    *) echo "unknown option: $a (try --help)" >&2; exit 2 ;;
  esac
done

host="$(uname -s)"
case "$host" in
  Darwin) HOST=mac ;;
  Linux)  HOST=linux ;;
  MINGW*|MSYS*|CYGWIN*) HOST=windows ;;
  *) echo "unsupported host: $host" >&2; exit 1 ;;
esac

if [ "$explicit" -eq 0 ]; then
  want_win=1
  [ "$HOST" = mac ] && want_mac=1
fi

say()  { printf '\n\033[1;36m==> %s\033[0m\n' "$*"; }
warn() { printf '\033[1;33mwarning:\033[0m %s\n' "$*" >&2; }
die()  { printf '\033[1;31merror:\033[0m %s\n' "$*" >&2; exit 1; }

command -v cargo  >/dev/null || die "cargo not found – install Rust from https://rustup.rs"
command -v rustup >/dev/null || die "rustup not found – install Rust from https://rustup.rs"

say "Refreshing docs/manual.html"
cargo run -q -p airzone26 --example gen_docs || warn "manual regeneration failed (continuing)"

if [ "$want_mac" -eq 1 ] && [ "$HOST" != mac ]; then
  die "macOS bundles can only be built on a Mac (Apple SDK + codesign). Run this script on your Mac, or push to GitHub and use the 'Build' workflow."
fi

[ "$clean" -eq 1 ] && { say "cleaning"; rm -rf dist target/bundled; }
[ "$run_tests" -eq 1 ] && { say "running tests"; cargo test --workspace --release; }
mkdir -p dist

zip_dir() { # zip_dir <dir>
  local d="$1"
  if command -v zip >/dev/null; then (cd dist && rm -f "$(basename "$d").zip" && zip -qr "$(basename "$d").zip" "$(basename "$d")")
  elif command -v powershell.exe >/dev/null; then
    powershell.exe -NoProfile -Command "Compress-Archive -Force -Path 'dist/$(basename "$d")/*' -DestinationPath 'dist/$(basename "$d").zip'"
  else warn "no zip tool found, skipping archive for $d"; fi
}

stage_docs() { # stage_docs <dir>
  cp README.md "$1/" 2>/dev/null || true
  mkdir -p "$1/docs" && cp docs/manual.html "$1/docs/" 2>/dev/null || true
}

# --------------------------------------------------------------------------- Windows
build_windows() {
  say "Windows x64 (10/11)"
  rustup target add "$WIN_TARGET" >/dev/null
  local out="dist/AirZone26-Windows-x64"
  rm -rf "$out"; mkdir -p "$out"

  if [ "$HOST" = windows ]; then
    cargo xtask bundle -p "$PKG" -p "$TONAL_PKG" --release --target "$WIN_TARGET"
    cp -R "target/bundled/$NAME.vst3" "$out/"
    cp    "target/bundled/$NAME.clap" "$out/"
    cp -R "target/bundled/$TONAL.vst3" "$out/"
    cp    "target/bundled/$TONAL.clap" "$out/"
  else
    # cross-compile with the MSVC toolchain via cargo-xwin (downloads the Windows SDK on first use)
    command -v cargo-xwin >/dev/null || { say "installing cargo-xwin"; cargo install --locked cargo-xwin; }
    cargo xwin build --release --target "$WIN_TARGET" -p "$PKG" -p "$TONAL_PKG" \
      || die "cross-compile failed (cargo-xwin needs internet access to download the Windows SDK/CRT from Microsoft). Build on Windows instead, or use the GitHub Actions workflow."
    local dll="target/$WIN_TARGET/release/${PKG}.dll"
    [ -f "$dll" ] || die "expected $dll was not produced"
    # a Windows VST3 is a bundle folder holding the renamed DLL; CLAP is the renamed DLL itself
    mkdir -p "$out/$NAME.vst3/Contents/x86_64-win"
    cp "$dll" "$out/$NAME.vst3/Contents/x86_64-win/$NAME.vst3"
    cp "$dll" "$out/$NAME.clap"
    local dll2="target/$WIN_TARGET/release/airzone_tonal.dll"
    [ -f "$dll2" ] || die "expected $dll2 was not produced"
    mkdir -p "$out/$TONAL.vst3/Contents/x86_64-win"
    cp "$dll2" "$out/$TONAL.vst3/Contents/x86_64-win/$TONAL.vst3"
    cp "$dll2" "$out/$TONAL.clap"
  fi
  stage_docs "$out"; zip_dir "$out"
  echo "   -> $out"
}

# --------------------------------------------------------------------------- macOS
codesign_adhoc() {
  for b in "$@"; do codesign --force --deep --sign - "$b" || warn "codesign failed for $b"; done
}

build_macos() {
  say "macOS Apple Silicon (arm64)"
  rustup target add "$MAC_ARM" >/dev/null
  cargo xtask bundle -p "$PKG" -p "$TONAL_PKG" --release --target "$MAC_ARM"
  local out="dist/AirZone26-macOS-arm64"
  rm -rf "$out"; mkdir -p "$out"
  cp -R "target/bundled/$NAME.vst3" "$out/"
  cp -R "target/bundled/$NAME.clap" "$out/"
  cp -R "target/bundled/$TONAL.vst3" "$out/"
  cp -R "target/bundled/$TONAL.clap" "$out/"
  codesign_adhoc "$out/$NAME.vst3" "$out/$NAME.clap" "$out/$TONAL.vst3" "$out/$TONAL.clap"
  stage_docs "$out"; zip_dir "$out"
  echo "   -> $out"

  if [ "$want_intel" -eq 1 ]; then
    say "macOS universal (arm64 + x86_64)"
    rustup target add "$MAC_X86" >/dev/null
    cargo xtask bundle-universal -p "$PKG" -p "$TONAL_PKG" --release
    local uout="dist/AirZone26-macOS-universal"
    rm -rf "$uout"; mkdir -p "$uout"
    cp -R "target/bundled/$NAME.vst3" "$uout/"
    cp -R "target/bundled/$NAME.clap" "$uout/"
    cp -R "target/bundled/$TONAL.vst3" "$uout/"
    cp -R "target/bundled/$TONAL.clap" "$uout/"
    codesign_adhoc "$uout/$NAME.vst3" "$uout/$NAME.clap" "$uout/$TONAL.vst3" "$uout/$TONAL.clap"
    stage_docs "$uout"; zip_dir "$uout"
    echo "   -> $uout"
  fi
}

[ "$want_win" -eq 1 ] && build_windows
[ "$want_mac" -eq 1 ] && build_macos

say "done"
ls -1 dist | sed 's/^/   /'

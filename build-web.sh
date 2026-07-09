#!/usr/bin/env bash
# Build TWO browser bundles: web/pkg-webgpu (Bevy's fast WebGPU backend) and
# web/pkg-webgl (plain WebGL2, works everywhere). boot.js picks between them
# at runtime after a real `navigator.gpu.requestAdapter()` capability probe —
# see Cargo.toml's `webgpu` feature comment for why a compile-time-only
# choice isn't safe (the browser can expose `navigator.gpu` without a working
# adapter behind it, and Bevy has no automatic fallback once WebGPU is
# selected).
#
# Requirements:
#   rustup target add wasm32-unknown-unknown
#   wasm-bindgen-cli matching the wasm-bindgen version in Cargo.lock
set -euo pipefail
cd "$(dirname "$0")"

WANT=$(grep -A1 'name = "wasm-bindgen"' Cargo.lock | grep version | head -1 | cut -d'"' -f2)
HAVE=$(wasm-bindgen --version 2>/dev/null | awk '{print $2}' || true)
if [[ "$HAVE" != "$WANT" ]]; then
  echo "wasm-bindgen-cli $WANT is required (found: ${HAVE:-none})."
  echo "Install it with: cargo install wasm-bindgen-cli --version $WANT"
  exit 1
fi

build_variant() {
  local name="$1" cargo_flags="$2"
  local outdir="web/pkg-${name}"
  echo "=========================================================================="
  echo "  Building the ${name} variant"
  echo "=========================================================================="
  rm -rf "$outdir"
  # shellcheck disable=SC2086
  cargo build --release --target wasm32-unknown-unknown $cargo_flags
  wasm-bindgen --target web --no-typescript --out-dir "$outdir" \
    target/wasm32-unknown-unknown/release/frozen_city.wasm

  echo "wasm size (${name}) before wasm-opt: $(du -h "$outdir/frozen_city_bg.wasm" | cut -f1)"
  if command -v wasm-opt >/dev/null; then
    wasm-opt -Oz --strip-debug --all-features \
      -o "$outdir/frozen_city_bg.wasm.opt" "$outdir/frozen_city_bg.wasm"
    mv "$outdir/frozen_city_bg.wasm.opt" "$outdir/frozen_city_bg.wasm"
    echo "wasm size (${name}) after wasm-opt:  $(du -h "$outdir/frozen_city_bg.wasm" | cut -f1)"
  else
    echo "=========================================================================="
    echo "  WARNING: binaryen (wasm-opt) not found -- shipping an UNOPTIMIZED wasm!"
    echo "  This blob is likely ~60-100 MB, which is a serious problem on mobile."
    echo "  Install binaryen and re-run this script to shrink it considerably:"
    echo "    https://github.com/WebAssembly/binaryen"
    echo "=========================================================================="
  fi

  # Precompressed copies: a web server with gzip_static serves these directly.
  gzip -9 -kf "$outdir/frozen_city_bg.wasm" "$outdir/frozen_city.js"
}

build_variant webgpu "--features webgpu"
build_variant webgl ""

# Shared (not per-variant) files also get a gzip_static-ready copy.
gzip -9 -kf web/boot.js web/index.html

echo
echo "Web build ready: web/pkg-webgpu and web/pkg-webgl."
echo "Serve it with the game server itself:"
echo "  cargo run --release -- --server 4595"
echo "then open http://localhost:4595/  (multiplayer: add ?join)"

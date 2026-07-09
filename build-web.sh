#!/usr/bin/env bash
# Build the browser version into web/pkg. Requirements:
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

cargo build --release --target wasm32-unknown-unknown
wasm-bindgen --target web --no-typescript --out-dir web/pkg \
  target/wasm32-unknown-unknown/release/frozen_city.wasm

echo "wasm size before wasm-opt: $(du -h web/pkg/frozen_city_bg.wasm | cut -f1)"

# Shrink the module when binaryen is available (60+ MB -> much smaller).
if command -v wasm-opt >/dev/null; then
  wasm-opt -Oz --strip-debug --all-features \
    -o web/pkg/frozen_city_bg.wasm.opt web/pkg/frozen_city_bg.wasm
  mv web/pkg/frozen_city_bg.wasm.opt web/pkg/frozen_city_bg.wasm
  echo "wasm size after wasm-opt:  $(du -h web/pkg/frozen_city_bg.wasm | cut -f1)"
else
  echo "=========================================================================="
  echo "  WARNING: binaryen (wasm-opt) not found -- shipping an UNOPTIMIZED wasm!"
  echo "  This blob is likely ~60-70 MB, which is a serious problem on mobile."
  echo "  Install binaryen and re-run this script to shrink it considerably:"
  echo "    https://github.com/WebAssembly/binaryen"
  echo "=========================================================================="
fi

# Precompressed copies: a web server with gzip_static serves these directly.
gzip -9 -kf web/pkg/frozen_city_bg.wasm web/pkg/frozen_city.js web/boot.js web/index.html

echo
echo "Web build ready in web/pkg."
echo "Serve it with the game server itself:"
echo "  cargo run --release -- --server 4595"
echo "then open http://localhost:4595/  (multiplayer: add ?join)"

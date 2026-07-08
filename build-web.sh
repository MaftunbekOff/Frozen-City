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

# Shrink the module when binaryen is available (60+ MB -> much smaller).
if command -v wasm-opt >/dev/null; then
  wasm-opt -Oz --strip-debug --all-features \
    -o web/pkg/frozen_city_bg.wasm.opt web/pkg/frozen_city_bg.wasm
  mv web/pkg/frozen_city_bg.wasm.opt web/pkg/frozen_city_bg.wasm
else
  echo "note: install binaryen (wasm-opt) to shrink the .wasm considerably"
fi

# Precompressed copies: a web server with gzip_static serves these directly.
gzip -9 -kf web/pkg/frozen_city_bg.wasm web/pkg/frozen_city.js

echo
echo "Web build ready in web/pkg."
echo "Serve it with the game server itself:"
echo "  cargo run --release -- --server 4595"
echo "then open http://localhost:4595/  (multiplayer: add ?join)"

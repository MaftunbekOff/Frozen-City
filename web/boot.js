// Picks between two wasm bundles at runtime: `pkg-webgpu` (Bevy's fast
// GPU-driven path) when the browser can actually acquire a WebGPU adapter,
// `pkg-webgl` (plain WebGL2, works everywhere) otherwise.
//
// `navigator.gpu` existing is NOT enough on its own — some browsers expose
// the API surface without a working adapter behind it (no GPU driver, some
// VMs/headless setups), and Bevy has no automatic fallback once it has
// committed to WebGPU. Actually calling requestAdapter() is the only real
// test.
async function pickVariant() {
  if (!navigator.gpu) return 'webgl';
  try {
    const adapter = await navigator.gpu.requestAdapter();
    return adapter ? 'webgpu' : 'webgl';
  } catch {
    return 'webgl';
  }
}

const status = document.getElementById('status');
const variant = await pickVariant();
if (status) status.textContent = `Loading the frozen world… (${variant})`;

const { default: init } = await import(`./pkg-${variant}/frozen_city.js`);
init().catch((e) => {
  // Winit uses an exception for control flow on wasm; that one is expected.
  if (!`${e}`.includes('Using exceptions for control flow')) {
    if (status) status.textContent = 'Failed to start: ' + e;
    throw e;
  }
});

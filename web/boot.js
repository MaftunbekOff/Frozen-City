import init from './pkg/frozen_city.js';

if (!navigator.gpu) {
  const status = document.getElementById('status');
  if (status) {
    status.textContent = 'This game needs WebGPU. Please use Chrome or Edge 121+, ' +
      'Firefox (2024+), or Safari on iOS 18+, and make sure hardware acceleration is enabled.';
  }
} else {
  init().catch((e) => {
    // Winit uses an exception for control flow on wasm; that one is expected.
    if (!`${e}`.includes('Using exceptions for control flow')) {
      const status = document.getElementById('status');
      if (status) status.textContent = 'Failed to start: ' + e;
      throw e;
    }
  });
}

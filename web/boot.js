import init from './pkg/frozen_city.js';

init().catch((e) => {
  // Winit uses an exception for control flow on wasm; that one is expected.
  if (!`${e}`.includes('Using exceptions for control flow')) {
    const status = document.getElementById('status');
    if (status) status.textContent = 'Failed to start: ' + e;
    throw e;
  }
});

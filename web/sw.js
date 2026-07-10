// Frozen City service worker.
//
// Every same-origin GET this worker handles — app shell (HTML/JS/JSON/
// icons) and the large wasm/js game bundles under ./pkg-webgpu/ and
// ./pkg-webgl/ alike — uses a single network-first strategy: always try
// the network first, and only fall back to the versioned cache when the
// network is unavailable (offline).
//
// This is deliberate, not just for the bundles. None of this site's
// filenames are content-hashed (see nginx config comment: "Fayl nomlari
// hash'lanmagan" — filenames aren't hashed), which is exactly why
// production nginx sends Cache-Control: no-cache for everything: a new
// build must never get stuck behind an old cached copy. A cache-first or
// stale-while-revalidate shell would silently reintroduce that staleness
// problem for service-worker clients (a returning player could load an
// old index.html/boot.js for an extra session after a deploy). Precaching
// on install just guarantees an offline fallback exists from the first
// visit; it is never preferred over a live network response.
//
// WebSocket upgrades (/ws, /ws-r2, /ws-r3) never go through the fetch
// event at all, so there's nothing to special-case for them here.

const CACHE_VERSION = 'frozen-city-v1';

const SHELL_ASSETS = [
  './',
  './index.html',
  './boot.js',
  './manifest.json',
  './icons/icon-192.png',
  './icons/icon-512.png',
];

self.addEventListener('install', (event) => {
  event.waitUntil(
    (async () => {
      const cache = await caches.open(CACHE_VERSION);
      // Use individual adds so one failing asset doesn't abort the whole
      // precache (addAll is all-or-nothing).
      await Promise.all(
        SHELL_ASSETS.map((url) =>
          cache.add(url).catch((err) => {
            console.warn('[sw] failed to precache', url, err);
          })
        )
      );
      await self.skipWaiting();
    })()
  );
});

self.addEventListener('activate', (event) => {
  event.waitUntil(
    (async () => {
      const keys = await caches.keys();
      await Promise.all(
        keys
          .filter((key) => key.startsWith('frozen-city-') && key !== CACHE_VERSION)
          .map((key) => caches.delete(key))
      );
      await self.clients.claim();
    })()
  );
});

self.addEventListener('fetch', (event) => {
  const { request } = event;

  // Only handle plain same-origin GETs. This also naturally excludes
  // WebSocket upgrade requests, which never surface as fetch events.
  if (request.method !== 'GET') return;

  const url = new URL(request.url);
  if (url.origin !== self.location.origin) return;

  event.respondWith(networkFirst(request));
});

async function networkFirst(request) {
  const cache = await caches.open(CACHE_VERSION);
  try {
    const response = await fetch(request);
    if (response && response.ok) {
      cache.put(request, response.clone());
    }
    return response;
  } catch (err) {
    // Offline (or the request otherwise failed) — fall back to whatever
    // we last cached, if anything.
    const cached = await cache.match(request);
    if (cached) return cached;
    throw err;
  }
}

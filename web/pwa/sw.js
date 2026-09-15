// Offline shell for the PWA build. build.sh fills __VERSION__ and __PRECACHE__.
//
// OpenDota responses are never cached here: backend.js already caches them
// with per-endpoint TTLs, and a second layer would serve stats past those TTLs.
const CACHE = 'dota-stats-__VERSION__';
const SHELL = [__PRECACHE__];

self.addEventListener('install', (e) => {
  e.waitUntil(caches.open(CACHE).then((c) => c.addAll(SHELL)).then(() => self.skipWaiting()));
});

// Drop shells from earlier deploys so storage doesn't grow per release.
self.addEventListener('activate', (e) => {
  e.waitUntil(
    caches.keys()
      .then((keys) => Promise.all(keys.filter((k) => k !== CACHE).map((k) => caches.delete(k))))
      .then(() => self.clients.claim()));
});

self.addEventListener('fetch', (e) => {
  const url = new URL(e.request.url);
  if (e.request.method !== 'GET' || url.origin !== self.location.origin) return;
  // Pages go network-first so a new deploy shows up online; the cached shell
  // is only the offline fallback.
  if (e.request.mode === 'navigate') {
    e.respondWith(fetch(e.request).catch(() => caches.match('index.html')));
    return;
  }
  e.respondWith(caches.match(e.request).then((hit) => hit || fetch(e.request)));
});

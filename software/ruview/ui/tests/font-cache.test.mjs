import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import test from 'node:test';
import vm from 'node:vm';

test('both local UI fonts remain available from the service worker when offline', async () => {
  const root = new URL('../', import.meta.url);
  const source = await fs.readFile(new URL('sw.js', root), 'utf8');
  const handlers = new Map();
  const entries = new Map();
  const cache = {
    async addAll(urls) {
      for (const url of urls) {
        const relative = new URL(url).pathname.replace(/^\/ui\//, '') || 'index.html';
        entries.set(url, new Response(await fs.readFile(new URL(relative, root))));
      }
    },
    async put(request, response) { entries.set(request.url, response); },
  };
  const self = {
    registration: { scope: 'https://example.test/ui/' },
    location: { origin: 'https://example.test' },
    addEventListener(type, handler) { handlers.set(type, handler); },
    skipWaiting() {},
    clients: { claim() {} },
  };
  const caches = {
    async open() { return cache; },
    async match(request) { return entries.get(request.url)?.clone(); },
  };
  vm.runInNewContext(source, { self, caches, URL, Response, console, fetch: async () => { throw new Error('offline'); } });
  let install;
  handlers.get('install')({ waitUntil(promise) { install = promise; } });
  await install;
  for (const name of ['JetBrainsMono-Variable.woff2', 'DepartureMono-Regular.woff2']) {
    const font = await fs.readFile(new URL(`fonts/${name}`, root));
    let response;
    handlers.get('fetch')({
      request: new Request(`https://example.test/ui/fonts/${name}`),
      respondWith(promise) { response = promise; },
    });
    const actual = await response;
    assert.equal(actual.status, 200);
    assert.deepEqual(Buffer.from(await actual.arrayBuffer()), font);
  }
});

import assert from 'node:assert/strict';
import test from 'node:test';
import fs from 'node:fs';
import vm from 'node:vm';
import { apiService } from '../services/api.service.js';
import { experimentService } from '../services/experiment.service.js';

test('profile catalogue requests bypass HTTP cache and reject malformed success payloads', async (t) => {
  const requests = [];
  t.mock.method(apiService, 'get', async (...args) => { requests.push(args); return { profiles: [] }; });
  assert.deepEqual(await experimentService.listProfiles(), []);
  assert.deepEqual(requests[0], ['/api/v1/experiments/setup-profiles', {}, { cache: 'no-store' }]);
  t.mock.method(apiService, 'get', async () => ({ message: 'not a profile catalogue' }));
  await assert.rejects(experimentService.listProfiles(), /Ungültige Profilantwort/);
});

test('service worker never replaces failed profile reads with a cached old revision', async () => {
  const handlers = {};
  const requests = [];
  const offline = new Error('network offline');
  const context = {
    URL, Response,
    self: {
      registration: { scope: 'http://127.0.0.1:8080/ui/' },
      location: { origin: 'http://127.0.0.1:8080' },
      addEventListener(name, handler) { handlers[name] = handler; },
    },
    fetch: async (...args) => { requests.push(args); throw offline; },
    caches: { match() { assert.fail('profile API must not read cached success'); } },
  };
  vm.runInNewContext(fs.readFileSync(new URL('../sw.js', import.meta.url), 'utf8'), context);
  for (const path of ['/api/v1/experiments/setup-profiles', '/api/v1/experiments/setup-profiles/room/setup-v2-draft']) {
    let response;
    const request = new Request(`http://127.0.0.1:8080${path}`);
    handlers.fetch({ request, respondWith(value) { response = value; } });
    await assert.rejects(response, /network offline/);
    assert.equal(requests.at(-1)[1].cache, 'no-store');
  }
});

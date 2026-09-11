import assert from 'node:assert/strict';
import { test } from 'node:test';
import { getApiToken, setApiToken, sensingProtocols, websocketProtocols } from '../services/ws-auth.js';

test('credential transport preserves UTF-8 bytes without URL or protocol delimiters', () => {
  const token = 'sëcret ,+/=🙂';
  const offers = websocketProtocols(token);
  assert.equal(offers[0], 'ruview.v1');
  assert.equal(offers[1], `ruview.bearer.${Buffer.from(token).toString('hex')}`);
  assert.match(offers[1], /^[a-z0-9.]+$/);
  assert.deepEqual(websocketProtocols(''), []);
});

test('session token is scoped to UI host and protected routes, with explicit clearing', () => {
  const values = new Map();
  globalThis.location = new URL('https://sensing.example:3000/ui/');
  globalThis.sessionStorage = {
    getItem: key => values.get(key), setItem: (key, value) => values.set(key, value),
    removeItem: key => values.delete(key),
  };
  setApiToken('secret');
  assert.equal(getApiToken('wss://sensing.example:3001/ws/sensing'), 'secret');
  assert.equal(getApiToken('wss://other.example/ws/sensing'), '');
  assert.deepEqual(sensingProtocols('wss://other.example/ws/sensing'), []);
  setApiToken('remote-token', 'https://other.example');
  assert.deepEqual(sensingProtocols('wss://other.example/ws/sensing'), websocketProtocols('remote-token'));
  assert.equal(getApiToken('wss://sensing.example/ws/sensing'), 'secret');
  assert.deepEqual(sensingProtocols('wss://sensing.example/ws/field'), []);
  for (const path of ['/ws/sensing', '/ws/introspection', '/api/v1/stream/pose']) {
    assert.deepEqual(sensingProtocols(`wss://sensing.example${path}`), websocketProtocols('secret'));
  }
  setApiToken('');
  assert.equal(getApiToken(), '');
  delete globalThis.location;
  delete globalThis.sessionStorage;
});

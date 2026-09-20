import assert from 'node:assert/strict';
import fs from 'node:fs';
import test from 'node:test';

import { API_CONFIG, LEGACY_ENDPOINTS } from '../config/api.config.js';

test('the active endpoint catalogue contains registered browser API paths', () => {
  assert.deepEqual(API_CONFIG.ENDPOINTS.POSE, {
    CURRENT: '/api/v1/pose/current',
    ZONES_SUMMARY: '/api/v1/pose/zones/summary',
    STATS: '/api/v1/pose/stats',
  });
  assert.deepEqual(API_CONFIG.ENDPOINTS.STREAM, {
    STATUS: '/api/v1/stream/status',
    WS_POSE: '/api/v1/stream/pose',
  });
  assert.equal(API_CONFIG.ENDPOINTS.NODES, '/api/v1/nodes');
  assert.equal(API_CONFIG.ENDPOINTS.DEV, undefined);
});

test('the production shell contains no removed placeholder tabs or claims', () => {
  const html = fs.readFileSync(new URL('../index.html', import.meta.url), 'utf8');
  const sensing = fs.readFileSync(new URL('../components/SensingTab.js', import.meta.url), 'utf8');

  assert.doesNotMatch(html, /data-tab="(?:architecture|performance|applications)"/);
  assert.doesNotMatch(html, /id="(?:architecture|performance|applications)"/);
  assert.doesNotMatch(html, /24 body regions|100Hz|\$30|3×3 Antenna Array|run-offline-demo/i);
  assert.match(html, /id="localControlHelper"/);
  assert.match(html, /id="hardware-node-list"/);
  assert.match(sensing, /id="classLabel">UNKNOWN/);
  assert.doesNotMatch(sensing, /id="val(?:Variance|Motion|Breath|Spectral)">0/);
  const serviceWorker = fs.readFileSync(new URL('../sw.js', import.meta.url), 'utf8');
  assert.match(serviceWorker, /components\/ServerControlPanel\.js/);
  assert.match(serviceWorker, /ruview-v11-device-identities/);
});

test('unregistered compatibility paths are explicit legacy endpoints', () => {
  assert.equal(LEGACY_ENDPOINTS.POSE.ANALYZE, '/api/v1/pose/analyze');
  assert.equal(LEGACY_ENDPOINTS.STREAM.START, '/api/v1/stream/start');
  assert.equal(LEGACY_ENDPOINTS.STREAM.WS_EVENTS, '/api/v1/stream/events');
  assert.equal(LEGACY_ENDPOINTS.DEV.RESET, '/api/v1/dev/reset');
});

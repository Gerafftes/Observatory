import assert from 'node:assert/strict';
import test from 'node:test';

import { API_CONFIG, LEGACY_ENDPOINTS } from '../config/api.config.js';

test('the active endpoint catalogue contains only registered pose and stream paths', () => {
  assert.deepEqual(API_CONFIG.ENDPOINTS.POSE, {
    CURRENT: '/api/v1/pose/current',
    ZONES_SUMMARY: '/api/v1/pose/zones/summary',
    STATS: '/api/v1/pose/stats',
  });
  assert.deepEqual(API_CONFIG.ENDPOINTS.STREAM, {
    STATUS: '/api/v1/stream/status',
    WS_POSE: '/api/v1/stream/pose',
  });
  assert.equal(API_CONFIG.ENDPOINTS.DEV, undefined);
});

test('unregistered compatibility paths are explicit legacy endpoints', () => {
  assert.equal(LEGACY_ENDPOINTS.POSE.ANALYZE, '/api/v1/pose/analyze');
  assert.equal(LEGACY_ENDPOINTS.STREAM.START, '/api/v1/stream/start');
  assert.equal(LEGACY_ENDPOINTS.STREAM.WS_EVENTS, '/api/v1/stream/events');
  assert.equal(LEGACY_ENDPOINTS.DEV.RESET, '/api/v1/dev/reset');
});

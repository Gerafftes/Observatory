'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const {
  medianFilter,
  parseIqHex,
  preprocessIq,
  removeLinearTrend,
  unwrapPhase,
} = require('../csi-preprocessing');

test('signed IQ produces amplitude and atan2 phase', () => {
  const iq = parseIqHex('00ff0304fc03');
  assert.deepEqual([...iq], [0, -1, 3, 4, -4, 3]);
  const result = preprocessIq(iq, 2);
  assert.deepEqual(result.amplitude, [5, 5]);
  assert.ok(Math.abs(result.rawPhase[0] - Math.atan2(4, 3)) < 1e-12);
  assert.ok(Math.abs(result.rawPhase[1] - Math.atan2(3, -4)) < 1e-12);
});

test('phase unwrap removes two-pi discontinuities', () => {
  const values = unwrapPhase([3.0, -3.0, -2.8]);
  assert.ok(values[1] > values[0]);
  assert.ok(Math.abs(values[1] - (2 * Math.PI - 3.0)) < 1e-12);
});

test('median filtering rejects spikes and detrending removes a line', () => {
  assert.deepEqual(medianFilter([1, 2, 99, 4, 5]), [1.5, 2, 4, 5, 4.5]);
  const detrended = removeLinearTrend([2, 5, 8, 11]);
  assert.ok(detrended.every(value => Math.abs(value) < 1e-12));
});

test('malformed and short IQ inputs fail closed', () => {
  assert.throws(() => parseIqHex('0xz1'), /hexadecimal/);
  assert.throws(() => preprocessIq(new Int8Array(4), 2), /expected at least/);
});

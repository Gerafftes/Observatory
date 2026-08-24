'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const {
  RX_ORDER,
  flattenTimeMajorChannel,
  splitBySession,
  timeMajorToFeatureMajor,
  torsoFromCoco,
} = require('../torso-v1');

test('torso schema uses fixed COCO shoulder and hip order', () => {
  const keypoints = Array.from({ length: 17 }, (_, index) => [index / 20, index / 25, 0.9]);
  const torso = torsoFromCoco(keypoints);
  assert.equal(torso.left_shoulder.x, 5 / 20);
  assert.equal(torso.right_shoulder.x, 6 / 20);
  assert.equal(torso.left_hip.x, 11 / 20);
  assert.equal(torso.right_hip.x, 12 / 20);
});

test('fixed RX order and dimensions are enforced', () => {
  const frames = Object.fromEntries(RX_ORDER.map((rx, rxIndex) => [
    rx,
    Array.from({ length: 20 }, (_, time) => ({ amplitude: new Array(64).fill(rxIndex * 100 + time) })),
  ]));
  const flat = flattenTimeMajorChannel(frames, 'amplitude');
  assert.equal(flat.length, 20 * 4 * 64);
  assert.equal(flat[64], 100);
  assert.throws(() => flattenTimeMajorChannel({ ...frames, RX4: frames.RX4.slice(1) }, 'amplitude'), /RX4 time 19/);
});

test('time-major channels convert to model feature-major layout', () => {
  assert.deepEqual([...timeMajorToFeatureMajor([1, 2, 3, 4, 5, 6], 2, 3)], [1, 4, 2, 5, 3, 6]);
});

test('training and validation never share a session', () => {
  const samples = ['a', 'a', 'b', 'b', 'c'].map(sessionId => ({ sessionId }));
  const split = splitBySession(samples, 0.34);
  const trainIds = new Set(split.train.map(sample => sample.sessionId));
  const evalIds = new Set(split.eval.map(sample => sample.sessionId));
  assert.equal([...trainIds].some(id => evalIds.has(id)), false);
  assert.throws(() => splitBySession([{ sessionId: 'only' }], 0.2), /at least two/);
});

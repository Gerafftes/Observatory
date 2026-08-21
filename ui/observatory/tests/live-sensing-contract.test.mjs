import assert from 'node:assert/strict';

import {
  OBSERVATORY_LIVE_FRAME_TIMEOUT_MS,
  isExplicitEsp32Frame,
  resolveObservatoryRenderContract,
  resolveObservatorySourceState,
  validateObservatoryGeometry,
  validateObservatoryPosition,
} from '../js/live-sensing-contract.js';

const room = [4.02, 2.59, 3.44];
const txPosition = [1.51, 1.19, 0.39];
const receivers = [
  [0, 0.5, 0.28],
  [4.02, 0.87, 0.97],
  [0, 0.74, 2.11],
  [4.02, 0.87, 2.46],
];

function hardwareFrame(overrides = {}) {
  return {
    type: 'sensing_update',
    source: 'esp32',
    room_dimensions: room,
    tx_position: txPosition,
    nodes: receivers.map((position, index) => ({
      node_id: index + 1,
      position,
    })),
    classification: {
      presence: true,
      motion_level: 'present_still',
      confidence: 0.8,
    },
    position_estimate: {
      state: 'position',
      point_id: 'P05',
      coordinates_m: [2.01, 0, 1.72],
    },
    signal_field: {
      grid_size: [2, 1, 2],
      values: [0.1, 0.2, 0.3, 0.4],
    },
    ...overrides,
  };
}

const nowMs = 20_000;
const freshReceipt = nowMs - 100;

// An open socket alone is not evidence of live hardware.
assert.deepEqual(
  resolveObservatorySourceState({
    selectedSource: 'ws',
    connectionState: 'open',
    connectionOpenedAtMs: nowMs - 100,
    frame: null,
    receivedAtMs: null,
    nowMs,
  }),
  {
    state: 'connecting',
    label: 'CONNECTING',
    reason: 'Waiting for the first explicit sensing frame.',
  }
);

assert.equal(
  resolveObservatorySourceState({
    selectedSource: 'ws',
    connectionState: 'open',
    connectionOpenedAtMs: nowMs - OBSERVATORY_LIVE_FRAME_TIMEOUT_MS - 1,
    frame: null,
    receivedAtMs: null,
    nowMs,
  }).state,
  'stale'
);

// Only the exact ESP32 source may earn the LIVE badge.
for (const source of [
  'esp32:offline',
  'esp32-live',
  'wifi',
  'live',
  'hardware',
  '',
  undefined,
]) {
  const frame = hardwareFrame({ source });
  const state = resolveObservatorySourceState({
    selectedSource: 'ws',
    connectionState: 'open',
    frame,
    receivedAtMs: freshReceipt,
    nowMs,
  });
  assert.equal(state.state, 'stale', `${String(source)} must not be marked LIVE`);
  assert.equal(isExplicitEsp32Frame(frame), false);
}

const spoofedSimulation = hardwareFrame({ _simulated: true });
assert.equal(isExplicitEsp32Frame(spoofedSimulation), false);
assert.equal(
  resolveObservatorySourceState({
    selectedSource: 'ws',
    connectionState: 'open',
    frame: spoofedSimulation,
    receivedAtMs: freshReceipt,
    nowMs,
  }).state,
  'simulated'
);

assert.equal(
  resolveObservatorySourceState({
    selectedSource: 'ws',
    connectionState: 'open',
    frame: hardwareFrame(),
    receivedAtMs: nowMs - OBSERVATORY_LIVE_FRAME_TIMEOUT_MS - 1,
    nowMs,
  }).state,
  'stale'
);

assert.equal(
  resolveObservatorySourceState({
    selectedSource: 'ws',
    connectionState: 'closed',
    frame: hardwareFrame(),
    receivedAtMs: freshReceipt,
    nowMs,
  }).state,
  'stale'
);

assert.equal(
  resolveObservatorySourceState({
    selectedSource: 'ws',
    connectionState: 'open',
    frame: hardwareFrame(),
    receivedAtMs: freshReceipt,
    nowMs,
  }).state,
  'live'
);

for (const source of ['demo', 'simulate', 'simulated']) {
  assert.equal(
    resolveObservatorySourceState({
      selectedSource: 'ws',
      connectionState: 'open',
      frame: { source },
      receivedAtMs: freshReceipt,
      nowMs,
    }).state,
    'simulated'
  );
}
assert.equal(
  resolveObservatorySourceState({
    selectedSource: 'demo',
    connectionState: 'closed',
    frame: null,
    receivedAtMs: null,
    nowMs,
  }).state,
  'simulated'
);

const validGeometry = validateObservatoryGeometry(hardwareFrame());
assert.equal(validGeometry.valid, true);
assert.deepEqual(validGeometry.roomDimensions, room);
assert.deepEqual(validGeometry.txPosition, txPosition);
assert.deepEqual(
  validGeometry.receivers.map((receiver) => receiver.position),
  receivers
);

for (const overrides of [
  { room_dimensions: null },
  { room_dimensions: [4.02, 2.59] },
  { room_dimensions: ['4.02', 2.59, 3.44] },
  { room_dimensions: [4.02, 0, 3.44] },
  { tx_position: [4.03, 1.19, 0.39] },
  { tx_position: [1.51, Number.NaN, 0.39] },
  { nodes: [] },
  { nodes: hardwareFrame().nodes.slice(0, 3) },
  { nodes: [{ node_id: 1, position: [0, 0.5] }] },
  {
    nodes: hardwareFrame().nodes.map((node, index) => ({
      ...node,
      node_id: index + 2,
    })),
  },
  {
    nodes: [
      { node_id: 1, position: [0, 0.5, 0.28] },
      { node_id: 1, position: [4.02, 0.87, 0.97] },
    ],
  },
]) {
  assert.equal(validateObservatoryGeometry(hardwareFrame(overrides)).valid, false);
}

const exactPosition = validateObservatoryPosition(hardwareFrame(), validGeometry);
assert.deepEqual(exactPosition, {
  valid: true,
  state: 'position',
  pointId: 'P05',
  coordinates: [2.01, 0, 1.72],
  reason: null,
});

const presenceFalseFrame = hardwareFrame({
  classification: { presence: false },
});
assert.equal(
  validateObservatoryPosition(presenceFalseFrame, validGeometry).state,
  'absent'
);
assert.equal(
  resolveObservatoryRenderContract(presenceFalseFrame).showHardwareMarker,
  false
);

for (const position_estimate of [
  { state: 'uncalibrated' },
  { state: 'ambiguous' },
  { state: 'position', point_id: 'P10', coordinates_m: [2.01, 0, 1.72] },
  { state: 'position', point_id: 'P05', coordinates_m: ['2.01', 0, 1.72] },
  { state: 'position', point_id: 'P05', coordinates_m: [4.03, 0, 1.72] },
]) {
  assert.equal(
    resolveObservatoryRenderContract(
      hardwareFrame({ position_estimate })
    ).showHardwareMarker,
    false
  );
}

// Legacy/coarse fields must never become a hardware position.
for (const coarseFrame of [
  hardwareFrame({
    position_estimate: { state: 'uncalibrated' },
    localization: { status: 'coarse', position: { x: 2.01, z: 1.72 } },
  }),
  hardwareFrame({
    position_estimate: undefined,
    persons: [{ position: [2.01, 0, 1.72] }],
  }),
]) {
  const contract = resolveObservatoryRenderContract(coarseFrame);
  assert.equal(contract.showHardwareMarker, false);
  assert.equal(contract.showHardwareField, false);
}

const validContract = resolveObservatoryRenderContract(hardwareFrame());
assert.equal(validContract.showHardwareMarker, true);
assert.equal(validContract.showHardwareField, true);
assert.equal(validContract.signalField.columns, 2);
assert.equal(validContract.signalField.rows, 2);
assert.equal(validContract.signalField.cellSizeX, room[0]);
assert.equal(validContract.signalField.cellSizeZ, room[2]);

for (const signal_field of [
  { grid_size: [2, 1, 2], values: [0.1] },
  { grid_size: [2, 2, 2], values: Array(4).fill(0.1) },
  { grid_size: [2, 1, 2], values: [0.1, 0.2, Number.NaN, 0.4] },
]) {
  assert.equal(
    resolveObservatoryRenderContract(
      hardwareFrame({ signal_field })
    ).showHardwareField,
    false
  );
}

console.log('Observatory live sensing contract tests passed.');

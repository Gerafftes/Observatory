import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

import {
  MMWAVE_REJECTION_VISIBLE_MS,
  MMWAVE_STATUS_FRESH_MS,
  MmwaveDebugView,
  clampScenePositionToRoom,
  normalizeMmwaveDebugStatus,
  normalizeRxDebugState,
  roomPositionToScene,
} from '../components/MmwaveDebugView.js';
import { displayCoordinatesForRoom } from '../components/gaussian-splats.js';

const room = [4.02, 2.59, 3.44];

test('room coordinates use the current UI orientation and stay centered', () => {
  assert.deepEqual(roomPositionToScene([0, 0, 0], room), [2.01, 0, -1.72]);
  assert.deepEqual(roomPositionToScene(room, room), [-2.01, 2.59, 1.72]);

  const sourcePosition = [0.75, 1.2, 2.1];
  const displayPosition = displayCoordinatesForRoom(sourcePosition, room);
  assert.deepEqual(roomPositionToScene(sourcePosition, room), [
    displayPosition[0] - room[0] / 2,
    displayPosition[1],
    displayPosition[2] - room[2] / 2,
  ]);
});

test('fresh single mmWave target becomes the signal-red reference marker', () => {
  const state = normalizeMmwaveDebugStatus({
    state: 'valid',
    room_dimensions_m: room,
    receiver_positions_m: [[0, 0.5, 0.28], [4.02, 0.87, 0.97]],
    target_count: 1,
    target_raw_position_mm: [0, 1580],
    target_position_mm: [2010, 1720],
    packet_age_ms: MMWAVE_STATUS_FRESH_MS,
    mounting_position_m: [0, 1.2, 1.72],
  }, 1000);

  assert.equal(state.accepted, true);
  assert.deepEqual(state.targetRawPositionMm, [0, 1580]);
  assert.deepEqual(state.targetPositionMm, [2010, 1720]);
  assert.deepEqual(state.diagnosticRawPositionMm, [0, 1580]);
  assert.deepEqual(state.diagnosticRoomPositionMm, [2010, 1720]);
  assert.ok(Math.abs(state.diagnosticScenePositionM[0]) < 1e-12);
  assert.deepEqual(state.diagnosticScenePositionM.slice(1), [0.08, 0]);
  assert.deepEqual(state.mountingPositionM, [0, 1.2, 1.72]);
  assert.deepEqual(state.receiverPositionsM, [[0, 0.5, 0.28], [4.02, 0.87, 0.97]]);
  assert.equal(state.rejectionVisible, false);
});

test('no target or stale target never produces a valid radar marker', () => {
  const noTarget = normalizeMmwaveDebugStatus({
    state: 'no_target',
    target_count: 0,
    target_position_mm: [2010, 1720],
    packet_age_ms: 20,
  });
  const stale = normalizeMmwaveDebugStatus({
    state: 'valid',
    room_dimensions_m: room,
    target_count: 1,
    target_position_mm: [2010, 1720],
    packet_age_ms: MMWAVE_STATUS_FRESH_MS + 1,
  });

  assert.equal(noTarget.accepted, false);
  assert.equal(noTarget.targetPositionMm, null);
  assert.equal(stale.accepted, false);
});

test('recent room-bounds rejects retain their diagnostic position', () => {
  const typed = normalizeMmwaveDebugStatus({
    state: 'valid',
    room_dimensions_m: room,
    target_count: 1,
    target_position_mm: [2010, 1720],
    packet_age_ms: 40,
    last_rejection: {
      category: 'room_bounds',
      reason: 'target [6162, 3665] mm is outside room',
      raw_position_mm: [365, 4112],
      position_mm: [6162, 3665],
      age_ms: MMWAVE_REJECTION_VISIBLE_MS,
    },
  });
  const legacy = normalizeMmwaveDebugStatus({
    last_rejection: {
      category: 'room_bounds',
      reason: 'target [4078, 2831] mm is outside room',
      age_ms: 10,
    },
  });

  assert.equal(typed.rejectionVisible, true);
  assert.deepEqual(typed.rejectedRawPositionMm, [365, 4112]);
  assert.deepEqual(typed.rejectedPositionMm, [6162, 3665]);
  assert.deepEqual(typed.diagnosticRawPositionMm, [365, 4112]);
  assert.deepEqual(typed.diagnosticRoomPositionMm, [6162, 3665]);
  assert.deepEqual(typed.diagnosticScenePositionM, [-4.152, 0.1, 1.945]);
  assert.equal(legacy.rejectionVisible, true);
  assert.deepEqual(legacy.rejectedPositionMm, [4078, 2831]);
});

test('rejected marker presentation stays on the room edge without changing diagnostics', () => {
  const rejectedScene = [-4.152, 0.1, 1.945];
  const clamped = clampScenePositionToRoom(rejectedScene, room);
  assert.ok(Math.abs(clamped[0] + 1.87) < 1e-9);
  assert.deepEqual(clamped.slice(1), [0.1, 1.58]);
  assert.deepEqual(rejectedScene, [-4.152, 0.1, 1.945]);
});

test('old rejects disappear from the viewport after their diagnostic TTL', () => {
  const state = normalizeMmwaveDebugStatus({
    last_rejection: {
      category: 'room_bounds',
      reason: 'target [6162, 3665] mm is outside room',
      age_ms: MMWAVE_REJECTION_VISIBLE_MS + 1,
    },
  });
  assert.equal(state.rejectionVisible, false);
  assert.equal(state.rejectedPositionMm, null);
  assert.equal(state.rejectedRawPositionMm, null);
});

test('simulated frames cannot create an RX/CSI marker', () => {
  const state = normalizeRxDebugState({
    source: 'simulated',
    _simulated: true,
    room_dimensions: room,
    position_estimate: { state: 'position', coordinates_m: [2, 1, 1.7], point_id: 'P05' },
  }, 1000, 1200);
  assert.equal(state.simulated, true);
  assert.equal(state.validPosition, false);
  assert.equal(state.coordinates, null);
});

test('fresh live RX position is independent from the radar marker', () => {
  const state = normalizeRxDebugState({
    source: 'esp32',
    room_dimensions: room,
    nodes: [{ node_id: 1, position: [0, 1, 0] }],
    position_estimate: { state: 'position', coordinates_m: [2.4, 0, 1.9], point_id: 'P05' },
  }, 1000, 1200);
  assert.equal(state.validPosition, true);
  assert.deepEqual(state.coordinates, [2.4, 0, 1.9]);
  assert.deepEqual(state.nodes[0].position, [0, 1, 0]);
});

test('debug component keeps the source legend explicit', () => {
  const source = readFileSync(new URL('../components/MmwaveDebugView.js', import.meta.url), 'utf8');
  assert.match(source, /class="mmwave-assistant sensing-mmwave-debug"/);
  assert.match(source, /data-mmwave-debug-visual="samaritan-v2"/);
  assert.match(source, /class="mmwave-assistant-header sensing-mmwave-debug-header"/);
  assert.match(source, /data-mmwave-debug-hardware/);
  assert.match(source, /RX\/TX anzeigen/);
  assert.match(source, /getSetupGeometry/);
  assert.match(source, /_configuredGeometry/);
  assert.match(source, /mmWave · Ground Truth/);
  assert.match(source, /RX\/CSI · WiFi-Schätzung/);
  assert.match(source, /Radar verworfen · außerhalb Raum/);
  assert.match(source, /Draufsicht/);
  assert.match(source, /keine Fusion/);
  assert.match(source, /createMarkerLabel\('MMWAVE1'/);
  assert.match(source, /createMarkerLabel\('RADAR TARGET'/);
  assert.match(source, /createMarkerLabel\(`RX\$\{node\.id\}`/);
  assert.doesNotMatch(source, /PlaneGeometry|CylinderGeometry|ConeGeometry|TorusGeometry/);
  assert.doesNotMatch(source, /0xffa62b|0x2dd4e8|0x7dd3fc/);
  assert.doesNotMatch(source, /children\[1\]\.scale/);
  assert.equal(typeof MmwaveDebugView, 'function');
});

import assert from 'node:assert/strict';
import test from 'node:test';

import {
  geometryEntities,
  markerWallDistance,
  planDistance,
  setMarkerWallDistance,
  setPlanDistance,
  RoomGeometryEditor,
  validateGeometryDraft,
} from '../components/RoomGeometryEditor.js';
import { defaultSetupProfileDocument } from '../components/ObservatoryControlCenter.js';

test('CAD geometry editor exposes TX and all four RX markers', () => {
  const profile = defaultSetupProfileDocument();
  assert.deepEqual(geometryEntities(profile).map((entity) => entity.id), ['TX', 'RX1', 'RX2', 'RX3', 'RX4']);
  assert.equal(validateGeometryDraft(profile).valid, true);
});

test('CAD axis triad distinguishes the X/Z top plan from Y height', () => {
  const container = { innerHTML: '' };
  const editor = new RoomGeometryEditor(container, { document: defaultSetupProfileDocument() });

  editor.render();

  assert.match(container.innerHTML, /data-axis="x"/);
  assert.match(container.innerHTML, /data-axis="z"/);
  assert.match(container.innerHTML, /data-axis="y"/);
  assert.match(container.innerHTML, /data-axis="z"[^>]*x1="92"[^>]*y1="62"[^>]*x2="92"[^>]*y2="96"/);
  assert.match(container.innerHTML, /\+Y \/ H/);
  assert.match(container.innerHTML, /y = Höhe/);
});

test('CAD geometry validation rejects out-of-room and duplicate receiver positions', () => {
  const profile = defaultSetupProfileDocument();
  profile.transmitter.position_m[0] = profile.room_dimensions_m[0] + 0.01;
  profile.receivers[1].position_m = [...profile.receivers[0].position_m];

  const validation = validateGeometryDraft(profile);
  assert.equal(validation.valid, false);
  assert.match(validation.errors.join(' '), /TX: Position liegt außerhalb des Raums/);
  assert.match(validation.errors.join(' '), /RX-Positionen müssen eindeutig sein/);
});

test('CAD geometry validation requires the canonical four receiver IDs', () => {
  const profile = defaultSetupProfileDocument();
  profile.receivers = profile.receivers.slice(0, 3);

  const validation = validateGeometryDraft(profile);
  assert.equal(validation.valid, false);
  assert.match(validation.errors.join(' '), /genau RX1, RX2, RX3 und RX4/);
});

test('Shift selection keeps an anchor and adds a second marker for distance editing', () => {
  const editor = new RoomGeometryEditor(null, { document: defaultSetupProfileDocument() });

  editor._select('RX1');
  editor._select('TX', true);

  assert.deepEqual(editor.selectedIds, ['RX1', 'TX']);
  assert.equal(editor.selectedId, 'RX1');
});

test('Shift selection accepts a wall as the second reference', () => {
  const editor = new RoomGeometryEditor(null, { document: defaultSetupProfileDocument() });

  editor._select('RX2');
  editor._select('WALL_XMAX', true);

  assert.deepEqual(editor.selectedIds, ['RX2', 'WALL_XMAX']);
  assert.equal(editor.selectedId, 'RX2');
});

test('plan distance moves only the second marker and preserves its height', () => {
  const profile = defaultSetupProfileDocument();
  const originalY = profile.receivers[0].position_m[1];
  const result = setPlanDistance(profile, 'TX', 'RX1', 1.25);

  assert.equal(result.error, '');
  assert.ok(Math.abs(planDistance(result.document.transmitter.position_m, result.document.receivers[0].position_m) - 1.25) < 1e-9);
  assert.equal(result.document.receivers[0].position_m[1], originalY);
  assert.notDeepEqual(result.document.receivers[0].position_m, profile.receivers[0].position_m);
});

test('plan distance rejects a target that would leave the room', () => {
  const profile = defaultSetupProfileDocument();
  const result = setPlanDistance(profile, 'TX', 'RX1', 10);

  assert.match(result.error, /passt.*nicht in den Raum/);
  assert.deepEqual(result.document.receivers[0].position_m, profile.receivers[0].position_m);
});

test('marker-wall distance moves the marker normal to the selected wall', () => {
  const profile = defaultSetupProfileDocument();
  const originalZ = profile.receivers[1].position_m[2];
  const result = setMarkerWallDistance(profile, 'RX2', 'WALL_XMAX', 0.75);

  assert.equal(result.error, '');
  assert.ok(Math.abs(result.document.receivers[1].position_m[0] - (profile.room_dimensions_m[0] - 0.75)) < 1e-9);
  assert.equal(result.document.receivers[1].position_m[2], originalZ);
  assert.ok(Math.abs(markerWallDistance(result.document.receivers[1].position_m, 'WALL_XMAX', profile.room_dimensions_m) - 0.75) < 1e-9);
});

test('marker-wall distance rejects a value outside the room', () => {
  const profile = defaultSetupProfileDocument();
  const result = setMarkerWallDistance(profile, 'RX2', 'WALL_XMAX', profile.room_dimensions_m[0] + 1);

  assert.match(result.error, /Wandabstand.*nicht in den Raum/);
  assert.deepEqual(result.document.receivers[1].position_m, profile.receivers[1].position_m);
});

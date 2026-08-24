import assert from 'node:assert/strict';
import test from 'node:test';

import {
  geometryEntities,
  markerWallDistance,
  planDistance,
  setMarkerWallDistance,
  setPlanDistance,
  RoomGeometryEditor,
  sensorOutsideDistance,
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
  assert.match(container.innerHTML, /data-axis="z"[^>]*x1="[0-9.]+"[^>]*y1="[0-9.]+"[^>]*x2="[0-9.]+"[^>]*y2="[0-9.]+"/);
  assert.match(container.innerHTML, /\+Y \/ H/);
  assert.match(container.innerHTML, /y: Höhe/);
});

test('CAD rulers avoid overlapping room-edge labels', () => {
  const container = { innerHTML: '' };
  const editor = new RoomGeometryEditor(container, { document: defaultSetupProfileDocument() });

  editor.render();

  assert.doesNotMatch(container.innerHTML, /data-cad-axis-tick="x" data-cad-axis-value="4\.02"/);
  assert.match(container.innerHTML, /data-cad-axis-tick="x" data-cad-axis-value="4\.50"/);
  assert.match(container.innerHTML, /data-cad-axis-tick="z" data-cad-axis-value="3\.44"/);
  assert.doesNotMatch(container.innerHTML, /data-cad-axis-tick="z" data-cad-axis-value="3\.50"/);
});

test('CAD geometry validation rejects out-of-room and duplicate receiver positions', () => {
  const profile = defaultSetupProfileDocument();
  profile.sensor_mount_radius_m = 0;
  profile.transmitter.position_m[0] = profile.room_dimensions_m[0] + 0.01;
  profile.receivers[1].position_m = [...profile.receivers[0].position_m];

  const validation = validateGeometryDraft(profile);
  assert.equal(validation.valid, false);
  assert.match(validation.errors.join(' '), /TX: Außenradius/);
  assert.match(validation.errors.join(' '), /RX-Positionen müssen eindeutig sein/);
});

test('CAD geometry allows horizontal exterior mounts only within the configured radius', () => {
  const profile = defaultSetupProfileDocument();
  profile.transmitter.position_m[0] = -0.25;
  profile.receivers[1].position_m[0] = profile.room_dimensions_m[0] + 0.5;

  assert.equal(sensorOutsideDistance(profile.transmitter.position_m, profile.room_dimensions_m), 0.25);
  assert.equal(validateGeometryDraft(profile).valid, true);

  profile.transmitter.position_m[0] = -0.51;
  const invalid = validateGeometryDraft(profile);
  assert.equal(invalid.valid, false);
  assert.match(invalid.errors.join(' '), /TX: Außenradius von 0\.50 m überschritten/);
});

test('CAD geometry keeps sensor height inside the room', () => {
  const profile = defaultSetupProfileDocument();
  profile.transmitter.position_m[1] = profile.room_dimensions_m[1] + 0.01;
  const validation = validateGeometryDraft(profile);

  assert.equal(validation.valid, false);
  assert.match(validation.errors.join(' '), /TX: Höhe liegt außerhalb des Raums/);
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

test('CAD view draws a visible connection between two shifted markers', () => {
  const container = { innerHTML: '' };
  const editor = new RoomGeometryEditor(container, { document: defaultSetupProfileDocument() });

  editor._select('RX1');
  editor._select('TX', true);

  assert.match(container.innerHTML, /data-cad-selection-line/);
  assert.match(container.innerHTML, /class="occ-cad-selection-line"/);
  assert.match(container.innerHTML, /data-cad-sensor-zone/);
  assert.match(container.innerHTML, /data-cad-sensor-radius/);
});

test('CAD view connects a shifted marker to the selected wall', () => {
  const container = { innerHTML: '' };
  const editor = new RoomGeometryEditor(container, { document: defaultSetupProfileDocument() });

  editor._select('RX2');
  editor._select('WALL_XMAX', true);

  assert.match(container.innerHTML, /data-cad-selection-line/);
  assert.match(container.innerHTML, /class="occ-cad-selection-line-backdrop"/);
});

test('clicking an empty CAD area clears the selection and its connection line', () => {
  const container = { innerHTML: '' };
  const editor = new RoomGeometryEditor(container, { document: defaultSetupProfileDocument() });

  editor._select('RX1');
  editor._select('TX', true);
  editor._handleClick({
    target: {
      closest(selector) {
        return selector === '[data-cad-svg]' ? {} : null;
      },
    },
  });

  assert.deepEqual(editor.selectedIds, []);
  assert.equal(editor.selectedId, null);
  assert.match(container.innerHTML, /Keine Auswahl/);
  assert.doesNotMatch(container.innerHTML, /data-cad-selection-line/);
});

test('an explicitly empty selection remains empty when the editor is reconstructed', () => {
  const editor = new RoomGeometryEditor(null, {
    document: defaultSetupProfileDocument(),
    selectedIds: [],
  });

  assert.deepEqual(editor.selectedIds, []);
  assert.equal(editor.selectedId, null);
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

test('marker-wall distance accepts a negative exterior distance within the radius', () => {
  const profile = defaultSetupProfileDocument();
  const result = setMarkerWallDistance(profile, 'RX2', 'WALL_XMAX', -0.25);

  assert.equal(result.error, '');
  assert.equal(result.document.receivers[1].position_m[0], profile.room_dimensions_m[0] + 0.25);
  assert.equal(markerWallDistance(result.document.receivers[1].position_m, 'WALL_XMAX', profile.room_dimensions_m), -0.25);
});

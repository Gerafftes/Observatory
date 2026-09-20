import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

import {
  calculateOptimalMmwavePlacement,
  geometryEntities,
  markerWallDistance,
  mmwaveExteriorAllowed,
  mmwaveMountingPosition,
  mmwaveYawMdeg,
  mmwaveYawToPointMdeg,
  mmwaveYawToReceiverMdeg,
  planDistance,
  setMarkerWallDistance,
  setPlanDistance,
  setWallDistance,
  RoomGeometryEditor,
  sensorOutsideDistance,
  validateGeometryDraft,
  wallPairDistance,
} from '../components/RoomGeometryEditor.js';
import { defaultSetupProfileDocument } from '../components/ObservatoryControlCenter.js';

test('CAD room-size title does not capture clicks intended for markers behind it', () => {
  const stylesheet = readFileSync(new URL('../style.css', import.meta.url), 'utf8');

  assert.match(stylesheet, /\.occ-cad-room-title\s*\{[^}]*pointer-events:\s*none/);
});

test('CAD room dimensions are a title above the X axis label', () => {
  const container = { innerHTML: '' };
  const editor = new RoomGeometryEditor(container, {
    document: defaultSetupProfileDocument(),
    setupSource: 'sqlite',
  });

  editor.render();

  assert.match(container.innerHTML, /<text class="occ-cad-room-title"[^>]*>RAUM · 4\.02 × 3\.44 m · H 2\.59 m<\/text>/);
  assert.match(container.innerHTML, /<text class="occ-cad-setup-source is-sqlite"[^>]*>SQLITE-SETUP GELADEN<\/text>/);
  assert.ok(container.innerHTML.indexOf('occ-cad-room-title') < container.innerHTML.indexOf('X \/ LÄNGE \(m\)'));
});

test('CAD geometry editor exposes TX, all four RX markers, and the mmWave mount', () => {
  const profile = defaultSetupProfileDocument();
  assert.deepEqual(geometryEntities(profile).map((entity) => entity.id), ['TX', 'RX1', 'RX2', 'RX3', 'RX4', 'MMWAVE']);
  assert.deepEqual(geometryEntities(profile).at(-1).position_m, profile.mmwave.mounting_position_m);
  assert.equal(validateGeometryDraft(profile).valid, true);
});

test('CAD validation rejects malformed mmWave orientation metadata', () => {
  const profile = defaultSetupProfileDocument();
  profile.mmwave.yaw_mdeg = 360001;
  profile.mmwave.raw_x_inverted = 'true';

  const validation = validateGeometryDraft(profile);

  assert.equal(validation.valid, false);
  assert.match(validation.errors.join(' '), /mmwave\.yaw_mdeg/);
  assert.match(validation.errors.join(' '), /mmwave\.raw_x_inverted/);
});

test('legacy profiles without mmWave metadata get a safe editor fallback', () => {
  const profile = defaultSetupProfileDocument();
  delete profile.mmwave;

  assert.deepEqual(mmwaveMountingPosition(profile), [0.0, 1.2, 1.72]);
  assert.deepEqual(geometryEntities(profile).at(-1).position_m, [0.0, 1.2, 1.72]);
});

test('optimal mmWave placement covers the complete default room and every TX/RX reference', () => {
  const result = calculateOptimalMmwavePlacement(defaultSetupProfileDocument());

  assert.equal(result.ok, true);
  assert.equal(result.roomCoveragePercent, 100);
  assert.equal(result.coverageAngleDeg, 90);
  assert.ok(result.maxRoomDistanceM < 6);
  assert.equal(result.referencePointsCovered, 5);
  assert.equal(result.referencePointCount, 5);
  assert.deepEqual(result.positionM, [4.02, 1.2, 0]);
  assert.equal(result.yawMdeg, 135000);
});

test('optimal mmWave placement reacts to changed room and TX/RX geometry', () => {
  const profile = defaultSetupProfileDocument();
  profile.room_dimensions_m = [3, 2.4, 4];
  profile.transmitter.position_m = [0.2, 1, 3.8];
  profile.receivers.forEach((receiver, index) => {
    receiver.position_m = [0.1 + index * 0.05, 0.8, 3.6 + index * 0.05];
  });

  const result = calculateOptimalMmwavePlacement(profile);

  assert.equal(result.ok, true);
  assert.deepEqual(result.positionM, [0, 1.2, 4]);
  assert.equal(result.yawMdeg, -45000);
  assert.equal(result.referencePointsCovered, 5);
});

test('optimal mmWave placement uses an allowed exterior wall position when it shortens the worst room distance', () => {
  const profile = defaultSetupProfileDocument();
  profile.sensor_mount_radius_m = 2;

  const result = calculateOptimalMmwavePlacement(profile);

  assert.equal(result.ok, true);
  assert.equal(result.key, 'outside-z0');
  assert.ok(Math.abs(result.positionM[2] + 1.160474041071148) < 1e-12);
  assert.ok(result.maxRoomDistanceM < Math.hypot(4.02, 3.44));
  assert.ok(result.coverageAngleDeg <= 120.000001);
});

test('optimal mmWave placement stays inside the room when exterior mounting is disabled', () => {
  const profile = defaultSetupProfileDocument();
  profile.sensor_mount_radius_m = 2;
  profile.mmwave.allow_exterior = false;

  const result = calculateOptimalMmwavePlacement(profile);

  assert.equal(mmwaveExteriorAllowed(profile), false);
  assert.equal(result.ok, true);
  assert.equal(result.key, 'corner-xmax-z0');
  assert.equal(sensorOutsideDistance(result.positionM, profile.room_dimensions_m), 0);
});

test('optimal mmWave placement fails clearly when one sensor cannot cover the room', () => {
  const profile = defaultSetupProfileDocument();
  profile.room_dimensions_m = [10, 2.5, 10];
  profile.sensor_mount_radius_m = 5;

  const result = calculateOptimalMmwavePlacement(profile);

  assert.equal(result.ok, false);
  assert.match(result.error, /keine vollständige Raumabdeckung möglich/);
});

test('CAD view renders an editable mmWave marker, view cone, and orientation inspector', () => {
  const container = { innerHTML: '' };
  const editor = new RoomGeometryEditor(container, { document: defaultSetupProfileDocument() });

  editor._select('MMWAVE');
  editor.render();

  assert.match(container.innerHTML, /data-geometry-id="MMWAVE"/);
  assert.match(container.innerHTML, /occ-cad-marker-mmwave/);
  assert.match(container.innerHTML, /occ-cad-swatch-mmwave/);
  assert.match(container.innerHTML, /MMWAVE \[x \/ y \/ z\] m/);
  assert.match(container.innerHTML, /data-cad-mmwave-fov/);
  assert.match(container.innerHTML, /data-cad-mmwave-centerline/);
  assert.match(container.innerHTML, /data-cad-yaw-handle/);
  assert.match(container.innerHTML, /data-cad-mmwave-yaw/);
  assert.match(container.innerHTML, /data-cad-mmwave-raw-x/);
  assert.match(container.innerHTML, /120° Sichtfeld/);
});

test('CAD view renders optional floor markers for every TX/RX calibration standpoint', () => {
  const container = { innerHTML: '' };
  const editor = new RoomGeometryEditor(container, { document: defaultSetupProfileDocument() });

  editor.render();

  assert.match(container.innerHTML, /·K = Kalibrierstandpunkt/);
  assert.match(container.innerHTML, /data-calibration-id="RX1_CALIBRATION"/);
  assert.match(container.innerHTML, /occ-cad-marker-calibration/);
  assert.match(container.innerHTML, /is-fallback/);
});

test('CAD calibration floor markers keep explicit positions separate from device positions', () => {
  const profile = defaultSetupProfileDocument();
  profile.receivers[0].calibration_position_m = [1.25, 0, 2.75];
  const container = { innerHTML: '' };
  const editor = new RoomGeometryEditor(container, { document: profile, selectedIds: ['RX1_CALIBRATION'] });

  editor.render();

  const marker = container.innerHTML.match(/<g class="[^"]*occ-cad-marker-calibration[^"]*"\s+data-calibration-handle data-calibration-id="RX1_CALIBRATION"[^>]*>/)?.[0];
  assert.ok(marker);
  assert.doesNotMatch(marker, /is-fallback/);
  assert.match(container.innerHTML, /RX1 · optionaler Ground Truth/);
  assert.match(container.innerHTML, /Geräteposition wieder als Fallback verwenden/);
  assert.match(container.innerHTML, /value="1\.25"/);
});

test('CAD coordinate editing writes a calibration position without moving the receiver', () => {
  const profile = defaultSetupProfileDocument();
  const container = { innerHTML: '' };
  let changed = null;
  const editor = new RoomGeometryEditor(container, {
    document: profile,
    selectedIds: ['RX1_CALIBRATION'],
    onChange: (document) => { changed = document; },
  });

  editor._handleFormChange({
    target: {
      value: '1.23',
      closest(selector) {
        return selector === '[data-cad-coordinate]'
          ? { dataset: { cadCoordinate: 'RX1_CALIBRATION.0' } }
          : null;
      },
    },
  });

  assert.deepEqual(changed.receivers[0].position_m, profile.receivers[0].position_m);
  assert.deepEqual(changed.receivers[0].calibration_position_m, [1.23, 0.5, 0.28]);
});

test('CAD view shows the setup-bound calibrated yaw without overwriting the base angle', () => {
  const profile = defaultSetupProfileDocument();
  profile.mmwave.yaw_mdeg = 217400;
  const container = { innerHTML: '' };
  const editor = new RoomGeometryEditor(container, {
    document: profile,
    selectedIds: ['MMWAVE'],
    yawCalibration: {
      base_yaw_mdeg: 217400,
      optimized_yaw_mdeg: 223400,
      correction_mdeg: 6000,
      point_count: 5,
    },
  });

  editor.render();

  assert.match(container.innerHTML, /Basis-Yaw/);
  assert.match(container.innerHTML, /Kalibriert wirksam: 223\.4°/);
  assert.match(container.innerHTML, /Korrektur 6\.0° aus 5 RX\/TX-Punkten/);
  assert.equal(editor.document.mmwave.yaw_mdeg, 217400);
});

test('CAD view exposes one save action for the complete current geometry', () => {
  const container = { innerHTML: '' };
  let savedDocument = null;
  const editor = new RoomGeometryEditor(container, {
    document: defaultSetupProfileDocument(),
    onSave: (document) => { savedDocument = document; },
  });

  editor.render();

  assert.match(container.innerHTML, /data-cad-action="save-positions"/);
  assert.match(container.innerHTML, />Geometrie speichern</);
  editor._handleClick({
    preventDefault() {},
    target: {
      closest(selector) {
        return selector === '[data-cad-action]'
          ? { dataset: { cadAction: 'save-positions' } }
          : null;
      },
    },
  });

  assert.deepEqual(savedDocument.transmitter.position_m, [1.51, 1.19, 0.39]);
  assert.deepEqual(savedDocument.receivers.map((receiver) => receiver.position_m), [
    [0.00, 0.50, 0.28],
    [4.02, 0.87, 0.97],
    [0.00, 0.74, 2.11],
    [4.02, 0.87, 2.46],
  ]);
  assert.deepEqual(savedDocument.mmwave.mounting_position_m, [0.0, 1.2, 1.72]);
});

test('mmWave angle helpers use the CAD x/z convention', () => {
  const profile = defaultSetupProfileDocument();
  profile.mmwave.mounting_position_m = [3.95, 1.5, 3.3];

  assert.equal(mmwaveYawToReceiverMdeg(profile), 217400);
  assert.equal(mmwaveYawToPointMdeg(profile, [4.95, 1.5, 3.3]), 0);
  assert.equal(mmwaveYawToPointMdeg(profile, [3.95, 1.5, 4.3]), 90000);
  assert.equal(mmwaveYawToPointMdeg(profile, [2.95, 1.5, 3.3]), 180000);
  assert.equal(mmwaveYawToPointMdeg(profile, [3.95, 1.5, 2.3]), 270000);
});

test('CAD orientation inspector applies exact yaw, RX1 aim, and raw-X mirror', () => {
  const container = { innerHTML: '' };
  let changedDocument = null;
  const editor = new RoomGeometryEditor(container, {
    document: defaultSetupProfileDocument(),
    onChange: (document) => { changedDocument = document; },
  });
  editor._select('MMWAVE');

  editor._handleFormChange({
    target: {
      value: '123.4',
      closest(selector) { return selector === '[data-cad-mmwave-yaw]' ? this : null; },
    },
  });
  assert.equal(mmwaveYawMdeg(changedDocument), 123400);
  assert.match(container.innerHTML, /data-cad-mmwave-yaw value="123\.4"/);

  editor._handleClick({
    preventDefault() {},
    target: {
      closest(selector) {
        return selector === '[data-cad-action]'
          ? { dataset: { cadAction: 'aim-mmwave-rx1' } }
          : null;
      },
    },
  });
  assert.equal(changedDocument.mmwave.yaw_mdeg, 270000);

  editor._handleFormChange({
    target: {
      checked: true,
      closest(selector) { return selector === '[data-cad-mmwave-raw-x]' ? this : null; },
    },
  });
  assert.equal(changedDocument.mmwave.raw_x_inverted, true);
  assert.match(container.innerHTML, /data-cad-mmwave-raw-x checked/);
});

test('yaw handle supports direct pointer adjustment and keyboard steps', () => {
  const container = { innerHTML: '', querySelector() { return null; } };
  let changedDocument = null;
  const editor = new RoomGeometryEditor(container, {
    document: defaultSetupProfileDocument(),
    onChange: (document) => { changedDocument = document; },
    selectedIds: ['MMWAVE'],
  });
  editor._svgPoint = () => ({ x: 162, y: 400 });
  editor.drag = { kind: 'yaw', pointerId: 7 };

  editor._handlePointerMove({ pointerId: 7 });
  editor._handlePointerUp({ pointerId: 7 });

  assert.notEqual(changedDocument.mmwave.yaw_mdeg, 0);
  const afterPointer = changedDocument.mmwave.yaw_mdeg;

  editor._handleKeyDown({
    key: 'ArrowRight',
    shiftKey: false,
    preventDefault() {},
    target: {
      closest(selector) { return selector === '[data-cad-yaw-handle]' ? {} : null; },
    },
  });
  assert.equal(changedDocument.mmwave.yaw_mdeg, (afterPointer + 1000) % 360000);
});

test('position drags publish the current MMWAVE2 draft before pointerup', () => {
  const profile = defaultSetupProfileDocument();
  const container = { innerHTML: '', querySelector() { return null; } };
  let changedDocument = null;
  const editor = new RoomGeometryEditor(container, {
    document: profile,
    onChange: (document) => { changedDocument = document; },
  });
  editor._svgPoint = () => ({ x: 360, y: 220 });
  editor.drag = { kind: 'position', id: 'MMWAVE2', pointerId: 8 };

  const previousCSS = globalThis.CSS;
  globalThis.CSS = { escape: (value) => value };
  try {
    editor._handlePointerMove({ pointerId: 8 });
  } finally {
    globalThis.CSS = previousCSS;
  }

  assert.ok(changedDocument);
  assert.notDeepEqual(
    changedDocument.mmwave_sensors.find((sensor) => sensor.node_id === 'MMWAVE2').mounting_position_m,
    profile.mmwave_sensors.find((sensor) => sensor.node_id === 'MMWAVE2').mounting_position_m,
  );
});

test('CAD view exposes and applies the mmWave placement calculation', () => {
  const container = { innerHTML: '' };
  let changedDocument = null;
  const editor = new RoomGeometryEditor(container, {
    document: defaultSetupProfileDocument(),
    onChange: (document) => { changedDocument = document; },
  });

  editor.render();
  assert.match(container.innerHTML, /data-cad-action="calculate-mmwave-placement"/);
  assert.match(container.innerHTML, />mmWave-Position berechnen</);

  editor._handleClick({
    preventDefault() {},
    target: {
      closest(selector) {
        return selector === '[data-cad-action]'
          ? { dataset: { cadAction: 'calculate-mmwave-placement' } }
          : null;
      },
    },
  });

  assert.deepEqual(changedDocument.mmwave.mounting_position_m, [4.02, 1.2, 0]);
  assert.equal(changedDocument.mmwave.yaw_mdeg, 135000);
  assert.match(container.innerHTML, /100% geometrische 2D-Abdeckung/);
  assert.match(container.innerHTML, /yaw 135\.00°/);
  assert.match(container.innerHTML, /TX\/RX 5\/5 im Sichtfeld/);
});

test('public mmWave placement action returns the calculation result for outer setup controls', () => {
  const container = { innerHTML: '' };
  const editor = new RoomGeometryEditor(container, { document: defaultSetupProfileDocument() });

  const result = editor.calculateOptimalMmwavePlacement();

  assert.equal(result.ok, true);
  assert.deepEqual(result.positionM, [4.02, 1.2, 0]);
  assert.deepEqual(editor.document.mmwave.mounting_position_m, [4.02, 1.2, 0]);
  assert.equal(editor.document.mmwave.yaw_mdeg, 135000);
});

test('CAD view exposes and applies the mmWave interior-only setting', () => {
  const container = { innerHTML: '' };
  let changedDocument = null;
  const editor = new RoomGeometryEditor(container, {
    document: defaultSetupProfileDocument(),
    onChange: (document) => { changedDocument = document; },
  });

  editor.render();
  assert.match(container.innerHTML, /data-cad-mmwave-exterior/);
  assert.match(container.innerHTML, /mmWave darf außerhalb des Raums montiert werden/);
  assert.match(container.innerHTML, /checked/);

  editor._handleFormChange({
    target: {
      checked: false,
      closest(selector) {
        return selector === '[data-cad-mmwave-exterior]' ? this : null;
      },
    },
  });

  assert.equal(changedDocument.mmwave.allow_exterior, false);
  assert.match(container.innerHTML, /Innenraum-only/);
  assert.doesNotMatch(container.innerHTML, /data-cad-mmwave-exterior checked/);
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

test('CAD geometry validation rejects an explicit calibration standpoint outside the room', () => {
  const profile = defaultSetupProfileDocument();
  profile.transmitter.calibration_position_m = [profile.room_dimensions_m[0] + 0.01, 0, 1];

  const validation = validateGeometryDraft(profile);

  assert.equal(validation.valid, false);
  assert.match(validation.errors.join(' '), /TX Kalibrierstandpunkt: Position liegt außerhalb des Raums/);
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

test('CAD geometry validates the mmWave mount with the same room and exterior-radius rules', () => {
  const profile = defaultSetupProfileDocument();
  profile.mmwave.mounting_position_m[0] = -0.25;

  assert.equal(validateGeometryDraft(profile).valid, true);

  profile.mmwave.mounting_position_m[0] = -0.51;
  const invalid = validateGeometryDraft(profile);
  assert.equal(invalid.valid, false);
  assert.match(invalid.errors.join(' '), /MMWAVE: Außenradius von 0\.50 m überschritten/);
});

test('CAD geometry rejects an exterior mmWave mount in interior-only mode', () => {
  const profile = defaultSetupProfileDocument();
  profile.mmwave.allow_exterior = false;
  profile.transmitter.position_m[0] = -0.25;

  assert.equal(validateGeometryDraft(profile).valid, true);

  profile.mmwave.mounting_position_m[0] = -0.01;

  const validation = validateGeometryDraft(profile);

  assert.equal(validation.valid, false);
  assert.match(validation.errors.join(' '), /MMWAVE: Montage ist auf den Innenraum beschränkt/);
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

test('clicking the free CAD viewport outside the room clears the selection', () => {
  const container = { innerHTML: '' };
  const editor = new RoomGeometryEditor(container, { document: defaultSetupProfileDocument() });

  editor._select('RX1');
  editor._select('TX', true);
  editor._handleClick({
    target: {
      closest(selector) {
        return selector === '[data-cad-viewport]' ? {} : null;
      },
    },
  });

  assert.deepEqual(editor.selectedIds, []);
  assert.equal(editor.selectedId, null);
  assert.match(container.innerHTML, /Keine Auswahl/);
  assert.doesNotMatch(container.innerHTML, /data-cad-selection-line/);
  assert.match(container.innerHTML, /data-cad-viewport/);
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

test('plan distance moves the mmWave mount and preserves its height', () => {
  const profile = defaultSetupProfileDocument();
  const originalY = profile.mmwave.mounting_position_m[1];
  const result = setPlanDistance(profile, 'TX', 'MMWAVE', 1.25);

  assert.equal(result.error, '');
  assert.ok(Math.abs(planDistance(result.document.transmitter.position_m, result.document.mmwave.mounting_position_m) - 1.25) < 1e-9);
  assert.equal(result.document.mmwave.mounting_position_m[1], originalY);
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

test('opposite X walls expose and apply the room-length distance', () => {
  const profile = defaultSetupProfileDocument();
  const result = setWallDistance(profile, 'WALL_X0', 'WALL_XMAX', 5.00);

  assert.equal(result.error, '');
  assert.deepEqual(result.document.room_dimensions_m, [5.00, 2.59, 3.44]);
  assert.equal(wallPairDistance(result.document, 'WALL_XMAX', 'WALL_X0'), 5.00);
  assert.deepEqual(result.document.receivers[1].position_m, profile.receivers[1].position_m);
});

test('opposite Z walls apply the room-width distance independent of selection order', () => {
  const profile = defaultSetupProfileDocument();
  const result = setWallDistance(profile, 'WALL_ZMAX', 'WALL_Z0', 4.10);

  assert.equal(result.error, '');
  assert.deepEqual(result.document.room_dimensions_m, [4.02, 2.59, 4.10]);
  assert.equal(wallPairDistance(result.document, 'WALL_Z0', 'WALL_ZMAX'), 4.10);
});

test('CAD inspector offers a room-distance input for opposite walls', () => {
  const container = { innerHTML: '' };
  const editor = new RoomGeometryEditor(container, {
    document: defaultSetupProfileDocument(),
    selectedIds: ['WALL_X0', 'WALL_XMAX'],
  });

  editor.render();

  assert.match(container.innerHTML, /Abstand zwischen Wänden/);
  assert.match(container.innerHTML, /Raumlänge L \(m\)/);
  assert.match(container.innerHTML, /Ändert Raumlänge L/);
  assert.match(container.innerHTML, /data-cad-selection-line/);
});

test('CAD inspector names the Z wall pair by its room-width meaning', () => {
  const container = { innerHTML: '' };
  const editor = new RoomGeometryEditor(container, {
    document: defaultSetupProfileDocument(),
    selectedIds: ['WALL_Z0', 'WALL_ZMAX'],
  });

  editor.render();

  assert.match(container.innerHTML, /Wand oben \(Z = 0\)/);
  assert.match(container.innerHTML, /Wand unten \(Z = B\)/);
  assert.match(container.innerHTML, /Abstand = Raumbreite B/);
  assert.match(container.innerHTML, /Ändert Raumbreite B/);
});

test('setting the selected opposite walls updates the editor room dimension', () => {
  const profile = defaultSetupProfileDocument();
  const container = {
    innerHTML: '',
    querySelector(selector) {
      return selector === '[data-cad-distance-input]' ? { value: '5.00' } : null;
    },
  };
  const editor = new RoomGeometryEditor(container, {
    document: profile,
    selectedIds: ['WALL_X0', 'WALL_XMAX'],
  });

  editor._setSelectedDistance();

  assert.equal(editor.document.room_dimensions_m[0], 5.00);
  assert.equal(editor.document.room_dimensions_m[2], profile.room_dimensions_m[2]);
});

test('wall distance rejects adjacent walls and a shrink that would invalidate a marker', () => {
  const profile = defaultSetupProfileDocument();
  const adjacent = setWallDistance(profile, 'WALL_X0', 'WALL_Z0', 2.00);
  assert.match(adjacent.error, /gegenüberliegende Wände/);

  const shrink = setWallDistance(profile, 'WALL_X0', 'WALL_XMAX', 2.00);
  assert.match(shrink.error, /Geometrie ungültig/);
  assert.deepEqual(shrink.document.room_dimensions_m, profile.room_dimensions_m);
});

import assert from 'node:assert/strict';
import test from 'node:test';

import { RoomGeometryEditor, mmwaveYawMdeg } from '../components/RoomGeometryEditor.js';
import { defaultSetupProfileDocument } from '../components/ObservatoryControlCenter.js';

function makeEditor(options = {}) {
  const input = { value: '', setCustomValidity() {}, reportValidity() {} };
  const host = { innerHTML: '', querySelector: (selector) => selector === '[data-cad-wall-angle-input]' ? input : null };
  const editor = new RoomGeometryEditor(host, { document: defaultSetupProfileDocument(), selectedIds: ['MMWAVE1'], ...options });
  editor.render();
  return { editor, host, input };
}

function wallClick(editor, wallId) {
  editor._handleClick({ shiftKey: true, target: {
    closest: (selector) => selector === '[data-wall-handle]' ? { dataset: { wallId } } : null,
  } });
}

function setAngle(editor, input, value) {
  input.value = String(value);
  editor._handleClick({ preventDefault() {}, target: {
    closest: (selector) => selector === '[data-cad-action]' ? { dataset: { cadAction: 'set-wall-angle' } } : null,
  } });
}

function displayedAngle(host) {
  return Number(host.innerHTML.match(/data-cad-wall-angle-input[^>]+value="([^"]+)"/)[1]);
}

test('sensor then Shift-click on a wall exposes the angle and keeps the wall distance control', () => {
  const { editor, host } = makeEditor();
  assert.doesNotMatch(host.innerHTML, /data-cad-wall-angle-input/);
  wallClick(editor, 'WALL_X0');
  assert.deepEqual(editor.selectedIds, ['MMWAVE1', 'WALL_X0']);
  assert.match(host.innerHTML, /data-cad-wall-angle-arc/);
  assert.match(host.innerHTML, /data-cad-distance-input/);
  assert.match(host.innerHTML, /0° parallel, 90° senkrecht/);
  assert.doesNotMatch(host.innerHTML, /Nullrichtung|occ-cad-wall-angle-reference/);
  editor._clearSelection();
  assert.doesNotMatch(host.innerHTML, /data-cad-wall-angle-input/);
  editor._select('RX1');
  wallClick(editor, 'WALL_X0');
  assert.doesNotMatch(host.innerHTML, /data-cad-wall-angle-input/);
});

test('pointer selection survives the click retargeted by the SVG render and empty clicks still clear it', () => {
  const { editor, host } = makeEditor({ selectedIds: [] });
  const target = (selector, dataset) => ({ closest: (candidate) => candidate === selector ? { dataset } : null });
  const empty = target('[data-cad-svg]', {});
  editor._handlePointerDown({ button: 0, pointerId: 1, preventDefault() {}, target: target('[data-geometry-handle]', { geometryId: 'MMWAVE2' }) });
  editor._handlePointerUp({ pointerId: 1 });
  editor._handleClick({ detail: 1, target: empty });
  assert.deepEqual(editor.selectedIds, ['MMWAVE2']);
  editor._handlePointerDown({ button: 0, shiftKey: true, preventDefault() {}, target: target('[data-wall-handle]', { wallId: 'WALL_XMAX' }) });
  editor._handleClick({ detail: 1, target: empty });
  assert.deepEqual(editor.selectedIds, ['MMWAVE2', 'WALL_XMAX']);
  assert.match(host.innerHTML, /data-cad-wall-angle-input/);
  editor._handlePointerDown({ button: 0, pointerId: 2, preventDefault() {}, target: target('[data-cad-yaw-handle]', { cadYawNode: 'MMWAVE2' }) });
  editor._handlePointerUp({ pointerId: 2 });
  editor._handleClick({ detail: 1, target: empty });
  assert.deepEqual(editor.selectedIds, ['MMWAVE2', 'WALL_XMAX']);
  editor._handlePointerDown({ button: 0, target: empty });
  editor._handleClick({ detail: 1, target: empty });
  assert.deepEqual(editor.selectedIds, []);
});

test('acute wall angles preserve the beam quadrant for either sensor and every wall', () => {
  const references = { WALL_X0: 270, WALL_XMAX: 90, WALL_Z0: 0, WALL_ZMAX: 180 };
  for (const nodeId of ['MMWAVE1', 'MMWAVE2']) {
    for (const [wallId, reference] of Object.entries(references)) {
      for (const quadrant of [0, 1, 2, 3]) {
        for (const angle of [0, 30.125, 45.123, 90]) {
          let changed;
          const profile = defaultSetupProfileDocument();
          const initialYaw = ((reference + quadrant * 90 + 45) % 360) * 1000;
          profile.mmwave_sensors.find((sensor) => sensor.node_id === nodeId).yaw_mdeg = initialYaw;
          if (nodeId === 'MMWAVE1') profile.mmwave.yaw_mdeg = initialYaw;
          const { editor, host, input } = makeEditor({ document: profile, selectedIds: [nodeId], onChange: (draft) => { changed = draft; } });
          wallClick(editor, wallId);
          const expected = structuredClone(profile);
          const relativeYaw = [angle, 180 - angle, 180 + angle, 360 - angle][quadrant];
          const yawMdeg = Math.round((reference + relativeYaw) * 1000) % 360000;
          expected.mmwave_sensors.find((sensor) => sensor.node_id === nodeId).yaw_mdeg = yawMdeg;
          if (nodeId === 'MMWAVE1') expected.mmwave.yaw_mdeg = yawMdeg;
          setAngle(editor, input, angle);
          assert.deepEqual(changed, expected);
          assert.equal(displayedAngle(host), angle);
          assert.deepEqual(editor.selectedIds, [nodeId, wallId]);
        }
      }
    }
  }
});

test('invalid and empty wall angles report an error without changing geometry', () => {
  let changes = 0;
  const { editor, input } = makeEditor({ onChange: () => { changes += 1; } });
  wallClick(editor, 'WALL_Z0');
  const before = structuredClone(editor.document);
  const errors = [];
  input.setCustomValidity = (message) => { if (message) errors.push(message); };
  for (const value of ['', ' ', 'abc', 'Infinity', '-0.001', '90.001']) setAngle(editor, input, value);
  assert.equal(errors.length, 6);
  assert.equal(changes, 0);
  assert.deepEqual(editor.document, before);
});

test('wall angle reflects the displayed calibration and setting it replaces the correction', () => {
  const profile = defaultSetupProfileDocument();
  profile.mmwave_sensors[1].yaw_mdeg = 180000;
  const { editor, host, input } = makeEditor({ document: profile, selectedIds: ['MMWAVE2'],
    yawCalibration: { node_id: 'MMWAVE2', base_yaw_mdeg: 180000, optimized_yaw_mdeg: 175125 },
  });
  wallClick(editor, 'WALL_XMAX');
  assert.equal(displayedAngle(host), 85.125);
  setAngle(editor, input, 90);
  assert.equal(mmwaveYawMdeg(editor.document, 'MMWAVE2'), 180000);
  assert.equal(editor.yawCalibration, null);
  assert.equal(displayedAngle(host), 90);
});

test('the arc is centred on the actual wall/beam intersection and its ends touch both lines', () => {
  const document = defaultSetupProfileDocument();
  document.room_dimensions_m = [8, 2.59, 5];
  document.mmwave.mounting_position_m = [2, 1.2, 1];
  document.mmwave.yaw_mdeg = 30000;
  const { editor, host } = makeEditor({ document });
  wallClick(editor, 'WALL_XMAX');
  assert.equal(displayedAngle(host), 60);
  const arc = host.innerHTML.match(/data-cad-wall-angle-arc d="([^"]+)"/)[1];
  const points = [...arc.matchAll(/[ML] ([\d.-]+) ([\d.-]+)/g)];
  const profile = editor.document;
  const margin = profile.sensor_mount_radius_m;
  const mount = profile.mmwave.mounting_position_m;
  const intersection = [8, 1 + 6 * Math.tan(Math.PI / 6)];
  const worldPoints = points.map((point) => {
    const x = (Number(point[1]) - 92) / 700 * (profile.room_dimensions_m[0] + 2 * margin) - margin;
    const z = (Number(point[2]) - 62) / 420 * (profile.room_dimensions_m[2] + 2 * margin) - margin;
    assert.ok(Math.abs(Math.hypot(x - intersection[0], z - intersection[1]) - 0.55) < 0.001);
    return [x, z];
  });
  assert.ok(Math.abs(worldPoints[0][0] - 8) < 0.001, 'one endpoint lies on the selected wall');
  const beamEnd = worldPoints.at(-1);
  assert.ok(Math.abs((beamEnd[1] - mount[2]) - (beamEnd[0] - mount[0]) * Math.tan(Math.PI / 6)) < 0.001,
    'the other endpoint lies on the beam axis');
  assert.match(host.innerHTML, /occ-cad-wall-angle-extension" d="M/);
  assert.doesNotMatch(host.innerHTML, /data-cad-selection-line/);
});

test('changing the selected wall measures that wall instead of the global yaw', () => {
  const document = defaultSetupProfileDocument();
  document.mmwave.mounting_position_m = [2, 1.2, 0.4];
  document.mmwave.yaw_mdeg = 15000;
  const { editor, host } = makeEditor({ document });
  wallClick(editor, 'WALL_XMAX');
  assert.equal(displayedAngle(host), 75);
  wallClick(editor, 'WALL_Z0');
  assert.equal(displayedAngle(host), 15);
  assert.deepEqual(editor.document, document);
  assert.match(host.innerHTML, /data-cad-wall-angle-arc/);
  assert.match(host.innerHTML, /occ-cad-wall-angle-extension" d="M/);
});

test('parallel and off-screen intersections do not produce a fictitious angle arc', () => {
  const { editor, host, input } = makeEditor();
  wallClick(editor, 'WALL_Z0');
  assert.equal(displayedAngle(host), 0);
  assert.doesNotMatch(host.innerHTML, /data-cad-wall-angle-arc/);
  assert.match(host.innerHTML, /keinen eindeutigen Schnittpunkt/);
  setAngle(editor, input, 0.001);
  assert.equal(displayedAngle(host), 0.001);
  assert.doesNotMatch(host.innerHTML, /data-cad-wall-angle-arc/);
  assert.match(host.innerHTML, /außerhalb der CAD-Ansicht/);
  assert.doesNotMatch(host.innerHTML, /NaN|Infinity/);
});

test('Enter applies the wall angle without submitting the surrounding profile form', () => {
  const { editor, input } = makeEditor();
  wallClick(editor, 'WALL_ZMAX');
  input.value = '90';
  let prevented = false;
  editor._handleKeyDown({ key: 'Enter', preventDefault() { prevented = true; }, target: {
    closest: (selector) => selector === '[data-cad-wall-angle-input]' ? input : null,
  } });
  assert.ok(prevented);
  assert.equal(mmwaveYawMdeg(editor.document), 270000);
});

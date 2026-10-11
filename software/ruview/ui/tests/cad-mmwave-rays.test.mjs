import assert from 'node:assert/strict';
import test from 'node:test';

import { RoomGeometryEditor, mmwaveMountingPosition, mmwaveYawMdeg } from '../components/RoomGeometryEditor.js';
import { ObservatoryControlCenter, defaultSetupProfileDocument } from '../components/ObservatoryControlCenter.js';

function profileWithMounts(first, second) {
  const profile = defaultSetupProfileDocument();
  profile.mmwave_sensors[0].mounting_position_m = first;
  profile.mmwave_sensors[1].mounting_position_m = second;
  profile.mmwave = { ...profile.mmwave_sensors[0] };
  return profile;
}

function makeEditor(document = defaultSetupProfileDocument(), options = {}) {
  const host = { innerHTML: '', querySelector: () => null, addEventListener() {}, removeEventListener() {} };
  const editor = new RoomGeometryEditor(host, { document, selectedIds: ['MMWAVE1'], ...options });
  editor.render();
  return { editor, host };
}

function align(editor) {
  editor._handleClick({ preventDefault() {}, target: { closest: () => ({ dataset: { cadAction: 'aim-mmwave-pair' } }) } });
}

function element(markup, attribute, id) {
  return markup.match(new RegExp(`<[^>]+${attribute}="${id}"[^>]*>`))?.[0];
}

function attribute(markup, name) {
  return markup.match(new RegExp(`(?:^| )${name}="([^"]*)"`))?.[1];
}

function svgToMetres(x, y, profile) {
  const margin = profile.sensor_mount_radius_m;
  return [(x - 92) / 700 * (profile.room_dimensions_m[0] + 2 * margin) - margin,
    (y - 62) / 420 * (profile.room_dimensions_m[2] + 2 * margin) - margin];
}

test('pair alignment changes only yaw and faces the other mount from either side', () => {
  for (const [first, second] of [
    [[0, 1.2, 1.72], [4.02, 1.2, 1.72]],
    [[0.3, 1.1, 0.5], [3.7, 1.4, 2.8]],
    [[3.7, 1.1, 2.8], [0.3, 1.4, 0.5]],
    [[2, 1.2, 0.5], [2, 1.2, 2.8]],
  ]) {
    const profile = profileWithMounts(first, second);
    let savedDraft;
    const { editor, host } = makeEditor(profile, { onChange: (draft) => { savedDraft = draft; } });
    align(editor);
    const firstYaw = mmwaveYawMdeg(savedDraft, 'MMWAVE1');
    const secondYaw = mmwaveYawMdeg(savedDraft, 'MMWAVE2');
    assert.equal((secondYaw - firstYaw + 360000) % 360000, 180000);
    for (const [id, origin, target] of [['MMWAVE1', first, second], ['MMWAVE2', second, first]]) {
      const angle = mmwaveYawMdeg(savedDraft, id) / 1000 * Math.PI / 180;
      const dx = target[0] - origin[0];
      const dz = target[2] - origin[2];
      assert.ok(Math.cos(angle) * dx + Math.sin(angle) * dz > 0);
      assert.ok(Math.abs(Math.cos(angle) * dz - Math.sin(angle) * dx) < 0.0001);
    }
    const expected = structuredClone(profile);
    expected.mmwave.yaw_mdeg = firstYaw;
    expected.mmwave_sensors[0].yaw_mdeg = firstYaw;
    expected.mmwave_sensors[1].yaw_mdeg = secondYaw;
    assert.deepEqual(savedDraft, expected);
    assert.deepEqual(editor.selectedIds, ['MMWAVE1']);
    assert.doesNotMatch(host.innerHTML, /data-cad-selection-line=/);
  }
});

test('pair alignment rejects missing, coincident and malformed mounts without changing the draft', () => {
  for (const mutate of [
    (profile) => { profile.mmwave_sensors.pop(); },
    (profile) => { profile.mmwave_sensors[1].mounting_position_m = [...profile.mmwave.mounting_position_m]; },
    (profile) => { profile.mmwave_sensors[1].mounting_position_m = [NaN, 1, 2]; },
    (profile) => { profile.mmwave_sensors[1].mounting_position_m = [null, 1, 2]; },
  ]) {
    const profile = defaultSetupProfileDocument();
    mutate(profile);
    let changes = 0;
    const { editor, host } = makeEditor(profile, { onChange: () => { changes += 1; } });
    align(editor);
    assert.deepEqual(editor.document, profile);
    assert.equal(changes, 0);
    assert.match(host.innerHTML, /verschiedenen gültigen x\/z-Positionen/);
  }
});

test('both sectors and centre rays stop at six metres at every bearing', () => {
  const profile = profileWithMounts([2, 1.2, 4], [11, 1.2, 4]);
  profile.room_dimensions_m = [14, 2.59, 10];
  for (const yaw of [0, 37125, 90000, 180000, 270000, 359999]) {
    profile.mmwave.yaw_mdeg = yaw;
    profile.mmwave_sensors.forEach((sensor) => { sensor.yaw_mdeg = yaw; });
    const { host } = makeEditor(profile);
    for (const id of ['MMWAVE1', 'MMWAVE2']) {
      const mount = mmwaveMountingPosition(profile, id);
      const distance = (x, y) => {
        const world = svgToMetres(x, y, profile);
        return Math.hypot(world[0] - mount[0], world[1] - mount[2]);
      };
      const ray = element(host.innerHTML, 'data-cad-mmwave-centerline', id);
      assert.ok(Math.abs(distance(Number(attribute(ray, 'x2')), Number(attribute(ray, 'y2'))) - 6) < 0.001);
      const sector = attribute(element(host.innerHTML, 'data-cad-mmwave-fov', id), 'd');
      const vertices = [...sector.matchAll(/L ([\d.-]+) ([\d.-]+)/g)];
      assert.ok(vertices.length > 2);
      vertices.forEach((vertex) => assert.ok(Math.abs(distance(Number(vertex[1]), Number(vertex[2])) - 6) < 0.001));
      const arrow = attribute(element(host.innerHTML, 'data-cad-mmwave-direction', id), 'd');
      const ends = [...arrow.matchAll(/[ML] ([\d.-]+) ([\d.-]+)/g)];
      assert.equal(ends.length, 2, 'a half arrow has exactly one segment');
      ends.forEach((end) => assert.ok(distance(Number(end[1]), Number(end[2])) < 6));
    }
    assert.match(host.innerHTML, /max\. 6 m/);
  }
});

test('opposed half-arrowheads occupy opposite sides and use distinct sensor colours', () => {
  const { editor, host } = makeEditor();
  align(editor);
  const colors = [];
  const sides = [];
  for (const id of ['MMWAVE1', 'MMWAVE2']) {
    const view = element(host.innerHTML, 'data-cad-mmwave-view', id);
    const color = attribute(view, 'style');
    colors.push(color);
    assert.match(element(host.innerHTML, 'data-geometry-id', id), new RegExp(color));
    const arrow = attribute(element(host.innerHTML, 'data-cad-mmwave-direction', id), 'd');
    const ends = [...arrow.matchAll(/[ML] ([\d.-]+) ([\d.-]+)/g)];
    sides.push(Number(ends[0][2]) - Number(ends[1][2]));
  }
  assert.notEqual(colors[0], colors[1]);
  assert.ok(sides[0] * sides[1] < 0);
});

test('manual pair alignment overrides a previous calibrated yaw even when the base angle stays the same', () => {
  const profile = defaultSetupProfileDocument();
  profile.mmwave.yaw_mdeg = 0;
  const { editor, host } = makeEditor(profile, {
    yawCalibration: { node_id: 'MMWAVE1', base_yaw_mdeg: 0, optimized_yaw_mdeg: 3000 },
  });
  align(editor);
  assert.equal(editor.yawCalibration, null);
  for (const id of ['MMWAVE1', 'MMWAVE2']) {
    const ray = element(host.innerHTML, 'data-cad-mmwave-centerline', id);
    assert.equal(attribute(ray, 'y1'), attribute(ray, 'y2'));
  }
});

test('legacy yaw correction belongs only to MMWAVE1', () => {
  const profile = defaultSetupProfileDocument();
  profile.mmwave_sensors.forEach((sensor) => { sensor.yaw_mdeg = 0; });
  profile.mmwave.yaw_mdeg = 0;
  const { host } = makeEditor(profile, { yawCalibration: { base_yaw_mdeg: 0, optimized_yaw_mdeg: 45000 } });
  const secondRay = element(host.innerHTML, 'data-cad-mmwave-centerline', 'MMWAVE2');
  assert.equal(attribute(secondRay, 'y1'), attribute(secondRay, 'y2'));
});

test('live orientation updates move the half arrow and only the selected sensor handle', () => {
  const { editor, host } = makeEditor();
  const changed = new Map();
  host.querySelector = (selector) => ({
    setAttribute(name, value) {
      if (!changed.has(selector)) changed.set(selector, {});
      changed.get(selector)[name] = value;
    },
  });
  editor.document.mmwave_sensors[1].yaw_mdeg = 90000;
  editor._updateMmwaveOrientation('MMWAVE2');
  editor.render();
  const arrow = element(host.innerHTML, 'data-cad-mmwave-direction', 'MMWAVE2');
  assert.equal(changed.get('[data-cad-mmwave-direction="MMWAVE2"]').d, attribute(arrow, 'd'));
  assert.equal(changed.get('[data-cad-yaw-handle][data-cad-yaw-node="MMWAVE2"]')['aria-valuenow'], '90.000');
  assert.ok([...changed.keys()].every((selector) => selector.includes('MMWAVE2')));
});

test('control center does not restore old yaw corrections over an aligned draft or another saved profile', () => {
  const { host } = makeEditor();
  const center = new ObservatoryControlCenter({ innerHTML: '', querySelector: (selector) => selector === '#occRoomCadEditor' ? host : null });
  center._mounted = true;
  center.selectedProfile = { profile_sha256: 'profile-1' };
  center.status = { mmwave: {
    sealed_cad: { profile_sha256: 'profile-1' },
    yaw_calibration: { node_id: 'MMWAVE1', base_yaw_mdeg: 0, optimized_yaw_mdeg: 3000 },
  } };
  center.profileDraft.mmwave.yaw_mdeg = 0;
  center._render();
  assert.equal(center.geometryEditor.yawCalibration.optimized_yaw_mdeg, 3000);
  align(center.geometryEditor);
  center._render();
  assert.equal(center.geometryEditor.yawCalibration, null);
  assert.equal(mmwaveYawMdeg(center.profileDraft, 'MMWAVE2'), 180000);
  center.profileDraftDirty = false;
  center.selectedProfile.profile_sha256 = 'profile-2';
  center._render();
  assert.equal(center.geometryEditor.yawCalibration, null);
});

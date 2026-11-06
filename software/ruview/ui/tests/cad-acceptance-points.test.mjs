import assert from 'node:assert/strict';
import { setImmediate } from 'node:timers/promises';
import test from 'node:test';

import { ObservatoryControlCenter, defaultSetupProfileDocument } from '../components/ObservatoryControlCenter.js';
import { RoomGeometryEditor } from '../components/RoomGeometryEditor.js';

test('CAD measures A01 from two walls and reloads the full profile through the real services', async (t) => {
  // Only the HTTP boundary is synthetic; editor, save, service and JSON encoding run unchanged.
  let stored = {
    id: 'synthetic-room', label: 'Synthetic room', version: 1,
    revision_id: 'synthetic-room-v1', profile_sha256: '1'.repeat(64),
    document: defaultSetupProfileDocument(),
  };
  const originalMount = structuredClone(stored.document.receivers[0].position_m);
  const referencePoints = structuredClone(stored.document.points);
  const requests = [];
  const unexpectedRequests = [];
  t.after(() => assert.deepEqual(unexpectedRequests, []));
  t.mock.method(globalThis, 'fetch', async (url, options) => {
    const path = new URL(url).pathname;
    requests.push({ path, method: options.method });
    if (path === '/api/v1/experiments/setup-profiles/synthetic-room' && options.method === 'PUT') {
      const payload = JSON.parse(options.body);
      stored = {
        ...stored, ...payload, version: 2,
        revision_id: 'synthetic-room-v2', profile_sha256: '2'.repeat(64),
      };
      return Response.json(stored);
    }
    if (path === '/api/v1/experiments/setup-profiles' && options.method === 'GET') {
      return Response.json({ profiles: [stored] });
    }
    const responses = {
      '/api/v1/control-center/status': { nodes: [] },
      '/api/v1/experiments/runs': { runs: [] },
      '/api/v1/recording/list': { recordings: [] },
      '/api/v1/models': { models: [] },
      '/api/v1/benchmarks/catalog': {},
      '/api/v1/classification/calibration/availability': { available: false },
    };
    if (!Object.hasOwn(responses, path) || options.method !== 'GET') {
      unexpectedRequests.push({ path, method: options.method });
      throw new Error(`Unexpected request: ${options.method} ${path}`);
    }
    return Response.json(responses[path]);
  });

  const center = new ObservatoryControlCenter({ querySelector() { return null; } });
  center._selectProfile(structuredClone(stored), { forceDraft: true });
  const form = { querySelector() { return null; } };
  assert.equal(Object.hasOwn(center._readProfileFromForm(form), 'acceptance_points'), false);
  const host = {
    innerHTML: '',
    querySelector(selector) {
      if (selector === '.occ-cad-room') {
        return { getBoundingClientRect: () => ({ left: 10, top: 20, width: 402, height: 344 }) };
      }
      return null;
    },
  };
  const editor = new RoomGeometryEditor(host, {
    document: center.profileDraft,
    onChange: (document) => { center.profileDraft = document; },
    onSave: (document) => center._saveGeometryDocument(document),
  });
  const click = (selector, dataset, event = {}) => editor._handleClick({
    preventDefault() {},
    target: { closest: (candidate) => candidate === selector ? { dataset } : null },
    ...event,
  });
  const action = (cadAction) => click('[data-cad-action]', { cadAction });
  const marker = (shiftKey = false) => click('[data-geometry-handle]', { geometryId: 'A01' }, { shiftKey });
  const wall = (wallId, shiftKey = false) => click('[data-wall-handle]', { wallId }, { shiftKey });
  const distance = (value) => {
    editor._handleFormChange({ target: {
      closest: (selector) => selector === '[data-cad-distance-input]' ? { value } : null,
    } });
    action('set-distance');
  };
  editor.render();
  assert.match(host.innerHTML, /\+ Abnahmepunkt/);
  assert.doesNotMatch(host.innerHTML, /occ-cad-marker-acceptance/);
  action('add-acceptance-point');
  click('[data-cad-svg]', {}, { clientX: 9, clientY: 100 });
  assert.equal(editor.document.acceptance_points, undefined);
  click('[data-cad-svg]', {}, { clientX: 110, clientY: 120 });
  const [placed] = editor.document.acceptance_points;
  assert.equal(placed.id, 'A01');
  assert.ok(Math.abs(placed.coordinates_m[0] - 1) < 1e-9);
  assert.deepEqual(placed.coordinates_m.slice(1), [0, 1]);
  assert.match(host.innerHTML, /occ-cad-marker-acceptance[^"]*is-selected/);

  marker();
  wall('WALL_X0', true);
  assert.match(host.innerHTML, /Bleibt fest · Wand links \(X = 0\)/);
  assert.match(host.innerHTML, /Wird verschoben · A01/);
  assert.doesNotMatch(host.innerHTML, /Rollen tauschen/);
  distance('1.25');
  assert.deepEqual(editor.document.acceptance_points[0].coordinates_m, [1.25, 0, 1]);

  wall('WALL_ZMAX');
  marker(true);
  assert.match(host.innerHTML, /Bleibt fest · Wand unten \(Z = B\)/);
  assert.match(host.innerHTML, /Wird verschoben · A01/);
  distance('1.44');
  assert.deepEqual(editor.document.acceptance_points[0].coordinates_m, [1.25, 0, 2]);
  for (const [wall, meters] of Object.entries({ X0: '1.25', XMAX: '2.77', Z0: '2.00', ZMAX: '1.44' })) {
    assert.ok(host.innerHTML.includes(`data-cad-acceptance-distance="A01.WALL_${wall}">${meters} m`));
  }
  const measured = [{ id: 'A01', coordinates_m: [1.25, 0, 2] }];
  assert.deepEqual(center._readProfileFromForm(form).acceptance_points, measured);

  const invalid = structuredClone(editor.document);
  invalid.acceptance_points[0].coordinates_m[0] = 4.03;
  await center._saveProfile(null, invalid);
  assert.match(center.error, /A01.*innerhalb des Raums/);
  assert.equal(requests.length, 0, 'An invalid point must be rejected before the service call');

  action('save-positions');
  assert.equal(center.busy, true);
  for (let attempt = 0; center.busy && attempt < 20; attempt++) await setImmediate();
  assert.equal(center.busy, false);
  assert.equal(center.error, '');
  assert.equal(center.profileDraftDirty, false);
  assert.deepEqual(stored.document.receivers[0].position_m, originalMount);
  assert.deepEqual(stored.document.acceptance_points, measured);
  assert.deepEqual(stored.document.points, referencePoints);
  assert.equal(requests.filter((request) => request.method === 'PUT').length, 1);

  const reopened = new ObservatoryControlCenter({ innerHTML: '' });
  reopened._mounted = true;
  await reopened.refresh();
  assert.equal(reopened.error, '');
  assert.equal(reopened.selectedProfile.revision_id, 'synthetic-room-v2');
  assert.deepEqual(reopened.profileDraft.receivers[0].position_m, originalMount);
  assert.deepEqual(reopened.profileDraft.acceptance_points, measured);
  const reopenedEditor = new RoomGeometryEditor(host, { document: reopened.profileDraft, selectedIds: ['A01'] });
  reopenedEditor.render();
  assert.match(host.innerHTML, /A01 · Wandabstände/);
});

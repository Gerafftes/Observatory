import assert from 'node:assert/strict';

import {
  displayCoordinatesForRoom,
  GaussianSplatRenderer,
  isSimulatedSensingData,
  normalizePositionEstimate,
  positionEstimateViewModel,
  resolvedBodyPosition,
  resolveFieldGridGeometry,
} from '../components/gaussian-splats.js';
import { SensingTab } from '../components/SensingTab.js';

const room = [4.02, 2.59, 3.44];
const columns = 25;
const rows = 21;
const cellSizeX = room[0] / (columns - 1);
const cellSizeZ = room[2] / (rows - 1);
const values = Array.from({ length: columns * rows }, (_, index) => index);

assert.deepEqual(
  displayCoordinatesForRoom([0, 0.5, 0.28], room),
  [4.02, 0.5, 0.28],
  'RX1 must move only from the left edge to the right edge'
);
assert.deepEqual(
  displayCoordinatesForRoom([4.02, 0.87, 0.97], room),
  [0, 0.87, 0.97],
  'RX2 must move only from the right edge to the left edge'
);
assert.deepEqual(
  displayCoordinatesForRoom([0, 0.74, 2.11], room),
  [4.02, 0.74, 2.11],
  'mirroring x must preserve RX3 depth'
);
assert.deepEqual(
  displayCoordinatesForRoom([4.02, 0.87, 2.46], room),
  [0, 0.87, 2.46],
  'mirroring x must preserve RX4 depth'
);
assert.equal(displayCoordinatesForRoom([-0.01, 0, 0], room), null);

const probabilityGrid = resolveFieldGridGeometry(
  { grid_size: [20, 1, 20], values: [] },
  {
    status: 'coarse',
    probability_map: {
      columns,
      rows,
      origin: { x: 0, z: 0 },
      cell_size_x_m: cellSizeX,
      cell_size_z_m: cellSizeZ,
      values,
    },
  },
  room
);

assert.equal(probabilityGrid.columns, columns);
assert.equal(probabilityGrid.rows, rows);
assert.equal(probabilityGrid.originX, 0);
assert.equal(probabilityGrid.originZ, 0);
assert.equal(probabilityGrid.cellSizeX, cellSizeX);
assert.equal(probabilityGrid.cellSizeZ, cellSizeZ);
assert.equal(probabilityGrid.values, values);
assert.equal(probabilityGrid.isProbability, true);
assert.ok(
  Math.abs(
    probabilityGrid.originX +
      (probabilityGrid.columns - 1) * probabilityGrid.cellSizeX -
      room[0]
  ) < 1e-12
);
assert.ok(
  Math.abs(
    probabilityGrid.originZ +
      (probabilityGrid.rows - 1) * probabilityGrid.cellSizeZ -
      room[2]
  ) < 1e-12
);

const legacyGrid = resolveFieldGridGeometry(
  { grid_size: [20, 1, 20], values: [0.25] },
  { status: 'unavailable' },
  room
);

assert.equal(legacyGrid.columns, 20);
assert.equal(legacyGrid.rows, 20);
assert.equal(legacyGrid.originX, room[0] / 40);
assert.equal(legacyGrid.originZ, room[2] / 40);
assert.equal(legacyGrid.cellSizeX, room[0] / 20);
assert.equal(legacyGrid.cellSizeZ, room[2] / 20);
assert.equal(legacyGrid.isProbability, false);

const positionEstimate = normalizePositionEstimate({
  state: 'position',
  point_id: 'P05',
  coordinates_m: [2.01, 0, 1.72],
});
assert.equal(positionEstimate.state, 'position');
assert.equal(positionEstimate.pointId, 'P05');
assert.deepEqual(positionEstimate.coordinates, [2.01, 0, 1.72]);
assert.equal(positionEstimate.reason, null);

for (const state of [
  'unknown',
  'ambiguous',
  'insufficient',
  'uncalibrated',
  'stale',
]) {
  const estimate = normalizePositionEstimate({ state });
  assert.equal(estimate.state, state);
  assert.equal(estimate.pointId, null);
  assert.equal(estimate.coordinates, null);
  assert.equal(typeof estimate.reason, 'string');
  assert.ok(estimate.reason.length > 0);
}

assert.deepEqual(normalizePositionEstimate(null), {
  state: 'unknown',
  label: 'UNKNOWN',
  pointId: null,
  coordinates: null,
  reason: 'No validated position estimate is present in this live frame.',
});
assert.equal(
  normalizePositionEstimate({
    state: 'position',
    point_id: 'P05',
    coordinates_m: [Number.NaN, 0, 1.72],
  }).state,
  'unknown'
);
for (const coordinates_m of [
  ['2.01', 0, 1.72],
  [2.01, 0, 1.72, 99],
  [-0.01, 0, 1.72],
]) {
  assert.equal(
    positionEstimateViewModel({
      source: 'esp32',
      room_dimensions: room,
      position_estimate: {
        state: 'position',
        point_id: 'P05',
        coordinates_m,
      },
    }).state,
    'unknown'
  );
}
for (const coordinates_m of [
  [room[0] + 0.01, 0, 1.72],
  [2.01, room[1] + 0.01, 1.72],
  [2.01, 0, room[2] + 0.01],
]) {
  assert.equal(
    positionEstimateViewModel({
      source: 'esp32',
      room_dimensions: room,
      position_estimate: {
        state: 'position',
        point_id: 'P05',
        coordinates_m,
      },
    }).state,
    'unknown'
  );
}
for (const point_id of ['P00', 'P10', 'P1', ' P01', 'P01 ']) {
  assert.equal(
    positionEstimateViewModel({
      source: 'esp32',
      room_dimensions: room,
      position_estimate: {
        state: 'position',
        point_id,
        coordinates_m: [2.01, 0, 1.72],
      },
    }).state,
    'unknown'
  );
}
assert.equal(
  normalizePositionEstimate({
    state: 'position',
    point_id: '',
    coordinates_m: [2.01, 0, 1.72],
  }).state,
  'unknown'
);

const hudElements = new Map(
  [
    'localizationStatus',
    'positionPointId',
    'positionCoordinates',
    'positionReason',
    'classLabel',
  ].map((id) => [id, { textContent: '', className: '' }])
);
const sensingTabState = {
  container: {
    querySelector(selector) {
      return selector.startsWith('#') ? hudElements.get(selector.slice(1)) : null;
    },
  },
  _setText: SensingTab.prototype._setText,
  _renderPositionEstimate: SensingTab.prototype._renderPositionEstimate,
};
SensingTab.prototype._renderPositionEstimate.call(sensingTabState, {
  source: 'esp32',
  classification: { presence: true },
  position_estimate: {
    state: 'position',
    point_id: 'P05',
    coordinates_m: [2.01, 0, 1.72],
  },
});
assert.equal(hudElements.get('localizationStatus').textContent, 'POSITION');
assert.equal(hudElements.get('localizationStatus').className, 'sensing-localization-status position');
assert.equal(hudElements.get('positionPointId').textContent, 'P05');
assert.equal(
  hudElements.get('positionCoordinates').textContent,
  'x 2.01 m · y 0.00 m · z 1.72 m'
);
assert.equal(hudElements.get('positionReason').textContent, '--');

SensingTab.prototype._renderPositionEstimate.call(sensingTabState, {
  source: 'esp32',
  classification: { presence: false },
  position_estimate: {
    state: 'position',
    point_id: 'P05',
    coordinates_m: [2.01, 0, 1.72],
  },
});
assert.equal(hudElements.get('localizationStatus').textContent, 'NO PRESENCE');
assert.equal(
  hudElements.get('localizationStatus').className,
  'sensing-localization-status unknown'
);
assert.equal(hudElements.get('positionPointId').textContent, '--');
assert.equal(hudElements.get('positionCoordinates').textContent, '--');
assert.match(
  hudElements.get('positionReason').textContent,
  /Keine Präsenz/
);

SensingTab.prototype._renderPositionEstimate.call(sensingTabState, {
  source: 'esp32',
  classification: { presence: false },
  position_estimate: { state: 'ambiguous' },
});
assert.equal(hudElements.get('localizationStatus').textContent, 'AMBIGUOUS');
assert.equal(hudElements.get('positionPointId').textContent, '--');
assert.equal(hudElements.get('positionCoordinates').textContent, '--');
assert.match(hudElements.get('positionReason').textContent, /Several reference points/);

const simulatedFrame = {
  source: 'simulated',
  room_dimensions: room,
  classification: { presence: true },
};
assert.equal(isSimulatedSensingData(simulatedFrame), true);
assert.deepEqual(positionEstimateViewModel(simulatedFrame), {
  state: 'simulated',
  label: 'SIMULATED DEMO',
  pointId: 'DEMO',
  coordinates: [2.01, 0, 1.72],
  reason: 'Synthetic demonstration; not a measured person position.',
});
assert.deepEqual(resolvedBodyPosition(simulatedFrame), [2.01, 0, 1.72]);

for (const source of [
  'esp32',
  'esp32:offline',
  'esp32-offline',
  'wifi',
  'live',
  'hardware:test',
]) {
  const spoofedHardwareFrame = {
    source,
    _simulated: true,
    room_dimensions: room,
    classification: { presence: true },
  };
  assert.equal(
    isSimulatedSensingData(spoofedHardwareFrame),
    false,
    `${source} must override a spoofed simulation marker`
  );
  assert.equal(positionEstimateViewModel(spoofedHardwareFrame).state, 'unknown');
  assert.equal(resolvedBodyPosition(spoofedHardwareFrame), null);
}
for (const source of [undefined, '', 'unknown-source']) {
  const spoofedUnknownFrame = {
    source,
    _simulated: true,
    room_dimensions: room,
    classification: { presence: true },
  };
  assert.equal(isSimulatedSensingData(spoofedUnknownFrame), false);
  assert.equal(resolvedBodyPosition(spoofedUnknownFrame), null);
}

const coarseOnlyHardwareFrame = {
  source: 'esp32',
  classification: { presence: true },
  localization: {
    status: 'coarse',
    position: { x: 2.01, z: 1.72 },
  },
  signal_field: {
    grid_size: [20, 1, 20],
    values: [1],
  },
};
assert.equal(
  resolvedBodyPosition(coarseOnlyHardwareFrame),
  null,
  'legacy localization and the diagnostic heatmap must not create a hardware body'
);

const bodyOpacity = {
  array: new Float32Array([0.4, 0.2]),
  needsUpdate: false,
};
const fieldOpacity = {
  array: new Float32Array([0.7, 0.1]),
  needsUpdate: false,
};
const bodyPositions = [];
const rendererState = {
  _lastData: { localization: { status: 'coarse' } },
  roomDimensions: room,
  container: {
    dataset: {
      cloudState: 'position',
      cloudTarget: '1.000,0.000,1.000',
      cloudPointId: 'P01',
    },
  },
  bodyBlob: {
    geometry: { attributes: { splatOpacity: bodyOpacity } },
    position: {
      set(x, y, z) {
        bodyPositions.push([x, y, z]);
      },
    },
  },
  fieldPoints: {
    geometry: { attributes: { splatOpacity: fieldOpacity } },
  },
};

assert.equal(
  GaussianSplatRenderer.prototype._updateBodyPosition.call(
    rendererState,
    coarseOnlyHardwareFrame,
    true
  ),
  false
);
assert.equal(rendererState.container.dataset.cloudState, 'unknown');
assert.equal('cloudTarget' in rendererState.container.dataset, false);
assert.deepEqual([...bodyOpacity.array], [0, 0]);
assert.equal(bodyOpacity.needsUpdate, true);

const firstPositionFrame = {
  source: 'esp32',
  position_estimate: {
    state: 'position',
    point_id: 'P05',
    coordinates_m: [2.01, 0, 1.72],
  },
};
assert.equal(
  GaussianSplatRenderer.prototype._updateBodyPosition.call(
    rendererState,
    firstPositionFrame,
    true
  ),
  true
);
assert.deepEqual(bodyPositions.at(-1), [2.01, 0, 1.72]);
assert.equal(rendererState.container.dataset.cloudTarget, '2.010,0.000,1.720');
assert.equal(rendererState.container.dataset.cloudPointId, 'P05');

const secondPositionFrame = {
  source: 'esp32',
  position_estimate: {
    state: 'position',
    point_id: 'P09',
    coordinates_m: [3.27, 0, 2.69],
  },
};
GaussianSplatRenderer.prototype._updateBodyPosition.call(
  rendererState,
  secondPositionFrame,
  true
);
assert.ok(
  bodyPositions.at(-1).every(
    (coordinate, index) => Math.abs(coordinate - [0.75, 0, 2.69][index]) < 1e-12
  ),
  'every accepted discrete point must be mirrored without interpolation'
);
assert.equal(
  GaussianSplatRenderer.prototype._animate.toString().includes('.lerp('),
  false,
  'the render loop must not interpolate discrete position estimates'
);

for (const state of [
  'unknown',
  'ambiguous',
  'insufficient',
  'uncalibrated',
  'stale',
]) {
  GaussianSplatRenderer.prototype._updateBodyPosition.call(
    rendererState,
    { source: 'esp32', position_estimate: { state } },
    true
  );
  assert.deepEqual(bodyPositions.at(-1), [room[0] / 2, 0, room[2] / 2]);
  assert.equal(rendererState.container.dataset.cloudState, state);
  assert.equal('cloudTarget' in rendererState.container.dataset, false);
  assert.equal('cloudPointId' in rendererState.container.dataset, false);
  assert.deepEqual([...bodyOpacity.array], [0, 0]);
}

GaussianSplatRenderer.prototype._updateBodyPosition.call(
  rendererState,
  firstPositionFrame,
  false
);
assert.equal(
  'cloudTarget' in rendererState.container.dataset,
  false,
  'a position without classified presence must not leave a body target'
);

GaussianSplatRenderer.prototype._updateBodyPosition.call(
  rendererState,
  simulatedFrame,
  true
);
assert.deepEqual(bodyPositions.at(-1), [2.01, 0, 1.72]);
assert.equal(rendererState.container.dataset.cloudState, 'simulated');
assert.equal(rendererState.container.dataset.cloudPointId, 'DEMO');

GaussianSplatRenderer.prototype.invalidatePositionEstimate.call(
  rendererState,
  'stale'
);
assert.equal(rendererState._lastData, null);
assert.equal(rendererState.container.dataset.cloudState, 'stale');
assert.equal('cloudTarget' in rendererState.container.dataset, false);
assert.equal('cloudPointId' in rendererState.container.dataset, false);
assert.deepEqual([...bodyOpacity.array], [0, 0]);
assert.deepEqual([...fieldOpacity.array], [0, 0]);
assert.equal(bodyOpacity.needsUpdate, true);
assert.equal(fieldOpacity.needsUpdate, true);

let disconnectInvalidations = 0;
SensingTab.prototype._onStateChange.call(
  {
    container: { querySelector: () => null },
    _invalidateLiveReadout() {
      disconnectInvalidations += 1;
    },
  },
  'disconnected'
);
assert.equal(
  disconnectInvalidations,
  1,
  'disconnect must clear the previous body and point readout immediately'
);

let rendererInvalidationState = null;
SensingTab.prototype._invalidateLiveReadout.call({
  ...sensingTabState,
  splatRenderer: {
    invalidatePositionEstimate(state) {
      rendererInvalidationState = state;
    },
  },
  _setBar() {},
  _updateNodePanels() {},
});
assert.equal(rendererInvalidationState, 'stale');
assert.equal(hudElements.get('localizationStatus').textContent, 'STALE');
assert.equal(hudElements.get('positionPointId').textContent, '--');
assert.equal(hudElements.get('positionCoordinates').textContent, '--');

let resolveThree;
const lifecycleCounts = {
  builds: 0,
  loads: 0,
  renderers: 0,
  rendererDisposals: 0,
  subscriptions: 0,
  unsubscriptions: 0,
  observers: 0,
  observerDisconnections: 0,
};
const lifecycleTab = new SensingTab({});
lifecycleTab._buildDOM = () => {
  lifecycleCounts.builds += 1;
};
lifecycleTab._loadThree = () => {
  lifecycleCounts.loads += 1;
  return new Promise((resolve) => {
    resolveThree = resolve;
  });
};
lifecycleTab._initSplatRenderer = () => {
  lifecycleCounts.renderers += 1;
  lifecycleTab.splatRenderer = {
    dispose() {
      lifecycleCounts.rendererDisposals += 1;
    },
  };
};
lifecycleTab._connectService = () => {
  lifecycleCounts.subscriptions += 2;
  lifecycleTab._unsubData = () => {
    lifecycleCounts.unsubscriptions += 1;
  };
  lifecycleTab._unsubState = () => {
    lifecycleCounts.unsubscriptions += 1;
  };
};
lifecycleTab._setupResize = () => {
  lifecycleCounts.observers += 1;
  lifecycleTab._resizeObserver = {
    disconnect() {
      lifecycleCounts.observerDisconnections += 1;
    },
  };
};

const firstInit = lifecycleTab.init();
const concurrentInit = lifecycleTab.init();
assert.equal(firstInit, concurrentInit, 'concurrent init calls must share one promise');
assert.equal(lifecycleCounts.builds, 1);
assert.equal(lifecycleCounts.loads, 1);
assert.equal(lifecycleCounts.renderers, 0);
resolveThree();
await Promise.all([firstInit, concurrentInit]);
assert.equal(lifecycleCounts.renderers, 1);
assert.equal(lifecycleCounts.subscriptions, 2);
assert.equal(lifecycleCounts.observers, 1);

await lifecycleTab.init();
assert.equal(lifecycleCounts.builds, 1, 'repeated initialized init must be a no-op');
assert.equal(lifecycleCounts.renderers, 1);
lifecycleTab.dispose();
lifecycleTab.dispose();
assert.equal(lifecycleCounts.rendererDisposals, 1);
assert.equal(lifecycleCounts.unsubscriptions, 2);
assert.equal(lifecycleCounts.observerDisconnections, 1);
assert.equal(lifecycleTab.splatRenderer, null);
assert.equal(lifecycleTab._resizeObserver, null);

const pendingTab = new SensingTab({});
let resolvePendingThree;
let pendingRendererInits = 0;
pendingTab._buildDOM = () => {};
pendingTab._loadThree = () =>
  new Promise((resolve) => {
    resolvePendingThree = resolve;
  });
pendingTab._initSplatRenderer = () => {
  pendingRendererInits += 1;
};
pendingTab._connectService = () => {};
pendingTab._setupResize = () => {};
const pendingInit = pendingTab.init();
pendingTab.dispose();
resolvePendingThree();
await pendingInit;
assert.equal(
  pendingRendererInits,
  0,
  'an init disposed while loading must not create a renderer or render loop'
);

console.log('sensing discrete position UI mapping: ok');

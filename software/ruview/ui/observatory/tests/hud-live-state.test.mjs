import assert from 'node:assert/strict';

import { HudController } from '../js/hud-controller.js';

function element() {
  return {
    children: [],
    className: '',
    disabled: false,
    id: '',
    style: {},
    textContent: '',
    appendChild(child) {
      this.children.push(child);
      registerTree(child);
    },
    querySelector(selector) {
      if (selector === '.field-disclaimer') {
        return this.children.find(
          (child) => child.className === 'field-disclaimer'
        ) || null;
      }
      return null;
    },
  };
}

const elements = new Map(
  [
    'hud',
    'panel-signal',
    'data-source-label',
    'scenario-area',
    'scenario-description',
    'scenario-quick-select',
    'opt-scenario',
    'opt-cycle',
    'hr-value',
    'br-value',
    'conf-value',
    'hr-bar',
    'br-bar',
    'conf-bar',
    'rssi-value',
    'var-value',
    'motion-value',
    'persons-value',
    'presence-indicator',
    'presence-label',
    'fall-alert',
  ].map((id) => [id, element()])
);
const sourceDot = element();
const capabilityLabel = element();

function registerTree(node) {
  if (node.id) elements.set(node.id, node);
  for (const child of node.children || []) registerTree(child);
}

globalThis.document = {
  createElement() {
    return element();
  },
  getElementById(id) {
    return elements.get(id) || null;
  },
  querySelector(selector) {
    if (selector === '#data-source-badge .dot') return sourceDot;
    if (selector === '#capabilities-bar .cap-item span:last-child') {
      return capabilityLabel;
    }
    return null;
  },
};

const hud = Object.create(HudController.prototype);
hud._ensureEvidenceUI();
elements.set(capabilityLabel.id, capabilityLabel);
hud._lerpHr = 0;
hud._lerpBr = 0;
hud._lerpConf = 0;

assert.ok(elements.get('measurement-status'));
assert.ok(elements.get('measurement-mode-note'));
assert.ok(elements.get('measurement-position-status'));
assert.equal(
  elements.get('panel-signal').querySelector('.field-disclaimer').textContent,
  'Signal field is diagnostic CSI data, not a measured person position.'
);
assert.equal(capabilityLabel.id, 'primary-capability-label');

for (const [state, label, expectedClass] of [
  ['connecting', 'CONNECTING', 'dot dot--connecting'],
  ['live', 'LIVE ESP32', 'dot dot--live'],
  ['simulated', 'SIMULATED', 'dot dot--demo'],
  ['stale', 'STALE', 'dot dot--stale'],
]) {
  hud.updateSourceBadge({ state, label });
  assert.equal(sourceDot.className, expectedClass);
  assert.equal(elements.get('data-source-label').textContent, label);
}

const liveView = {
  sourceState: {
    state: 'live',
    label: 'LIVE ESP32',
    reason: 'Fresh frame from the explicit ESP32 source.',
  },
  contract: {
    geometry: {
      valid: true,
      receivers: [{}, {}, {}, {}],
    },
    position: {
      valid: true,
      pointId: 'P05',
      coordinates: [2.01, 0, 1.72],
    },
    signalField: { valid: true },
    showHardwareField: true,
  },
};
hud._updateMeasurementStatus(liveView);
assert.equal(
  elements.get('measurement-status').className,
  'measurement-status measurement-status--live'
);
assert.equal(
  elements.get('measurement-geometry-status').textContent,
  'MEASURED GEOMETRY · 4 RX · TX blue / RX amber'
);
assert.equal(
  elements.get('measurement-position-status').textContent,
  'P05 · x 2.01 · y 0.00 · z 1.72 m'
);
assert.equal(
  elements.get('measurement-field-note').textContent,
  'Diagnostic CSI field · not a person position'
);
assert.equal(
  elements.get('primary-capability-label').textContent,
  'Neutral Position Marker'
);

hud._updateMeasurementStatus({
  sourceState: {
    state: 'simulated',
    reason: 'Local demonstration data; not an ESP32 measurement.',
  },
});
assert.equal(
  elements.get('measurement-position-status').textContent,
  'Procedural animated pose · not measured'
);
assert.equal(
  elements.get('primary-capability-label').textContent,
  'Simulated Pose'
);

hud._updateMeasurementStatus({
  sourceState: {
    state: 'stale',
    reason: 'The last sensing frame is no longer fresh.',
  },
});
assert.equal(
  elements.get('measurement-position-status').textContent,
  'No current position'
);
assert.equal(
  elements.get('measurement-field-note').textContent,
  'Hardware marker and field cleared'
);

hud._setSimulationControlsEnabled(false);
assert.equal(elements.get('scenario-area').style.display, 'none');
assert.equal(elements.get('scenario-quick-select').disabled, true);
assert.equal(elements.get('opt-scenario').disabled, true);
assert.equal(elements.get('opt-cycle').disabled, true);

hud.updateHUD(null, {}, {
  sourceState: {
    state: 'connecting',
    label: 'CONNECTING',
    reason: 'Waiting for the first explicit sensing frame.',
  },
  contract: null,
});
assert.equal(elements.get('data-source-label').textContent, 'CONNECTING');
assert.equal(elements.get('presence-label').textContent, 'WAITING FOR FRAME');
assert.equal(elements.get('persons-value').textContent, 0);

console.log('Observatory HUD live-state tests passed.');

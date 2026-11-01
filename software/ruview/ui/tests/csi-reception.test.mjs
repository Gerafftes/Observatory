import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

import { HardwareTab } from '../components/HardwareTab.js';
import { ObservatoryControlCenter } from '../components/ObservatoryControlCenter.js';
import { apiService } from '../services/api.service.js';
import { experimentService } from '../services/experiment.service.js';
import { API_CONFIG } from '../config/api.config.js';
import { formatCsiReceptionMetric } from '../utils/csi-reception.js';

const synthetic = JSON.parse(readFileSync(new URL('./fixtures/csi-reception-synthetic.json', import.meta.url)));

// Only the DOM operations used by HardwareTab; no browser or network required.
function element() {
  return {
    children: [],
    style: { setProperty() {} },
    setAttribute() {},
    append(...children) { this.children.push(...children); },
    appendChild(child) { this.children.push(child); return child; },
    replaceChildren(...children) { this.children = children; },
    set textContent(value) { this.children = [String(value)]; },
    get textContent() { return this.children.map((child) => typeof child === 'string' ? child : child.textContent).join(''); },
  };
}

function withDocument(run) {
  const original = globalThis.document;
  globalThis.document = { createElement: element };
  try {
    return run();
  } finally {
    if (original === undefined) delete globalThis.document;
    else globalThis.document = original;
  }
}

const renderers = {
  'control center': (node) => {
    const center = new ObservatoryControlCenter(null);
    center.status = { nodes: [node] };
    return center._nodesMarkup();
  },
  'hardware tab': (node) => withDocument(() => new HardwareTab(null).createNodeRow(node).textContent),
};

for (const [name, render] of Object.entries(renderers)) {
  test(`${name} shows separate counts, real zeros and independent percentages`, () => {
    const complete = render(synthetic.nodes[0]);
    assert.match(complete, /Formatausschlüsse/);
    assert.match(complete, /Ungeklärte Sequenzlücken/);
    assert.match(complete, /3 · 60\.0%/);
    assert.match(complete, /0 · 0\.0%/);
    assert.doesNotMatch(complete, /Verlust|75\.0%|geschätzt/);
    const missing = render(synthetic.nodes[1]);
    assert.match(missing, /2 · 50\.0%/);
    assert.match(missing, /1 · 25\.0%/);
  });

  test(`${name} handles older API responses and missing metric fields without zero fallback`, () => {
    for (const node of [
      synthetic.nodes[2],
      { ...synthetic.nodes[2], csi_reception: {} },
      { ...synthetic.nodes[2], csi_reception: { format_exclusions: { percent: 60 } } },
    ]) {
      const markup = render(node);
      assert.match(markup, /nicht verfügbar/);
      assert.doesNotMatch(markup, /91\.2%|0\.0%|103 geschätzt|Verlust/);
    }
  });

  test(`${name} distinguishes a first frame from an available zero gap rate`, () => {
    const markup = render(synthetic.nodes[3]);
    assert.match(markup, /0 · 0\.0%/);
    assert.match(markup, /0 · --/);
    const empty = structuredClone(synthetic.nodes[3]);
    empty.csi_reception.format_exclusions = { count: 0, denominator: 0, valid: false, percent: null };
    assert.doesNotMatch(render(empty), /0\.0%/);
  });

  test(`${name} hides cumulative historical values while the RX is stale`, () => {
    const markup = render({ ...synthetic.nodes[0], status: 'stale', rssi_dbm: -39, frame_rate_hz: 22.1 });
    assert.match(markup, /keine aktuellen Daten/);
    assert.doesNotMatch(markup, /60\.0%|0\.0%|3 ·|22\.1 Hz|-39\.0 dBm|Verlust/);
  });
}

test('metric validity and denominator gate percentages independently of counts', () => {
  for (const metric of [
    { count: 2, denominator: 0, valid: true, percent: 50 },
    { count: 2, denominator: 4, valid: false, percent: 50 },
    { count: 2, denominator: 4, valid: true, percent: null },
    { count: 2, denominator: 4, valid: true, percent: NaN },
  ]) {
    assert.equal(formatCsiReceptionMetric({ status: 'active', csi_reception: { format_exclusions: metric } }, 'format_exclusions'), '2 · --');
  }
});

test('both displays explain denominators, cumulative scope and sync rate estimate', () => {
  const center = renderers['control center'](synthetic.nodes[0]);
  withDocument(() => {
    const row = new HardwareTab(null).createNodeRow(synthetic.nodes[0]);
    const help = row.children[1].children.map((child) => child.title || '').join(' ');
    for (const text of [center, help]) {
      assert.match(text, /alle gültigen neuen Frames am Rastergate/);
      assert.match(text, /Vorwärtsübergänge plus fehlende Positionen/);
      assert.match(text, /Kumulativ seit Beginn des RX-\/Serverzustands/);
      assert.match(text, /Bootwechsel/);
      assert.match(text, /unterschiedliche Nenner/);
      assert.match(text, /Schätzung aus Sync-Sequenzen/);
    }
  });
});

test('hardware refresh forwards the additive nodes payload and clears data on failure', async () => {
  const original = apiService.get;
  const status = element();
  const list = element();
  const tab = new HardwareTab({ querySelector: (selector) => selector === '#hardware-node-list' ? list : status });
  try {
    let rendered;
    const renderNodes = tab.renderNodes.bind(tab);
    tab.renderNodes = (payload) => { rendered = payload; };
    apiService.get = async (path) => {
      assert.equal(path, API_CONFIG.ENDPOINTS.NODES);
      return structuredClone(synthetic);
    };
    await tab.refreshNodes();
    assert.deepEqual(rendered, synthetic);
    tab.renderNodes = renderNodes;
    withDocument(() => tab.renderNodes(rendered));
    assert.match(list.textContent, /3 · 60\.0%/);
    apiService.get = async () => { throw new Error('offline'); };
    // Capture the asynchronous response, then render with the offline DOM.
    tab.renderNodes = (payload) => { rendered = payload; };
    await tab.refreshNodes();
    withDocument(() => renderNodes(rendered));
    assert.equal(status.textContent, 'Node-Daten nicht verfügbar');
    assert.doesNotMatch(list.textContent, /60\.0%|0\.0%/);
  } finally {
    apiService.get = original;
  }
});

test('control center service forwards the additive payload without transforming diagnostics', async () => {
  const original = apiService.get;
  try {
    apiService.get = async (path) => {
      assert.equal(path, '/api/v1/control-center/status');
      return structuredClone(synthetic);
    };
    assert.deepEqual(await experimentService.getControlCenterStatus(), synthetic);
  } finally {
    apiService.get = original;
  }
});

import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

import {
  MmwaveCalibrationAssistant,
  mmwaveAssistantViewModel,
  mmwaveTransportDiagnostic,
} from '../components/MmwaveCalibrationAssistant.js';

const stylesheet = readFileSync(new URL('../style.css', import.meta.url), 'utf8');
const syntheticPassStatus = JSON.parse(readFileSync(
  new URL('./fixtures/mmwave-synthetic-pass-status.json', import.meta.url),
  'utf8',
));

function zones(trainingBlocks = 0, blindVisits = 0) {
  return Array.from({ length: 9 }, (_, index) => ({
    id: `P${String(index + 1).padStart(2, '0')}`,
    center_mm: [index * 100, index * 100],
    training_blocks: trainingBlocks,
    blind_visits: blindVisits,
  }));
}

test('transport facts explain the target mismatch and show missing counters as unknown', () => {
  const assistant = new MmwaveCalibrationAssistant({});
  const html = assistant._transportFacts({
    node_control: { reachable: true, url_configured: true, token_configured: true },
    connection: { hint: 'Radar sendet an 192.168.4.50:5010; Server 192.168.4.3:5010.' },
    uart_bytes_received: null,
    cad_profile: { profile_sha256: 'd7682b4dccbadae3', mounting_position_m: [3.9, 0, 3.3] },
  });
  assert.match(html, /192\.168\.4\.50:5010/);
  assert.match(html, /192\.168\.4\.3:5010/);
  assert.match(html, /CAD-Profil d7682b4dccbadae3/);
  assert.match(html, /UART<\/dt><dd>--<\/dd>/);
  assert.doesNotMatch(html, /Konfiguration unvollständig/);
});

test('assistant starts at the connection gate', () => {
  const model = mmwaveAssistantViewModel({ state: 'disconnected', zones: [] });
  assert.equal(model.activeStep, 0);
  assert.equal(model.connected, false);
});

test('calibration reference follows the Samaritan visual contract', () => {
  const blockStart = stylesheet.indexOf('/* mmWave-guided D6 calibration assistant */');
  const blockEnd = stylesheet.indexOf('\n@media (max-width: 900px) {\n  .sensing-layout', blockStart + 1);
  const assistantStyles = stylesheet.slice(blockStart, blockEnd);

  assert.notEqual(blockStart, -1);
  assert.notEqual(blockEnd, -1);
  assert.match(assistantStyles, /--mmwave-paper:\s*#fafaf8/);
  assert.match(assistantStyles, /--mmwave-ink:\s*#111111/);
  assert.match(assistantStyles, /--mmwave-signal:\s*#e51c23/);
  assert.match(assistantStyles, /font-family:\s*var\(--font-family-mono\)/);
  assert.match(assistantStyles, /border-radius:\s*0/);
  assert.match(assistantStyles, /box-shadow:\s*none/);
  assert.doesNotMatch(assistantStyles, /#32b8c6/i);
});

test('connection guidance renders the configured UDP port', () => {
  const status = {
    state: 'disconnected',
    reason: 'Waiting for a packet.',
    udp_port: 15010,
    packets_rejected: 0,
    zones: [],
  };
  const assistant = new MmwaveCalibrationAssistant({});
  assistant.status = status;
  const html = assistant._guidance(mmwaveAssistantViewModel(status));
  assert.match(html, /15010/);
  assert.match(html, /UART/);
  assert.match(html, /Waiting for a packet\./);
});

test('transport diagnosis separates wiring, parser, and UDP failures', () => {
  assert.equal(mmwaveTransportDiagnostic({
    uart_bytes_received: 0,
    radar_frames_valid: 0,
    udp_packets_sent: 0,
  }).state, 'uart_idle');
  assert.equal(mmwaveTransportDiagnostic({
    uart_bytes_received: 30,
    radar_frames_valid: 0,
    udp_packets_sent: 0,
  }).state, 'invalid_frames');
  assert.equal(mmwaveTransportDiagnostic({
    uart_bytes_received: 30,
    radar_frames_valid: 1,
    udp_packets_sent: 0,
  }).state, 'udp_blocked');
  assert.equal(mmwaveTransportDiagnostic({
    uart_bytes_received: 300,
    radar_frames_valid: 10,
    udp_packets_sent: 10,
  }).state, 'streaming');
});

test('synthetic server-to-UI contract reaches the passed blind gate', () => {
  const model = mmwaveAssistantViewModel(syntheticPassStatus);
  const assistant = new MmwaveCalibrationAssistant({});
  assistant.status = syntheticPassStatus;
  const html = assistant._guidance(model);

  assert.equal(model.trainingComplete, true);
  assert.equal(model.blindComplete, true);
  assert.equal(model.activeStep, 6);
  assert.equal(mmwaveTransportDiagnostic(syntheticPassStatus).state, 'streaming');
  assert.match(html, /Blindtest PASS/);
  assert.match(html, /für die Live-Anzeige freigegeben/);
});

test('coverage and training phases select the corresponding guided step', () => {
  const coverage = mmwaveAssistantViewModel({
    state: 'valid',
    transform: {},
    coverage_cells: 12,
    zones: [],
    session: { phase: 'coverage', kind: 'calibration' },
  });
  assert.equal(coverage.activeStep, 2);

  const training = mmwaveAssistantViewModel({
    state: 'valid',
    transform: {},
    coverage_cells: 40,
    zones: zones(2),
    session: { phase: 'training', kind: 'calibration' },
  });
  assert.equal(training.activeStep, 4);
});

test('blind completion requires two visits in every trained zone', () => {
  const incomplete = mmwaveAssistantViewModel({
    state: 'valid',
    transform: {},
    zones: zones(6, 1),
  });
  assert.equal(incomplete.trainingComplete, true);
  assert.equal(incomplete.blindComplete, false);
  assert.equal(incomplete.activeStep, 5);

  const complete = mmwaveAssistantViewModel({
    state: 'valid',
    transform: {},
    zones: zones(6, 2),
  });
  assert.equal(complete.blindComplete, true);
  assert.equal(complete.activeStep, 6);
});

test('calibration start remains locked until the setup is sealed', () => {
  const status = {
    state: 'valid',
    configured: true,
    setup_sealed: false,
    node_id: 'radar-01',
    mode: 'calibration',
    packet_age_ms: 10,
    zones: [],
  };
  const assistant = new MmwaveCalibrationAssistant({});
  assistant.status = status;
  const html = assistant._guidance(mmwaveAssistantViewModel(status));
  assert.match(html, /Zwei-Phasen-Kalibrierung<\/button>/);
  assert.match(html, /disabled/);
  assert.match(html, /versiegeltes Setup/);
});

test('passed radar preflight unlocks phase one without requiring CSI', () => {
  const status = {
    state: 'valid',
    configured: true,
    setup_sealed: true,
    node_id: 'radar-01',
    mode: 'calibration',
    packet_age_ms: 10,
    zones: [],
    preflight: { ready: true, gates: [] },
    radar_preflight: { ready: true, gates: [] },
    fixed_point_preflight: { ready: true, gates: [] },
  };
  const assistant = new MmwaveCalibrationAssistant({});
  assistant.status = status;
  const html = assistant._guidance(mmwaveAssistantViewModel(status));
  assert.match(html, /25-s-Preflight bestanden/);
  assert.match(html, /data-mmwave-action="prepare-calibration"/);
  assert.doesNotMatch(html, /prepare-calibration" class="mmwave-primary-button" disabled/);
  assert.match(html, /SOFTWARE-ONLY \/ UNVALIDATED/);
});

test('an outside-room target stays connected and does not hide phase one', () => {
  const status = {
    state: 'invalid',
    reason: 'target [5000, 1200] mm is outside room',
    last_rejection: { category: 'room_bounds' },
    configured: true,
    setup_sealed: true,
    zones: [],
    radar_preflight: { ready: true, gates: [] },
    fixed_point_preflight: { ready: true, gates: [] },
    preflight: { ready: false, gates: [{ id: 'rx1_25s_ready', pass: false, detail: 'offline' }] },
  };
  const assistant = new MmwaveCalibrationAssistant({});
  assistant.status = status;

  const model = mmwaveAssistantViewModel(status);
  const html = assistant._guidance(model);

  assert.equal(model.connected, true);
  assert.match(html, /Zwei-Phasen-Kalibrierung<\/button>/);
  assert.doesNotMatch(html, /prepare-calibration" class="mmwave-primary-button" disabled/);
  assert.doesNotMatch(html, /Warte auf Radar/);
});

test('phase one advances in server order and only then offers phase two', async () => {
  const anchors = ['RX1', 'RX2', 'RX3', 'RX4', 'TX'].map((id, index) => ({
    id,
    state: index === 0 ? 'current' : 'pending',
    check: null,
  }));
  const assistant = new MmwaveCalibrationAssistant({});
  assistant._render = () => {};
  assistant.status = {
    preflight: { ready: true, gates: [] },
    fixed_point_calibration: { state: 'active', current_anchor_id: 'RX1', anchors },
  };
  assistant.calibrationPlan = {
    phase: 'anchor_measuring', anchorId: 'RX1', leadSeconds: 5, toleranceMm: 350, durationSeconds: 65,
  };
  const originalFetch = globalThis.fetch;
  let call = 0;
  const requestBodies = [];
  globalThis.fetch = async (_url, options) => {
    call += 1;
    requestBodies.push(JSON.parse(options.body));
    const complete = call === 2;
    return {
      ok: true,
      async json() {
        return {
          fixed_point_calibration: complete
            ? {
              state: 'complete',
              current_anchor_id: null,
              anchors: anchors.map((anchor) => ({ ...anchor, state: 'complete', check: { pass: true } })),
            }
            : {
              state: 'active',
              current_anchor_id: 'RX2',
              anchors: anchors.map((anchor, index) => ({
                ...anchor,
                state: index === 0 ? 'complete' : index === 1 ? 'current' : 'pending',
                check: index === 0 ? { pass: false, median_error_mm: 980, sample_count: 8 } : null,
              })),
            },
        };
      },
    };
  };

  try {
    await assistant._checkCurrentFixedPoint();
    assert.equal(assistant.status.fixed_point_calibration.current_anchor_id, 'RX2');
    assert.equal(assistant.calibrationPlan.phase, 'anchor_waiting');
    assistant.calibrationPlan = { ...assistant.calibrationPlan, phase: 'anchor_measuring', anchorId: 'RX2' };
    await assistant._checkCurrentFixedPoint();
  } finally {
    globalThis.fetch = originalFetch;
  }

  assert.equal(assistant.calibrationPlan.phase, 'phase_two_ready');
  assert.deepEqual(requestBodies.map((body) => body.window_ms), [10_000, 10_000]);
  assert.match(assistant._calibrationPreparationMarkup(), /Phase 2 starten/);
});

test('a brief stale radar status does not hide an active phase-one measurement', () => {
  const assistant = new MmwaveCalibrationAssistant({});
  assistant.status = {
    state: 'stale',
    reason: 'The last mmWave packet is stale.',
    zones: [],
    fixed_point_calibration: {
      state: 'active',
      current_anchor_id: 'RX3',
      anchors: [{ id: 'RX3', state: 'current', check: null }],
    },
  };
  assistant.calibrationPlan = {
    phase: 'anchor_measuring',
    anchorId: 'RX3',
    startsAtMs: Date.now() + 5_000,
    displaySeconds: 5,
  };

  const html = assistant._guidance(mmwaveAssistantViewModel(assistant.status));

  assert.match(html, /PHASE 1 · MESSUNG/);
  assert.match(html, /RX3/);
  assert.doesNotMatch(html, /Warte auf Radar/);
});

test('yaw result compares every RX and TX point before phase two', () => {
  const assistant = new MmwaveCalibrationAssistant({});
  const html = assistant._yawCalibrationMarkup({
    applied: true,
    optimized_yaw_mdeg: 223400,
    base_yaw_mdeg: 217400,
    correction_mdeg: 6000,
    point_count: 5,
    before_rms_error_mm: 1010,
    after_rms_error_mm: 180,
    improvement_percent: 82.2,
    anchors: ['RX1', 'RX2', 'RX3', 'RX4', 'TX'].map((id, index) => ({
      id,
      before_error_mm: 900 + index * 20,
      after_error_mm: 120 + index * 10,
    })),
  });

  assert.match(html, /KORREKTUR AKTIV/);
  assert.match(html, /223\.4°/);
  assert.match(html, /1010 → 180 mm/);
  assert.match(html, /RX1/);
  assert.match(html, /RX4/);
  assert.match(html, /TX/);
  assert.match(html, /5 Punkte gemeinsam/);
});

test('completed yaw calibration remains repeatable from the alignment details', () => {
  const source = readFileSync(new URL('../components/MmwaveCalibrationAssistant.js', import.meta.url), 'utf8');

  assert.match(source, /id="mmwaveYawCalibrationResult"/);
  assert.match(source, /data-mmwave-action="repeat-yaw-calibration"/);
  assert.match(source, />Winkel neu kalibrieren</);
  assert.match(source, /this\.busy \|\| this\.status\?\.session \? 'disabled' : ''/);
});

test('repeat yaw calibration returns to the standard view and hides the old result after refresh', async () => {
  const assistant = new MmwaveCalibrationAssistant({});
  assistant._render = () => {};
  assistant.status = {
    state: 'valid',
    configured: true,
    setup_sealed: true,
    fixed_point_calibration: { state: 'complete' },
    yaw_calibration: { optimized_yaw_mdeg: 217400 },
    fixed_point_preflight: { ready: true, gates: [] },
    radar_preflight: { ready: true, gates: [] },
    zones: [],
  };
  assistant.calibrationPlan = { phase: 'phase_two_ready' };
  const originalFetch = globalThis.fetch;
  const requests = [];
  globalThis.fetch = async (url, options = {}) => {
    requests.push({ url, method: options.method || 'GET' });
    return {
      ok: true,
      async json() {
        return {
          ...assistant.status,
          ...(options.method === 'POST'
            ? { fixed_point_calibration: null, yaw_calibration: null }
            : { fixed_point_calibration: { state: 'complete' }, yaw_calibration: { optimized_yaw_mdeg: 217400 } }),
        };
      },
    };
  };

  try {
    await assistant._onClick({
      target: {
        closest(selector) {
          return selector === '[data-mmwave-action]'
            ? { dataset: { mmwaveAction: 'repeat-yaw-calibration' } }
            : null;
        },
      },
    });
    assert.deepEqual(requests, [{
      url: '/api/v1/mmwave/fixed-points/cancel',
      method: 'POST',
    }]);
    assert.equal(assistant.calibrationPlan, null);
    assert.equal(assistant.status.yaw_calibration, null);
    assert.equal(assistant.status.fixed_point_calibration, null);
    await assistant.refresh();
  } finally {
    globalThis.fetch = originalFetch;
  }

  assert.deepEqual(requests, [
    { url: '/api/v1/mmwave/fixed-points/cancel', method: 'POST' },
    { url: '/api/v1/mmwave/status', method: 'GET' },
  ]);
  assert.equal(assistant.calibrationPlan, null);
  assert.equal(assistant.status.yaw_calibration, null);
  assert.equal(assistant.status.fixed_point_calibration, null);
  const html = assistant._guidance(mmwaveAssistantViewModel(assistant.status));
  assert.match(html, /data-mmwave-action="prepare-calibration"/);
  assert.doesNotMatch(html, /PHASE 1 ABGESCHLOSSEN|KORREKTUR AKTIV/);
});

test('repeat dismissal survives assistant recreation until a fresh phase-one start', async () => {
  const storage = {
    value: null,
    getItem() { return this.value; },
    setItem(_key, value) { this.value = value; },
    removeItem() { this.value = null; },
  };
  const previousStorage = globalThis.sessionStorage;
  Object.defineProperty(globalThis, 'sessionStorage', { configurable: true, value: storage });
  const originalFetch = globalThis.fetch;
  globalThis.fetch = async (_url, options = {}) => ({
    ok: true,
    async json() {
      return options.method === 'POST'
        ? { fixed_point_calibration: null, yaw_calibration: null }
        : {
          state: 'valid',
          zones: [],
          fixed_point_calibration: { state: 'complete' },
          yaw_calibration: { optimized_yaw_mdeg: 217400 },
        };
    },
  });

  try {
    const first = new MmwaveCalibrationAssistant({});
    first._render = () => {};
    first.status = {
      state: 'valid',
      fixed_point_calibration: { state: 'complete' },
      yaw_calibration: { optimized_yaw_mdeg: 217400 },
    };
    await first._onClick({
      target: {
        closest(selector) {
          return selector === '[data-mmwave-action]'
            ? { dataset: { mmwaveAction: 'repeat-yaw-calibration' } }
            : null;
        },
      },
    });

    const recreated = new MmwaveCalibrationAssistant({});
    recreated._render = () => {};
    await recreated.refresh();

    assert.equal(storage.value, 'true');
    assert.equal(recreated.status.fixed_point_calibration, null);
    assert.equal(recreated.status.yaw_calibration, null);
  } finally {
    globalThis.fetch = originalFetch;
    if (previousStorage === undefined) delete globalThis.sessionStorage;
    else Object.defineProperty(globalThis, 'sessionStorage', { configurable: true, value: previousStorage });
  }
});

test('a fresh phase-one start re-enables server fixed-point status after the reset', async () => {
  const assistant = new MmwaveCalibrationAssistant({});
  assistant._render = () => {};
  assistant.fixedPointResultDismissed = true;
  assistant.calibrationPlan = {
    phase: 'phase_one_starting', durationSeconds: 65, leadSeconds: 5, toleranceMm: 350,
  };
  const originalFetch = globalThis.fetch;
  globalThis.fetch = async () => ({
    ok: true,
    async json() {
      return {
        fixed_point_calibration: {
          state: 'active', current_anchor_id: 'RX1', anchors: [],
        },
      };
    },
  });

  try {
    await assistant._startFixedPointPhase();
  } finally {
    globalThis.fetch = originalFetch;
  }

  assert.equal(assistant.fixedPointResultDismissed, false);
  assert.equal(assistant.status.fixed_point_calibration.state, 'active');
  assert.equal(assistant.calibrationPlan.phase, 'anchor_waiting');
});

test('a completed server snapshot does not replace an already prepared repeat form', async () => {
  const assistant = new MmwaveCalibrationAssistant({});
  assistant._render = () => {};
  assistant.calibrationPlan = assistant._defaultCalibrationPlan();
  const originalFetch = globalThis.fetch;
  globalThis.fetch = async () => ({
    ok: true,
    async json() {
      return { state: 'valid', zones: [], fixed_point_calibration: { state: 'complete' } };
    },
  });

  try {
    await assistant.refresh();
  } finally {
    globalThis.fetch = originalFetch;
  }

  assert.equal(assistant.calibrationPlan.phase, 'form');
});

test('status refresh requests are serialized to prevent stale UI snapshots', async () => {
  const assistant = new MmwaveCalibrationAssistant({});
  assistant._render = () => {};
  const originalFetch = globalThis.fetch;
  let calls = 0;
  let resolveResponse;
  globalThis.fetch = async () => {
    calls += 1;
    return new Promise((resolve) => {
      resolveResponse = resolve;
    });
  };

  try {
    const first = assistant.refresh();
    await assistant.refresh();
    assert.equal(calls, 1);
    assert.equal(assistant.refreshInFlight, true);

    resolveResponse({
      ok: true,
      async json() {
        return { state: 'no_target', zones: [] };
      },
    });
    await first;
    assert.equal(assistant.refreshInFlight, false);
  } finally {
    globalThis.fetch = originalFetch;
  }
});

test('calibration preparation exposes fixed-point and phase-two settings', () => {
  const assistant = new MmwaveCalibrationAssistant({});
  assistant.status = {
    state: 'valid',
    zones: [],
    preflight: { ready: true, gates: [] },
  };
  assistant.calibrationPlan = {
    phase: 'form', durationSeconds: 120, leadSeconds: 30, toleranceMm: 420,
  };

  const html = assistant._guidance(mmwaveAssistantViewModel(assistant.status));

  assert.match(html, /id="mmwaveCalibrationPrepareForm"/);
  assert.match(html, /name="duration_seconds"[^>]+min="60"[^>]+value="120"/);
  assert.match(html, /name="lead_seconds"[^>]+min="5"[^>]+value="30"/);
  assert.match(html, /name="tolerance_mm"[^>]+value="420"/);
  assert.match(html, /Phase 1 starten/);
});

test('calibration preparation starts the server-owned fixed-point phase', async () => {
  const assistant = new MmwaveCalibrationAssistant({});
  assistant._render = () => {};
  const form = {
    querySelector(selector) {
      if (selector.includes('duration_seconds')) return { value: '120' };
      if (selector.includes('lead_seconds')) return { value: '30' };
      return { value: '420' };
    },
  };
  const originalFetch = globalThis.fetch;
  globalThis.fetch = async () => ({
    ok: true,
    async json() {
      return {
        fixed_point_calibration: {
          state: 'active', current_anchor_id: 'RX1', anchors: [],
        },
      };
    },
  });

  try {
    await assistant._scheduleCalibration(form);
  } finally {
    globalThis.fetch = originalFetch;
  }

  assert.equal(assistant.calibrationPlan.phase, 'anchor_waiting');
  assert.equal(assistant.calibrationPlan.durationSeconds, 120);
  assert.equal(assistant.calibrationPlan.leadSeconds, 30);
  assert.equal(assistant.calibrationPlan.toleranceMm, 420);
});

test('countdown starts mmWave calibration with the chosen empty duration', async () => {
  const calibrationContext = {
    profile_id: 'profile-fixed-room',
    profile_revision_id: 'profile-fixed-room-v7',
  };
  const assistant = new MmwaveCalibrationAssistant({}, () => calibrationContext);
  assistant._render = () => {};
  const originalFetch = globalThis.fetch;
  let requestBody = null;
  globalThis.fetch = async (_url, options) => {
    requestBody = JSON.parse(options.body);
    return {
      ok: true,
      async json() {
        return { state: 'no_target', session: { kind: 'calibration', phase: 'empty_calibration' } };
      },
    };
  };

  try {
    await assistant._startCalibration({ durationSeconds: 120, leadSeconds: 30 });
  } finally {
    globalThis.fetch = originalFetch;
  }

  assert.deepEqual(requestBody, {
    kind: 'calibration',
    calibration_context: calibrationContext,
    policy: { zone_count: 9, empty_calibration_seconds: 120 },
  });
  assert.equal(assistant.calibrationPlan.phase, 'collecting');
});

test('calibration start fails before the request when no setup profile is selected', async () => {
  const assistant = new MmwaveCalibrationAssistant({});
  assistant._render = () => {};
  const originalFetch = globalThis.fetch;
  let fetchCalled = false;
  globalThis.fetch = async () => {
    fetchCalled = true;
    throw new Error('fetch must not run');
  };

  try {
    await assistant._startCalibration({ durationSeconds: 65, leadSeconds: 20 });
  } finally {
    globalThis.fetch = originalFetch;
  }

  assert.equal(fetchCalled, false);
  assert.match(assistant.actionError, /Setup-Profil ausgewählt/);
});

test('completion follows the sealed variable zone count', () => {
  const model = mmwaveAssistantViewModel({
    state: 'valid',
    zone_count: 3,
    zones: zones(6, 2).slice(0, 3),
  });
  assert.equal(model.trainingComplete, true);
  assert.equal(model.blindComplete, true);
  assert.equal(model.zoneCount, 3);
});

test('mmWave assistant is read-only before the physical sensor check', () => {
  const assistant = new MmwaveCalibrationAssistant({});
  const shell = assistant._shell();

  assert.match(shell, /READ-ONLY/);
  assert.match(shell, /name="origin_x_mm" type="number" required disabled/);
  assert.match(shell, /class="mmwave-secondary-button" disabled>Ausrichtung speichern<\/button>/);
});

test('failed blind run explicitly keeps live positioning locked', () => {
  const status = {
    state: 'valid',
    position_live_approved: false,
    blind_verdict: 'FAIL',
    blind_report_sha256: 'a'.repeat(64),
    zones: zones(6, 2),
  };
  const assistant = new MmwaveCalibrationAssistant({});
  assistant.status = status;
  const html = assistant._guidance(mmwaveAssistantViewModel(status));
  assert.match(html, /Blindtest FAIL/);
  assert.match(html, /bleibt für die Live-Anzeige gesperrt/);
});

test('training guidance uses coordinates instead of manual P labels', () => {
  const status = {
    state: 'valid',
    mode: 'calibration',
    target_position_mm: [1_240, 1_390],
    recommended_zone_id: 'P02',
    zones: zones(2, 0),
    session: {
      phase: 'training',
      kind: 'calibration',
      aligned_samples: 42,
    },
  };
  const assistant = new MmwaveCalibrationAssistant({});
  assistant.status = status;
  const html = assistant._guidance(mmwaveAssistantViewModel(status));
  assert.match(html, /dünn erfassten Bereich bei 0\.10 \/ 0\.10 m/);
  assert.doesNotMatch(html, /Gehe zu P02/);
  assert.match(html, /Radar 1\.24 \/ 1\.39 m/);
});

test('calibration walk explains continuous radar labels without a manual point grid', () => {
  const status = {
    state: 'valid',
    node_id: 'radar-01',
    mode: 'calibration',
    packet_age_ms: 8,
    zones: [],
  };
  const assistant = new MmwaveCalibrationAssistant({});
  assistant.status = status;

  const html = assistant._guidance(mmwaveAssistantViewModel(status));

  assert.match(html, /Phase 1 bestätigt RX1–RX4 und TX/);
  assert.match(html, /Radar-X\/Z und CSI/);
});

test('HTTP 409 start errors survive a later successful status refresh', async () => {
  const assistant = new MmwaveCalibrationAssistant({}, () => ({
    profile_id: 'profile-fixed-room',
    profile_revision_id: 'profile-fixed-room-v7',
  }));
  assistant._render = () => {};
  const originalFetch = globalThis.fetch;
  let calls = 0;
  globalThis.fetch = async () => {
    calls += 1;
    if (calls === 1) {
      return {
        ok: false,
        status: 409,
        async json() {
          return { error: 'mmWave preflight is not ready: radar_stream_fresh' };
        },
      };
    }
    return {
      ok: true,
      async json() {
        return { state: 'valid', zones: [] };
      },
    };
  };

  try {
    await assistant._startCalibration({ durationSeconds: 120, leadSeconds: 30 });
    assert.match(assistant.actionError, /preflight is not ready/);
    await assistant.refresh();
  } finally {
    globalThis.fetch = originalFetch;
  }

  assert.match(assistant.actionError, /radar_stream_fresh/);
  assert.equal(assistant.statusError, '');
});

test('active stale sessions explain the interrupted radar stream instead of hiding the session', () => {
  const status = {
    state: 'stale',
    reason: 'No fresh transport packet.',
    zones: [],
    node_control: { reachable: true },
    session: {
      lifecycle: 'active',
      kind: 'calibration',
      phase: 'empty_calibration',
      aligned_samples: 12,
    },
  };
  const assistant = new MmwaveCalibrationAssistant({});
  assistant.status = status;

  const html = assistant._guidance(mmwaveAssistantViewModel(status));

  assert.match(html, /Radar verbunden, aber Datenstrom unterbrochen/);
  assert.match(html, /Die Sitzung läuft bis zum konfigurierten Ende weiter/);
  assert.match(html, /Stoppen/);
  assert.doesNotMatch(html, /<h4>Warte auf Radar<\/h4>/);
});

test('completed empty calibration shows a validity verdict and concrete reasons', () => {
  const status = {
    state: 'valid',
    mode: 'calibration',
    zones: [],
    session: {
      lifecycle: 'active',
      kind: 'calibration',
      phase: 'coverage',
      aligned_samples: 24,
      empty_validity: {
        verdict: 'invalid',
        reasons: ['2 in-room radar target packet(s) were observed'],
        outside_room_targets: 4,
        in_room_targets: 2,
        multi_target_packets: 0,
        invalid_packets: 0,
        sequence_gaps: 0,
        reboots: 0,
        radar_packets: 20,
        max_radar_gap_ms: 120,
        csi_frames: 240,
        duration_seconds: 65,
      },
    },
  };
  const assistant = new MmwaveCalibrationAssistant({});
  assistant.status = status;

  const html = assistant._guidance(mmwaveAssistantViewModel(status));

  assert.match(html, /Leermessung: UNGÜLTIG/);
  assert.match(html, /2 in-room radar target packet/);
  assert.match(html, /4 Außenraum-Ziele ignoriert/);
});

test('interrupted sessions remain visible without offering an automatic continuation', () => {
  const status = {
    state: 'disconnected',
    reason: 'Server restarted.',
    zones: [],
    session: {
      lifecycle: 'interrupted',
      kind: 'calibration',
      phase: 'empty_calibration',
      error: null,
      empty_validity: {
        verdict: 'invalid',
        reasons: ['radar transport gap reached 64000 ms'],
        outside_room_targets: 0,
        csi_frames: 0,
      },
    },
  };
  const assistant = new MmwaveCalibrationAssistant({});
  assistant.status = status;

  const html = assistant._guidance(mmwaveAssistantViewModel(status));

  assert.match(html, /Sitzung unterbrochen/);
  assert.match(html, /Leermessung: UNGÜLTIG/);
  assert.match(html, /nicht automatisch fortgesetzt/);
  assert.doesNotMatch(html, /data-mmwave-action="stop"/);
  assert.doesNotMatch(html, /Warte auf Radar/);
});

test('preflight blockers use understandable labels and transport details', () => {
  const status = {
    configured: true,
    setup_sealed: true,
    preflight: {
      ready: false,
      gates: [
        { id: 'radar_stream_fresh', pass: false, detail: 'age_ms=2400' },
        { id: 'rx1_25s_ready', pass: false, detail: 'only 4 seconds observed' },
      ],
    },
    reject_reasons: { room_bounds: 3 },
    raw_udp_packets: 12,
    transport: {
      queue_length: 2,
      queue_peak: 5,
      window_samples: 256,
      valid_samples: 252,
      valid_rate_hz: 9.96,
      inter_arrival_median_ms: 100,
      inter_arrival_p95_ms: 112,
      receive_to_process_median_ms: 3,
      receive_to_process_p95_ms: 14,
    },
  };
  const assistant = new MmwaveCalibrationAssistant({});

  const requirement = assistant._startRequirement(status);
  const facts = assistant._transportFacts(status);

  assert.match(requirement, /Radar-Transport frisch/);
  assert.match(requirement, /RX1-Stream 25 s/);
  assert.doesNotMatch(requirement, /radar_stream_fresh/);
  assert.match(facts, /UDP roh/);
  assert.match(facts, /room_bounds 3/);
  assert.match(facts, /Queue \/ Peak/);
  assert.match(facts, /252 \/ 256/);
  assert.match(facts, /9\.96 Hz/);
  assert.match(facts, /100 ms \/ 112 ms/);
  assert.match(facts, /3 ms \/ 14 ms/);
});

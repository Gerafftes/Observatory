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

test('calibration start remains locked until setup v2 is sealed', () => {
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
  assert.match(html, /Kalibrierung vorbereiten<\/button>/);
  assert.match(html, /disabled/);
  assert.match(html, /Setup-v2/);
});

test('passed 25-second preflight unlocks calibration start without claiming validation', () => {
  const status = {
    state: 'valid',
    configured: true,
    setup_sealed: true,
    node_id: 'radar-01',
    mode: 'calibration',
    packet_age_ms: 10,
    zones: [],
    preflight: { ready: true, gates: [] },
  };
  const assistant = new MmwaveCalibrationAssistant({});
  assistant.status = status;
  const html = assistant._guidance(mmwaveAssistantViewModel(status));
  assert.match(html, /25-s-Preflight bestanden/);
  assert.match(html, /data-mmwave-action="prepare-calibration"/);
  assert.doesNotMatch(html, /prepare-calibration" class="mmwave-primary-button" disabled/);
  assert.match(html, /SOFTWARE-ONLY \/ UNVALIDATED/);
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

test('calibration preparation exposes configurable empty duration and lead time', () => {
  const assistant = new MmwaveCalibrationAssistant({});
  assistant.status = {
    state: 'valid',
    zones: [],
    preflight: { ready: true, gates: [] },
  };
  assistant.calibrationPlan = { phase: 'form', durationSeconds: 120, leadSeconds: 30 };

  const html = assistant._guidance(mmwaveAssistantViewModel(assistant.status));

  assert.match(html, /id="mmwaveCalibrationPrepareForm"/);
  assert.match(html, /name="duration_seconds"[^>]+min="60"[^>]+value="120"/);
  assert.match(html, /name="lead_seconds"[^>]+min="5"[^>]+value="30"/);
  assert.match(html, /Countdown starten/);
});

test('calibration preparation validates and schedules the chosen countdown', () => {
  const assistant = new MmwaveCalibrationAssistant({});
  assistant._render = () => {};
  assistant._startCalibrationTimer = () => {};
  const before = Date.now();
  const form = {
    querySelector(selector) {
      return { value: selector.includes('duration_seconds') ? '120' : '30' };
    },
  };

  assistant._scheduleCalibration(form);

  assert.equal(assistant.calibrationPlan.phase, 'countdown');
  assert.equal(assistant.calibrationPlan.durationSeconds, 120);
  assert.equal(assistant.calibrationPlan.leadSeconds, 30);
  assert.ok(assistant.calibrationPlan.startsAtMs >= before + 29_000);
  assert.match(assistant._calibrationPreparationMarkup(), /Sicher zurück ab \d{2}:\d{2}:\d{2} Uhr/);
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

  assert.match(html, /CSI wird mit Radar-X\/Z verknüpft/);
  assert.match(html, /P01–P09 entfällt/);
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
    transport: { queue_length: 2, queue_peak: 5, last_receive_to_process_delay_ms: 14 },
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
});

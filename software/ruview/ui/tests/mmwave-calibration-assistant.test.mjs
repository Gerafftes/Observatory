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
  assert.match(html, /Kalibrierung<\/button>/);
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
  assert.doesNotMatch(html, /start-calibration" class="mmwave-primary-button" disabled/);
  assert.match(html, /SOFTWARE-ONLY \/ UNVALIDATED/);
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

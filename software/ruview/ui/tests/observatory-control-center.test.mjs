import assert from 'node:assert/strict';
import test from 'node:test';

import { apiService } from '../services/api.service.js';
import { experimentService } from '../services/experiment.service.js';
import {
  generateThreeByThreePoints,
  ObservatoryControlCenter,
  defaultSetupProfileDocument,
} from '../components/ObservatoryControlCenter.js';

test('default setup profile keeps the legacy point grid only for schema compatibility', () => {
  const profile = defaultSetupProfileDocument();

  assert.deepEqual(profile.room_dimensions_m, [4.02, 2.59, 3.44]);
  assert.equal(profile.sensor_mount_radius_m, 0.5);
  assert.equal(profile.transmitter.id, 'TX');
  assert.deepEqual(profile.receivers.map((receiver) => receiver.id), ['RX1', 'RX2', 'RX3', 'RX4']);
  assert.equal(profile.mmwave.sensor, 'HLK-LD2450');
  assert.deepEqual(profile.mmwave.mounting_position_m, [0.0, 1.2, 1.72]);
  assert.equal(profile.mmwave.allow_exterior, true);
  assert.deepEqual(profile.points.map((point) => point.id), [
    'P01', 'P02', 'P03', 'P04', 'P05', 'P06', 'P07', 'P08', 'P09',
  ]);
  assert.equal(profile.mmwave_status, 'NOT_CONNECTED');
});

test('geometry snapshot exposes the same CAD mmWave, TX, and RX positions', () => {
  const controlCenter = new ObservatoryControlCenter(null);
  controlCenter.profileDraft = defaultSetupProfileDocument();
  controlCenter.profileDraft.mmwave.mounting_position_m = [2.05, 1.55, 3.3];
  controlCenter.profileDraft.transmitter.position_m = [1.6, 1.2, 0.4];

  const snapshot = controlCenter.getCurrentGeometrySnapshot();

  assert.deepEqual(snapshot.roomDimensions, [4.02, 2.59, 3.44]);
  assert.deepEqual(snapshot.mountingPositionM, [2.05, 1.55, 3.3]);
  assert.deepEqual(snapshot.txPosition, [1.6, 1.2, 0.4]);
  assert.deepEqual(snapshot.receiverPositionsM.map((node) => node.id), ['RX1', 'RX2', 'RX3', 'RX4']);
  assert.deepEqual(snapshot.receiverPositionsM[3].position, [4.02, 0.87, 2.46]);
});

test('profile reads the CAD mmWave exterior policy together with form coordinates', () => {
  const controlCenter = new ObservatoryControlCenter(null);
  const form = {
    querySelector(selector) {
      if (selector === '[data-cad-mmwave-exterior]') return { checked: false };
      return { value: '0' };
    },
  };

  const profile = controlCenter._readProfileFromForm(form);

  assert.equal(profile.mmwave.allow_exterior, false);
});

test('optional point generator follows edited room dimensions', () => {
  const points = generateThreeByThreePoints([8, 2.6, 4]);

  assert.deepEqual(points[0], { id: 'P01', coordinates_m: [2, 0, 1] });
  assert.deepEqual(points[4], { id: 'P05', coordinates_m: [4, 0, 2] });
  assert.deepEqual(points[8], { id: 'P09', coordinates_m: [6, 0, 3] });
});

test('room editor presents mmWave as the primary calibration route', () => {
  const container = { innerHTML: '' };
  const controlCenter = new ObservatoryControlCenter(container);
  controlCenter._mounted = true;
  controlCenter.connectionState = 'ready';
  controlCenter.status = { mmwave: { packets_received: 0 }, nodes: [] };

  controlCenter._render();

  assert.match(container.innerHTML, /Radar-Referenz/);
  assert.match(container.innerHTML, /mmWave \[x \/ y \/ z\]/);
  assert.match(container.innerHTML, /mmwave\.mounting_position_m/);
  assert.match(container.innerHTML, /mmWave öffnen/);
  assert.match(container.innerHTML, /data-occ-action="calculate-mmwave-placement"/);
  assert.match(container.innerHTML, />mmWave-Position berechnen</);
  assert.match(container.innerHTML, /P01–P09-Raster/);
  assert.match(container.innerHTML, /Legacy/);
  assert.doesNotMatch(container.innerHTML, />Trainingspunkte P01–P09/);
});

test('CAD save forwards the complete edited profile to the existing profile submit', () => {
  const form = { querySelector() { return null; } };
  const container = {
    querySelector(selector) {
      return selector === '#occProfileForm' ? form : null;
    },
  };
  const controlCenter = new ObservatoryControlCenter(container);
  const document = defaultSetupProfileDocument();
  document.transmitter.position_m = [1.6, 1.25, 0.45];
  document.receivers[0].position_m = [-0.2, 0.5, 0.28];
  document.mmwave.mounting_position_m = [4.2, 1.2, 1.72];
  let submittedForm = null;
  controlCenter._saveProfile = async (submitted) => { submittedForm = submitted; };

  controlCenter._saveGeometryDocument(document);

  assert.equal(submittedForm, form);
  assert.deepEqual(controlCenter.profileDraft.transmitter.position_m, [1.6, 1.25, 0.45]);
  assert.deepEqual(controlCenter.profileDraft.receivers[0].position_m, [-0.2, 0.5, 0.28]);
  assert.deepEqual(controlCenter.profileDraft.mmwave.mounting_position_m, [4.2, 1.2, 1.72]);
});

test('setup-v2 draft action binds the exact saved CAD profile revision', () => {
  const controlCenter = new ObservatoryControlCenter(null);
  controlCenter.status = { capabilities: { setup_v2_draft_export: true } };
  controlCenter.selectedProfile = {
    id: 'profile-room-1',
    version: 3,
    revision_id: 'profile-room-1-v3',
  };

  const markup = controlCenter._setupV2DraftActionMarkup();

  assert.match(markup, /Setup-v2-Entwurf laden/);
  assert.match(markup, /profile-room-1\/setup-v2-draft\?revision_id=profile-room-1-v3/);
  assert.match(markup, /download="profile-room-1-v3-setup-v2\.draft\.json"/);
});

test('setup-v2 draft action stays hidden until the backend advertises support', () => {
  const controlCenter = new ObservatoryControlCenter(null);
  controlCenter.selectedProfile = { id: 'profile-room-1', version: 3 };

  assert.equal(controlCenter._setupV2DraftActionMarkup(), '');
});

test('position workflow keeps manual P01 capture behind a legacy fallback', () => {
  const controlCenter = new ObservatoryControlCenter(null);
  controlCenter.status = { mmwave: { packets_received: 0 }, recording: { phase: 'idle' } };

  const markup = controlCenter._workflowActions(
    { current_phase: 'train_p01_p09', current_status: 'READY' },
    0,
    0,
    'P01',
    new Set(),
  );

  assert.match(markup, /CSI mit mmWave-Koordinaten kalibrieren/);
  assert.match(markup, /Punktaufnahme/);
  assert.match(markup, /Legacy-Fallback/);
  assert.doesNotMatch(markup, />P01 aufnehmen</);
});

test('empty calibration requires preparation before any measurement can start', () => {
  const controlCenter = new ObservatoryControlCenter(null);
  controlCenter.connectionState = 'ready';

  const markup = controlCenter._workflowActions(
    { current_phase: 'seal_setup', current_status: 'PASS' },
    0,
    0,
    'P01',
    new Set(),
  );

  assert.match(markup, /Leerkalibrierung vorbereiten/);
  assert.match(markup, /data-occ-action="prepare-empty"/);
  assert.doesNotMatch(markup, /data-occ-action="start-empty"/);
});

test('compatible stored calibration is offered before a new empty-room measurement', () => {
  const controlCenter = new ObservatoryControlCenter(null);
  controlCenter.connectionState = 'ready';
  controlCenter.selectedProfile = { id: 'profile-test-1', revision_id: 'profile-test-1-v2' };
  controlCenter.calibrationAvailability = {
    available: true,
    calibration: {
      calibration_id: 'calibration-test-1',
      captured_at: '2026-08-28T12:00:00Z',
      node_count: 3,
    },
  };

  const markup = controlCenter._workflowActions(
    { current_phase: 'seal_setup', current_status: 'PASS' },
    0,
    0,
    'P01',
    new Set(),
  );

  assert.match(markup, /Gespeicherte Leerkalibrierung gefunden/);
  assert.match(markup, /data-occ-action="reuse-empty"/);
  assert.match(markup, /Eine Positions-, Raum-, Firmware-, Grid- oder TX-Filteränderung/);
  assert.match(markup, /data-occ-action="prepare-empty"/);
});

test('empty calibration preparation exposes duration and lead-time controls', () => {
  const controlCenter = new ObservatoryControlCenter(null);
  controlCenter.emptyCalibrationPlan = { phase: 'form', durationSeconds: 120, leadSeconds: 30 };

  const markup = controlCenter._workflowActions(
    { current_phase: 'seal_setup', current_status: 'PASS' },
    0,
    0,
    'P01',
    new Set(),
  );

  assert.match(markup, /id="occEmptyCalibrationPrepareForm"/);
  assert.match(markup, /name="duration_seconds"[^>]+min="60"[^>]+value="120"/);
  assert.match(markup, /name="lead_seconds"[^>]+min="5"[^>]+value="30"/);
  assert.match(markup, /Countdown starten/);
});

test('empty calibration shows the safe return clock during countdown and collection', () => {
  const controlCenter = new ObservatoryControlCenter(null);
  const startsAtMs = Date.now() + 30_000;
  controlCenter.emptyCalibrationPlan = {
    phase: 'countdown',
    durationSeconds: 120,
    leadSeconds: 30,
    startsAtMs,
  };

  const countdownMarkup = controlCenter._emptyCalibrationWorkflowMarkup();

  assert.equal(controlCenter._emptyCalibrationSafeReturnAt(), startsAtMs + 120_000);
  assert.match(countdownMarkup, /Sicher zurück ab \d{2}:\d{2}:\d{2} Uhr/);

  controlCenter.emptyCalibrationPlan = {
    ...controlCenter.emptyCalibrationPlan,
    phase: 'collecting',
    endsAtMs: startsAtMs + 120_000,
  };
  const collectingMarkup = controlCenter._emptyCalibrationWorkflowMarkup();

  assert.match(collectingMarkup, /Sicher zurück ab \d{2}:\d{2}:\d{2} Uhr/);
  assert.match(collectingMarkup, /LEERKALIBRIERUNG ENDET IN/);
});

test('empty calibration rejects a measurement shorter than one minute', () => {
  const controlCenter = new ObservatoryControlCenter(null);
  const form = {
    querySelector(selector) {
      return { value: selector.includes('duration_seconds') ? '59' : '20' };
    },
  };

  controlCenter._scheduleEmptyCalibration(form);

  assert.equal(controlCenter.emptyCalibrationPlan, null);
  assert.match(controlCenter.error, /mindestens 60 Sekunden/);
});

test('empty calibration scheduling creates a countdown with the chosen values', () => {
  const controlCenter = new ObservatoryControlCenter(null);
  const before = Date.now();
  const form = {
    querySelector(selector) {
      return { value: selector.includes('duration_seconds') ? '120' : '30' };
    },
  };

  controlCenter._scheduleEmptyCalibration(form);

  assert.equal(controlCenter.emptyCalibrationPlan.phase, 'countdown');
  assert.equal(controlCenter.emptyCalibrationPlan.durationSeconds, 120);
  assert.equal(controlCenter.emptyCalibrationPlan.leadSeconds, 30);
  assert.ok(controlCenter.emptyCalibrationPlan.startsAtMs >= before + 29_000);
  controlCenter._clearEmptyCalibrationTimer();
});

test('countdown start activates the server only after the countdown phase', async () => {
  const controlCenter = new ObservatoryControlCenter(null);
  controlCenter._render = () => {};
  controlCenter.selectedProfile = { id: 'profile-test-1', revision_id: 'profile-test-1-v1' };
  controlCenter.emptyCalibrationPlan = {
    phase: 'countdown',
    durationSeconds: 120,
    leadSeconds: 30,
    startsAtMs: Date.now() + 30_000,
  };
  const originalPost = apiService.post;
  let startEndpoint = null;
  let startBody = null;
  let advanceArgs = null;
  apiService.post = async (endpoint, body) => {
    startEndpoint = endpoint;
    startBody = body;
    return { success: true };
  };
  controlCenter._advance = async (...args) => { advanceArgs = args; };

  try {
    await controlCenter._startEmptyCalibration();
  } finally {
    apiService.post = originalPost;
    controlCenter._clearEmptyCalibrationTimer();
  }

  assert.equal(startEndpoint, '/api/v1/classification/calibration/start');
  assert.deepEqual(startBody, {
    profile_id: 'profile-test-1',
    profile_revision_id: 'profile-test-1-v1',
  });
  assert.equal(controlCenter.emptyCalibrationPlan.phase, 'collecting');
  assert.equal(controlCenter.emptyCalibrationPlan.durationSeconds, 120);
  assert.deepEqual(advanceArgs, [
    'empty_calibration',
    'RUNNING',
    {
      calibration_kind: 'wifi_d5_d6',
      requested_duration_seconds: 120,
      start_delay_seconds: 30,
    },
  ]);
});

test('automatic calibration completion stops the server and advances the workflow', async () => {
  const controlCenter = new ObservatoryControlCenter(null);
  controlCenter._render = () => {};
  controlCenter.emptyCalibrationPlan = {
    phase: 'collecting',
    durationSeconds: 60,
    leadSeconds: 20,
    endsAtMs: Date.now() - 1,
  };
  const originalPost = apiService.post;
  let stopEndpoint = null;
  let completionArgs = null;
  apiService.post = async (endpoint) => {
    stopEndpoint = endpoint;
    return { success: true, status: 'ready' };
  };
  controlCenter._completePhaseAndOpenNext = async (...args) => { completionArgs = args; };

  try {
    await controlCenter._stopEmptyCalibration({ automatic: true });
  } finally {
    apiService.post = originalPost;
    controlCenter._clearEmptyCalibrationTimer();
  }

  assert.equal(stopEndpoint, '/api/v1/classification/calibration/stop');
  assert.equal(controlCenter.emptyCalibrationPlan, null);
  assert.equal(completionArgs[0], 'empty_calibration');
  assert.equal(completionArgs[1], 'train_p01_p09');
  assert.equal(completionArgs[2].automatic_completion, true);
  assert.equal(completionArgs[2].requested_duration_seconds, 60);
});

test('stored calibration reuse advances the workflow without starting measurement', async () => {
  const controlCenter = new ObservatoryControlCenter(null);
  controlCenter.selectedRun = { id: 'run-test-1' };
  controlCenter.selectedProfile = { id: 'profile-test-1', revision_id: 'profile-test-1-v2' };
  controlCenter._render = () => {};
  controlCenter.refresh = async () => {};
  const originalReuse = experimentService.reuseCalibration;
  const originalAdvance = experimentService.advancePhase;
  const reuseArgs = [];
  const advanceArgs = [];
  experimentService.reuseCalibration = async (args) => {
    reuseArgs.push(args);
    return {
      success: true,
      calibration_id: 'calibration-test-1',
      calibration_context_sha256: 'b'.repeat(64),
    };
  };
  experimentService.advancePhase = async (id, args) => {
    advanceArgs.push({ id, ...args });
    return { id, workflow: { current_phase: args.phase, current_status: args.status } };
  };

  try {
    await controlCenter._reuseEmptyCalibration();
  } finally {
    experimentService.reuseCalibration = originalReuse;
    experimentService.advancePhase = originalAdvance;
  }

  assert.deepEqual(reuseArgs, [{ profileId: 'profile-test-1', profileRevisionId: 'profile-test-1-v2' }]);
  assert.equal(advanceArgs.length, 2);
  assert.equal(advanceArgs[0].phase, 'empty_calibration');
  assert.equal(advanceArgs[0].status, 'REUSED');
  assert.equal(advanceArgs[0].payload.calibration_source, 'reused');
  assert.equal(advanceArgs[0].payload.reused_without_measurement, true);
  assert.equal(advanceArgs[1].phase, 'train_p01_p09');
  assert.match(controlCenter.message, /Keine neue Leeraumkalibrierung nötig/);
});

test('guide starts with setup navigation before a run exists', () => {
  const controlCenter = new ObservatoryControlCenter(null);

  const markup = controlCenter._guideMarkup();

  assert.match(markup, /Setup-Profil speichern/);
  assert.match(markup, /data-occ-action="focus-profile"/);
  assert.match(markup, /Maße prüfen/);
});

test('runtime setup seal binds the active setup identity', async () => {
  const controlCenter = new ObservatoryControlCenter(null);
  controlCenter.selectedRun = { id: 'run-hardware-1' };
  controlCenter.status = {
    position_setup: {
      active: true,
      setup_id: 'setup-runtime-1',
      setup_sha256: 'b'.repeat(64),
    },
  };
  let advanceCall = null;
  controlCenter._advance = async (...args) => { advanceCall = args; };

  const actionButton = { dataset: { occAction: 'seal' } };
  await controlCenter._onClick({
    target: {
      closest: (selector) => selector === '[data-occ-action]' ? actionButton : null,
    },
  });

  assert.deepEqual(advanceCall, [
    'seal_setup',
    'PASS',
    { setup_id: 'setup-runtime-1', setup_sha256: 'b'.repeat(64) },
  ]);
});

test('setup seal stays locked without an active runtime setup', async () => {
  const controlCenter = new ObservatoryControlCenter(null);
  controlCenter.selectedRun = {
    id: 'run-locked-1',
    workflow: { current_phase: 'create_experiment', current_status: 'READY' },
  };
  controlCenter.connectionState = 'ready';
  let advanceCalled = false;
  controlCenter._advance = async () => { advanceCalled = true; };

  const markup = controlCenter._workflowActions(
    controlCenter.selectedRun.workflow,
    0,
    0,
    'P01',
    new Set(),
  );
  await controlCenter._sealSetup();

  assert.match(markup, /Runtime-Setup fehlt/);
  assert.match(markup, /disabled/);
  assert.equal(advanceCalled, false);
  assert.match(controlCenter.error, /ohne aktives Setup-v2/);
});

test('legacy workflow seal without runtime identity cannot continue', () => {
  const controlCenter = new ObservatoryControlCenter(null);
  controlCenter.connectionState = 'ready';
  controlCenter.status = {
    position_setup: {
      active: true,
      setup_id: 'setup-runtime-1',
      setup_sha256: 'b'.repeat(64),
    },
  };
  controlCenter.selectedRun = {
    id: 'run-legacy-seal',
    workflow: {
      current_phase: 'train_p01_p09',
      current_status: 'READY',
      profile_sha256: 'a'.repeat(64),
      events: [{
        phase: 'seal_setup',
        status: 'PASS',
        payload: { profile_sha256: 'a'.repeat(64) },
      }],
    },
  };

  const guide = controlCenter._guideMarkup();
  const actions = controlCenter._workflowActions(
    controlCenter.selectedRun.workflow,
    0,
    0,
    'P01',
    new Set(),
  );

  assert.match(guide, /Run besitzt kein gültiges Runtime-Seal/);
  assert.match(guide, /data-occ-action="clear-run"/);
  assert.match(actions, /Run ohne gültiges Runtime-Seal/);
  assert.doesNotMatch(actions, /data-occ-action="open-mmwave-calibration"/);
});

test('guide routes the active calibration phase to the mmWave assistant', () => {
  const controlCenter = new ObservatoryControlCenter(null);
  controlCenter.connectionState = 'ready';
  controlCenter.status = {
    position_setup: {
      active: true,
      setup_id: 'setup-runtime-1',
      setup_sha256: 'b'.repeat(64),
    },
  };
  controlCenter.selectedRun = {
    workflow: {
      current_phase: 'train_p01_p09',
      current_status: 'READY',
      profile_sha256: 'a'.repeat(64),
      events: [{
        phase: 'seal_setup',
        status: 'PASS',
        payload: {
          seal_kind: 'active_position_setup_v2',
          profile_sha256: 'a'.repeat(64),
          setup_id: 'setup-runtime-1',
          setup_sha256: 'b'.repeat(64),
        },
      }],
    },
  };

  const markup = controlCenter._guideMarkup();

  assert.match(markup, /PHASE 04 \/ 10/);
  assert.match(markup, /CSI mit mmWave-Referenz kalibrieren/);
  assert.match(markup, /data-occ-action="open-mmwave-calibration"/);
  assert.match(markup, /P01–P09 müssen nicht manuell aufgenommen werden/);
});

test('guide promotes the next workflow action after a completed phase', () => {
  const controlCenter = new ObservatoryControlCenter(null);
  controlCenter.connectionState = 'ready';
  controlCenter.status = {
    position_setup: {
      active: true,
      setup_id: 'setup-runtime-1',
      setup_sha256: 'b'.repeat(64),
    },
  };
  controlCenter.selectedRun = {
    workflow: {
      current_phase: 'empty_calibration',
      current_status: 'PASS',
      profile_sha256: 'a'.repeat(64),
      events: [{
        phase: 'seal_setup',
        status: 'PASS',
        payload: {
          seal_kind: 'active_position_setup_v2',
          profile_sha256: 'a'.repeat(64),
          setup_id: 'setup-runtime-1',
          setup_sha256: 'b'.repeat(64),
        },
      }],
    },
  };

  const markup = controlCenter._guideMarkup();

  assert.match(markup, /mmWave-Kalibrierung vorbereiten/);
  assert.match(markup, /data-occ-action="open-training"/);
  assert.match(markup, /Baseline abgeschlossen/);
});

test('guide labels synthetic walkthrough phases as unvalidated', () => {
  const controlCenter = new ObservatoryControlCenter(null);
  controlCenter.connectionState = 'ready';
  controlCenter.selectedRun = {
    workflow: {
      current_phase: 'predict',
      current_status: 'READY',
      events: [{ payload: { software_only: true, demo: 'guide walkthrough only' } }],
    },
  };

  const markup = controlCenter._guideMarkup();

  assert.match(markup, /PHASE OFFEN|BEREIT · DEMO/);
  assert.match(markup, /softwareseitig als abgeschlossen/);
  assert.match(markup, /SOFTWARE-ONLY \/ UNVALIDATED/);
  assert.doesNotMatch(markup, /Die Prediction ist registriert\. Öffne jetzt/);
});

test('benchmark panel has no hidden score claims before WiFi data exists', () => {
  const controlCenter = new ObservatoryControlCenter(null);
  controlCenter.models = [];
  controlCenter.status = { active_model_id: null };
  controlCenter.benchmarkCatalog = {
    status: 'READY_FOR_WIFI_DATA',
    baseline: { id: 'prototype_d6' },
    comparators: [{ id: 'knn' }, { id: 'svm' }],
    split: { id: 'sealed_wifi_train_blind_test_v1' },
    rx_ablation: [{ id: 'rx1_rx2_rx3_rx4' }],
  };

  const markup = controlCenter._benchmarkMarkup();
  assert.match(markup, /READY_FOR_WIFI_DATA|Bereit\./);
  assert.match(markup, /prototype_d6/);
  assert.match(markup, /sealed_wifi_train_blind_test_v1/);
  assert.match(markup, /keine Modellwerte/);
  assert.doesNotMatch(markup, /accuracy\s*[:=]\s*0\./i);
});

test('unclear cockpit metrics expose keyboard-focusable explanations', () => {
  const controlCenter = new ObservatoryControlCenter(null);
  controlCenter.status = {
    nodes: [],
    classification_calibration: { phase: 'ready' },
    mmwave: { packets_received: 0 },
  };

  const markup = controlCenter._overviewMarkup();
  assert.match(markup, /class="occ-info" tabindex="0" role="note"/);
  assert.match(markup, /Online-Zahl im letzten Status/);
  assert.match(markup, /kein WiFi-Feature/);
});

test('artifact inputs explain the prediction-truth boundary', () => {
  const controlCenter = new ObservatoryControlCenter(null);

  const markup = controlCenter._artifactForm('truth', 'Truth-Artefakt', 'position-truth.json');
  assert.match(markup, /relativer Pfad unter data\//);
  assert.match(markup, /Truth enthält die später aufgedeckten echten Positionen/);
  assert.match(markup, /Prüfen & registrieren/);
});

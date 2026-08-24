import assert from 'node:assert/strict';
import test from 'node:test';

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
  assert.deepEqual(profile.points.map((point) => point.id), [
    'P01', 'P02', 'P03', 'P04', 'P05', 'P06', 'P07', 'P08', 'P09',
  ]);
  assert.equal(profile.mmwave_status, 'NOT_CONNECTED');
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
  assert.match(container.innerHTML, /mmWave öffnen/);
  assert.match(container.innerHTML, /P01–P09-Raster/);
  assert.match(container.innerHTML, /Legacy/);
  assert.doesNotMatch(container.innerHTML, />Trainingspunkte P01–P09/);
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

test('guide starts with setup navigation before a run exists', () => {
  const controlCenter = new ObservatoryControlCenter(null);

  const markup = controlCenter._guideMarkup();

  assert.match(markup, /Setup-Profil speichern/);
  assert.match(markup, /data-occ-action="focus-profile"/);
  assert.match(markup, /Maße prüfen/);
});

test('guide routes the active calibration phase to the mmWave assistant', () => {
  const controlCenter = new ObservatoryControlCenter(null);
  controlCenter.connectionState = 'ready';
  controlCenter.selectedRun = {
    workflow: { current_phase: 'train_p01_p09', current_status: 'READY' },
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
  controlCenter.selectedRun = {
    workflow: { current_phase: 'empty_calibration', current_status: 'PASS' },
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

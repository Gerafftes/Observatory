import assert from 'node:assert/strict';
import test from 'node:test';

import {
  ExperimentConsole,
  SYNTHETIC_FIXTURE_ID,
  experimentConsoleViewModel,
} from '../components/ExperimentConsole.js';

test('control center stays locked when SQLite persistence is unavailable', () => {
  const model = experimentConsoleViewModel({
    available: false,
    status: 'PERSISTENCE_UNAVAILABLE',
  });

  assert.equal(model.available, false);
  assert.equal(model.canStart, false);
  assert.equal(model.statusLabel, 'PERSISTENCE UNAVAILABLE');
  assert.equal(model.validationLabel, 'UNVALIDATED');
  assert.equal(model.livePositionApproved, false);
});

test('ready replay model exposes the explicit evidence boundary', () => {
  const run = {
    id: 'run-1',
    state: 'completed',
    execution_status: 'PASS',
    validation_status: 'UNVALIDATED',
    live_position_approved: false,
  };
  const model = experimentConsoleViewModel(
    { available: true, status: 'READY', run_count: 1 },
    [run],
    run,
  );

  assert.equal(model.canStart, true);
  assert.equal(model.runCount, 1);
  assert.equal(model.selectedRun, run);
  assert.equal(model.validationLabel, 'UNVALIDATED');
  assert.equal(model.livePositionApproved, false);
});

test('console shell contains replay-only controls and no live capture action', () => {
  const console = new ExperimentConsole({});
  const shell = console._shell();

  assert.match(shell, /Synthetic Replay starten/);
  assert.match(shell, new RegExp(SYNTHETIC_FIXTURE_ID));
  assert.match(shell, /SOFTWARE REPLAY/);
  assert.match(shell, /UNVALIDATED/);
  assert.match(shell, /NO LIVE HARDWARE/);
  assert.doesNotMatch(shell, /Aufnahme starten/);
  assert.doesNotMatch(shell, /OTA/);
});

import assert from 'node:assert/strict';
import test from 'node:test';

import TrainingPanel, { trainingCapability } from '../components/TrainingPanel.js';
import { trainingService } from '../services/training.service.js';

test('training actions require an explicit server capability', () => {
  const status = {
    capabilities: {
      start: false,
      pretrain: true,
    },
  };

  assert.equal(trainingCapability(status, 'start'), false);
  assert.equal(trainingCapability(status, 'pretrain'), true);
  assert.equal(trainingCapability(status, 'lora'), false);
  assert.equal(trainingCapability(null, 'start'), false);
});

test('unavailable training neither starts work nor opens a progress socket', async () => {
  const panel = Object.create(TrainingPanel.prototype);
  panel.state = {
    recordings: [],
    trainingStatus: {
      active: false,
      message: 'Training is unavailable.',
      capabilities: {
        start: false,
        progress_websocket: false,
      },
    },
  };
  panel.config = { selectedRecordings: [] };
  panel.progressData = { losses: [], pcks: [] };
  panel._set = patch => Object.assign(panel.state, patch);

  const originalConnect = trainingService.connectProgressStream;
  const originalStart = trainingService.startTraining;
  let connected = false;
  let started = false;
  trainingService.connectProgressStream = () => { connected = true; };
  trainingService.startTraining = async () => { started = true; };

  try {
    await panel._launchTraining('startTraining', 'start');
  } finally {
    trainingService.connectProgressStream = originalConnect;
    trainingService.startTraining = originalStart;
  }

  assert.equal(connected, false);
  assert.equal(started, false);
  assert.equal(panel.state.error, 'Training is unavailable.');
});

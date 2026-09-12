import assert from 'node:assert/strict';
import test from 'node:test';

import { trainingCapability } from '../components/TrainingPanel.js';

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

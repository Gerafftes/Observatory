'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');

function iqHex() {
  const bytes = [0, 0];
  for (let index = 0; index < 64; index++) bytes.push(3, 4);
  return Buffer.from(bytes).toString('hex');
}

test('aligner emits synthetic torso-v1 amplitude and sanitized phase channels', () => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'torso-v1-'));
  const gtPath = path.join(directory, 'gt.jsonl');
  const csiPath = path.join(directory, 'csi.jsonl');
  const outputPath = path.join(directory, 'paired.jsonl');
  const baseMs = Date.parse('2026-08-15T10:00:00.000Z');
  const keypoints = Array.from({ length: 17 }, (_, index) => [index / 20, index / 25, 0.95]);
  const gt = Array.from({ length: 5 }, (_, index) => JSON.stringify({
    ts_ns: (baseMs + index * 20) * 1e6,
    keypoints,
    confidence: 0.95,
  })).join('\n');
  fs.writeFileSync(gtPath, `${gt}\n`);
  const csi = [];
  for (let time = 0; time < 20; time++) {
    for (let rx = 1; rx <= 4; rx++) {
      csi.push(JSON.stringify({
        type: 'raw_csi',
        timestamp: new Date(baseMs + time * 4 + rx).toISOString(),
        node_id: rx,
        subcarriers: 64,
        iq_hex: iqHex(),
        seq: time,
      }));
    }
  }
  fs.writeFileSync(csiPath, `${csi.join('\n')}\n`);

  const result = spawnSync(process.execPath, [
    path.resolve(__dirname, '..', 'align-ground-truth.js'),
    '--gt', gtPath,
    '--csi', csiPath,
    '--output', outputPath,
    '--task', 'torso',
    '--session-id', 'fixture-session-a',
  ], { encoding: 'utf8' });
  assert.equal(result.status, 0, result.stderr);
  const sample = JSON.parse(fs.readFileSync(outputPath, 'utf8'));
  assert.equal(sample.schema_version, 'torso-v1');
  assert.equal(sample.validation_status, 'UNVALIDATED');
  assert.deepEqual(sample.rx_order, ['RX1', 'RX2', 'RX3', 'RX4']);
  assert.deepEqual(sample.csi_shape, [20, 4, 64]);
  assert.equal(sample.csi_amplitude.length, 20 * 4 * 64);
  assert.equal(sample.csi_phase.length, 20 * 4 * 64);
  assert.equal(sample.phase_status, 'sanitized');
  assert.equal(sample.torso.left_shoulder.confidence, 0.95);
});

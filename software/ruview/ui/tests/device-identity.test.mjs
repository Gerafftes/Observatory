import assert from 'node:assert/strict';
import fs from 'node:fs';
import test from 'node:test';

import {
  DEVICE_IDENTITIES,
  UNKNOWN_DEVICE_COLOR,
  deviceColor,
  deviceIdentity,
  receiverIdentity,
  receiverLabel,
} from '../device-identity.js';

test('known RX wire IDs map to stable identities and distinct colors', () => {
  const colors = [1, 2, 3, 4].map((wireId) => receiverIdentity(wireId)?.color);

  assert.deepEqual([1, 2, 3, 4].map(receiverLabel), ['RX1', 'RX2', 'RX3', 'RX4']);
  assert.equal(new Set(colors).size, 4);
  assert.deepEqual(colors, [
    DEVICE_IDENTITIES.RX1.color,
    DEVICE_IDENTITIES.RX2.color,
    DEVICE_IDENTITIES.RX3.color,
    DEVICE_IDENTITIES.RX4.color,
  ]);
});

test('TX is the only legacy alias and unknown devices stay unassigned', () => {
  assert.equal(deviceIdentity('TX'), DEVICE_IDENTITIES.TX1);
  assert.equal(deviceIdentity('TX1'), DEVICE_IDENTITIES.TX1);
  assert.equal(receiverIdentity(5), null);
  assert.equal(deviceIdentity('RX5'), null);
  assert.equal(deviceColor('RX5'), UNKNOWN_DEVICE_COLOR);
});

test('firmware and UI share the exact canonical RGB palette', () => {
  const rxSource = fs.readFileSync(
    new URL('../../firmware/esp32-csi-node/main/node_identity.c', import.meta.url),
    'utf8',
  );
  const txSource = fs.readFileSync(
    new URL('../../firmware/esp32-csi-tx/esp32-csi-tx.ino', import.meta.url),
    'utf8',
  );

  for (const identity of Object.values(DEVICE_IDENTITIES).filter(({ role }) => role === 'receiver')) {
    const [red, green, blue] = identity.color.match(/[0-9a-f]{2}/gi).map((channel) => Number.parseInt(channel, 16));
    const firmwareEntry = new RegExp(
      `label = "${identity.id}", \\.red = ${red}, \\.green = ${green}, \\.blue = ${blue}`,
    );
    assert.match(rxSource, firmwareEntry);
  }

  assert.match(txSource, /kIdentityRed = 255/);
  assert.match(txSource, /kIdentityGreen = 159/);
  assert.match(txSource, /kIdentityBlue = 10/);
  assert.equal(DEVICE_IDENTITIES.TX1.color, '#ff9f0a');
});

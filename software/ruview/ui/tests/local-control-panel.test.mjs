import assert from 'node:assert/strict';
import test from 'node:test';

import { LocalControlPanel } from '../components/LocalControlPanel.js';

test('local control panel exposes browser discovery and serial-port actions', () => {
  const container = { innerHTML: '' };
  const panel = new LocalControlPanel(container);
  panel._mounted = true;
  panel.available = true;
  panel.info = { helper_version: '0.3.0' };
  panel.nodes = [{ ip: '192.168.4.2', mac: 'AA:BB:CC:DD:EE:FF' }];
  panel.ports = [{ name: '/dev/cu.usbserial', is_esp32_compatible: true }];

  panel._render();

  assert.match(container.innerHTML, /Lokale Hardware-Steuerung/);
  assert.match(container.innerHTML, /data-helper-action="discover"/);
  assert.match(container.innerHTML, /data-helper-action="ports"/);
  assert.match(container.innerHTML, /192\.168\.4\.2/);
  assert.match(container.innerHTML, /ESP32-kompatibel/);
});

test('local control panel keeps a clear helper-offline state', () => {
  const container = { innerHTML: '' };
  const panel = new LocalControlPanel(container);
  panel._mounted = true;
  panel.available = false;
  panel.error = 'Failed to fetch';

  panel._render();

  assert.match(container.innerHTML, /NICHT ERREICHBAR/);
  assert.match(container.innerHTML, /Nodes suchen/);
  assert.match(container.innerHTML, /Failed to fetch/);
  assert.match(container.innerHTML, /data-helper-action="discover" disabled/);
});

import assert from 'node:assert/strict';
import test from 'node:test';

import { ServerControlPanel } from '../components/ServerControlPanel.js';

test('server control panel exposes browser start, restart, stop, and refresh actions', () => {
  const container = { innerHTML: '' };
  const panel = new ServerControlPanel(container);
  panel._mounted = true;
  panel.available = true;
  panel.status = {
    running: true,
    pid: 1234,
    ui_url: 'http://localhost:8080/ui/index.html#sensing',
  };

  panel._render();

  assert.match(container.innerHTML, /Serversteuerung/);
  assert.match(container.innerHTML, /data-server-action="start"/);
  assert.match(container.innerHTML, /data-server-action="restart"/);
  assert.match(container.innerHTML, /data-server-action="stop"/);
  assert.match(container.innerHTML, /data-server-action="refresh"/);
  assert.match(container.innerHTML, /LÄUFT · PID 1234/);
  assert.match(container.innerHTML, /data-server-action="start" disabled/);
});

test('server control panel keeps the actions visible when the helper is offline', () => {
  const container = { innerHTML: '' };
  const panel = new ServerControlPanel(container);
  panel._mounted = true;
  panel.available = false;
  panel.error = 'Failed to fetch';

  panel._render();

  assert.match(container.innerHTML, /NICHT ERREICHBAR/);
  assert.match(container.innerHTML, /Sensing-Server einmal mit der aktuellen Binary/);
  assert.match(container.innerHTML, /data-server-action="start" disabled/);
  assert.match(container.innerHTML, /Failed to fetch/);
});

test('server control panel disables mutations while the helper reports a busy action', () => {
  const container = { innerHTML: '' };
  const panel = new ServerControlPanel(container);
  panel._mounted = true;
  panel.available = true;
  panel.status = {
    running: false,
    busy: true,
    last_action: 'stop',
  };

  panel._render();

  assert.match(container.innerHTML, /AKTION LÄUFT/);
  assert.match(container.innerHTML, /data-server-action="start" disabled/);
  assert.match(container.innerHTML, /data-server-action="restart" disabled/);
  assert.match(container.innerHTML, /data-server-action="stop" disabled/);
  assert.doesNotMatch(container.innerHTML, /data-server-action="refresh" disabled/);
});

test('server control panel surfaces a process error returned by the helper', async () => {
  const container = { innerHTML: '' };
  const panel = new ServerControlPanel(container);
  panel._mounted = true;
  panel.message = 'Sensing-Server wurde gestartet.';

  const originalFetch = globalThis.fetch;
  globalThis.fetch = async () => ({
    ok: true,
    json: async () => ({
      running: false,
      error: 'Sensing-Server beendet mit exit status: 1',
    }),
  });

  try {
    await panel.refresh({ quiet: true });
  } finally {
    globalThis.fetch = originalFetch;
  }

  assert.match(container.innerHTML, /Sensing-Server beendet mit exit status: 1/);
  assert.doesNotMatch(container.innerHTML, /Sensing-Server wurde gestartet\./);
});

test('server start notifies the control center to reload stored setup profiles', async () => {
  const container = { innerHTML: '' };
  const actions = [];
  const panel = new ServerControlPanel(container, {
    onServerAction: async (action) => actions.push(action),
  });
  panel._mounted = true;
  panel.available = true;

  const originalFetch = globalThis.fetch;
  let requestCount = 0;
  globalThis.fetch = async (_, options = {}) => {
    requestCount += 1;
    if (options.method === 'POST') {
      return { ok: true, json: async () => ({ message: 'Sensing-Server wurde gestartet.' }) };
    }
    return { ok: true, json: async () => ({ running: true, pid: 1234 }) };
  };

  try {
    await panel._runAction('start');
  } finally {
    globalThis.fetch = originalFetch;
  }

  assert.deepEqual(actions, ['start']);
  assert.equal(requestCount, 2);
});

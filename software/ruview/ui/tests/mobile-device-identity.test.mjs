import assert from 'node:assert/strict';
import fs from 'node:fs';
import test from 'node:test';

import { DEVICE_IDENTITIES, UNKNOWN_DEVICE_THREE_COLOR } from '../device-identity.js';

const mobileView = fs.readFileSync(
  new URL('../mobile/src/assets/webview/gaussian-splats.html', import.meta.url),
  'utf8',
);

test('mobile 3D view uses the canonical device colors', () => {
  for (const { id, threeColor } of Object.values(DEVICE_IDENTITIES)) {
    const hex = threeColor.toString(16).padStart(6, '0');
    assert.match(mobileView, new RegExp(`${id}:\\s*0x${hex}\\b`));
  }
  assert.match(mobileView, new RegExp(`UNKNOWN_DEVICE_COLOR\\s*=\\s*0x${UNKNOWN_DEVICE_THREE_COLOR.toString(16)}\\b`));
});

test('mobile 3D view keeps one marker per stable node identity', () => {
  assert.match(mobileView, /this\.nodeMarkers\s*=\s*new Map\(\)/);
  assert.match(mobileView, /const identity = canonicalNodeId\(node\)/);
  assert.match(mobileView, /this\.nodeMarkers\.get\(key\)/);
  assert.doesNotMatch(mobileView, /this\.nodeMarker\s*=/);
});

test('mobile 3D inline script parses', () => {
  const start = mobileView.lastIndexOf('<script>');
  const end = mobileView.lastIndexOf('</script>');

  assert.ok(start >= 0 && end > start, 'inline script is present');
  assert.doesNotThrow(() => new Function(mobileView.slice(start + 8, end)));
});

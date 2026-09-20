import assert from 'node:assert/strict';
import fs from 'node:fs';
import test from 'node:test';

const mobileView = fs.readFileSync(
  new URL('../mobile/src/assets/webview/gaussian-splats.html', import.meta.url),
  'utf8',
);

test('mobile 3D view uses the canonical device colors', () => {
  assert.match(mobileView, /TX1:\s*0xff9f0a/);
  assert.match(mobileView, /RX1:\s*0x0a84ff/);
  assert.match(mobileView, /RX2:\s*0x30d158/);
  assert.match(mobileView, /RX3:\s*0xff375f/);
  assert.match(mobileView, /RX4:\s*0xbf5af2/);
  assert.match(mobileView, /UNKNOWN_DEVICE_COLOR\s*=\s*0x68737a/);
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

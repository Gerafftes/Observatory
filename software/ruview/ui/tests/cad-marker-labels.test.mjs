import assert from 'node:assert/strict';
import test from 'node:test';

import { layoutCadMarkerLabels } from '../components/RoomGeometryEditor.js';

function label(x, y, width = 24) {
  return { x, y, box: { x: 14, y: -19, width, height: 12 } };
}

function assertSeparated(labels, positions) {
  const bounds = labels.map(({ x, y, box }, index) => ({
    x: x + box.x + positions[index].x - 14 - 3,
    y: y + box.y + positions[index].y + 10 - 3,
    width: box.width + 6,
    height: box.height + 6,
  }));
  bounds.forEach((box, index) => {
    assert.ok(box.x >= 0 && box.y >= 0 && box.x + box.width <= 980 && box.y + box.height <= 600);
    bounds.slice(index + 1).forEach((other) => {
      assert.ok(box.x + box.width <= other.x || other.x + other.width <= box.x
        || box.y + box.height <= other.y || other.y + other.height <= box.y,
      `labels ${index} and ${bounds.indexOf(other)} overlap`);
    });
  });
}

test('coincident RX and calibration labels separate while the RX label stays in place', () => {
  const labels = [label(200, 250), label(200, 250, 40)];
  const before = structuredClone(labels);
  const positions = layoutCadMarkerLabels(labels);
  assert.deepEqual(positions[0], { x: 14, y: -10 });
  assertSeparated(labels, positions);
  assert.deepEqual(labels, before, 'layout must never move either physical point');
});

test('nearby labels separate even when their point coordinates differ', () => {
  const labels = [label(200, 250), label(216, 255, 40)];
  assertSeparated(labels, layoutCadMarkerLabels(labels));
});

test('labels return to their ordinary offset when moving apart', () => {
  const labels = [label(200, 250), label(200, 250, 40)];
  assert.notDeepEqual(layoutCadMarkerLabels(labels)[1], { x: 14, y: -10 });
  labels[1].x = 350;
  assert.deepEqual(layoutCadMarkerLabels(labels), [{ x: 14, y: -10 }, { x: 14, y: -10 }]);
});

test('crowded labels remain separate and inside the SVG at its edges', () => {
  for (const [x, y] of [[0, 0], [975, 595], [250, 250]]) {
    const labels = Array.from({ length: 12 }, (_, index) => label(x, y, index < 7 ? 24 : 42));
    const positions = layoutCadMarkerLabels(labels);
    assertSeparated(labels, positions);
    assert.deepEqual(layoutCadMarkerLabels(labels), positions, 'placement is stable across redraws');
  }
});

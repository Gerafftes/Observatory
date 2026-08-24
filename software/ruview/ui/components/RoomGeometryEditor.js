const VIEWBOX = Object.freeze({
  width: 980,
  height: 600,
  plot: { x: 92, y: 62, width: 700, height: 420 },
});

const DEFAULT_ROOM = [4.02, 2.59, 3.44];
export const DEFAULT_SENSOR_MOUNT_RADIUS_M = 0.5;
export const MAX_SENSOR_MOUNT_RADIUS_M = 5;
const EDITABLE_IDS = ['TX', 'RX1', 'RX2', 'RX3', 'RX4'];
const WALL_IDS = ['WALL_X0', 'WALL_XMAX', 'WALL_Z0', 'WALL_ZMAX'];
const SELECTABLE_IDS = [...EDITABLE_IDS, ...WALL_IDS];

const WALL_LABELS = Object.freeze({
  WALL_X0: 'Wand X = 0',
  WALL_XMAX: 'Wand X = L',
  WALL_Z0: 'Wand Z = 0',
  WALL_ZMAX: 'Wand Z = B',
});

function escapeHTML(value) {
  return String(value ?? '')
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#039;');
}

function numberValue(value, fallback = 0) {
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : fallback;
}

function clone(value) {
  return typeof structuredClone === 'function'
    ? structuredClone(value)
    : JSON.parse(JSON.stringify(value));
}

function vector3(value, fallback = [0, 0, 0]) {
  return [0, 1, 2].map((index) => numberValue(value?.[index], fallback[index]));
}

function roomDimensions(document) {
  const room = vector3(document?.room_dimensions_m, DEFAULT_ROOM);
  return room.map((value, index) => value > 0 ? value : DEFAULT_ROOM[index]);
}

export function sensorMountRadius(document) {
  const value = Number(document?.sensor_mount_radius_m);
  return Number.isFinite(value) && value >= 0
    ? Math.min(value, MAX_SENSOR_MOUNT_RADIUS_M)
    : DEFAULT_SENSOR_MOUNT_RADIUS_M;
}

export function sensorOutsideDistance(position, room) {
  const dimensions = vector3(room, DEFAULT_ROOM);
  const x = numberValue(position?.[0]);
  const z = numberValue(position?.[2]);
  const dx = x < 0 ? -x : Math.max(0, x - dimensions[0]);
  const dz = z < 0 ? -z : Math.max(0, z - dimensions[2]);
  return Math.hypot(dx, dz);
}

function sensorPositionWithinBounds(position, room, radius) {
  return numberValue(position?.[1]) >= 0
    && numberValue(position?.[1]) <= room[1]
    && sensorOutsideDistance(position, room) <= radius + 0.000001;
}

function entityPosition(document, id) {
  if (id === 'TX') return vector3(document?.transmitter?.position_m);
  return vector3(document?.receivers?.find((receiver) => receiver.id === id)?.position_m);
}

function updateEntityPosition(document, id, position) {
  const next = clone(document || {});
  if (id === 'TX') {
    next.transmitter = {
      ...(next.transmitter || { id: 'TX' }),
      position_m: [...position],
    };
  } else {
    next.receivers = (next.receivers || []).map((receiver) => receiver.id === id
      ? { ...receiver, position_m: [...position] }
      : receiver);
  }
  return next;
}

function updateRoomDimensions(document, dimensions) {
  const next = clone(document || {});
  next.room_dimensions_m = [...dimensions];
  return next;
}

export function planDistance(first, second) {
  return Math.hypot(
    numberValue(first?.[0]) - numberValue(second?.[0]),
    numberValue(first?.[2]) - numberValue(second?.[2]),
  );
}

export function setPlanDistance(document, anchorId, movingId, requestedDistance) {
  const distance = Number(requestedDistance);
  if (!EDITABLE_IDS.includes(anchorId) || !EDITABLE_IDS.includes(movingId) || anchorId === movingId) {
    return { document: clone(document || {}), error: 'Für einen Abstand müssen zwei verschiedene Marker ausgewählt sein.' };
  }
  if (!Number.isFinite(distance) || distance <= 0) {
    return { document: clone(document || {}), error: 'Der Abstand muss ein endlicher Wert größer als 0 sein.' };
  }

  const room = vector3(document?.room_dimensions_m, [0, 0, 0]);
  if (room.some((value) => !Number.isFinite(value) || value <= 0)) {
    return { document: clone(document || {}), error: 'Der Abstand kann erst bei gültigen Raummaßen gesetzt werden.' };
  }

  const anchor = entityPosition(document, anchorId);
  const moving = entityPosition(document, movingId);
  const deltaX = moving[0] - anchor[0];
  const deltaZ = moving[2] - anchor[2];
  const currentDistance = Math.hypot(deltaX, deltaZ);
  const direction = currentDistance > 0.000001
    ? [deltaX / currentDistance, deltaZ / currentDistance]
    : [1, 0];
  const candidate = [
    anchor[0] + direction[0] * distance,
    moving[1],
    anchor[2] + direction[1] * distance,
  ];
  if (!sensorPositionWithinBounds(candidate, room, sensorMountRadius(document))) {
    return {
      document: clone(document || {}),
      error: 'Der gewünschte Abstand passt in der aktuellen Richtung nicht in den Raum oder Außenradius.',
    };
  }
  return { document: updateEntityPosition(document, movingId, candidate), error: '' };
}

export function geometryEntities(document) {
  return EDITABLE_IDS.map((id) => ({
    id,
    role: id === 'TX' ? 'transmitter' : 'receiver',
    position_m: entityPosition(document, id),
  }));
}

export function wallLabel(id) {
  return WALL_LABELS[id] || id;
}

export function markerWallDistance(position, wallId, room) {
  if (wallId === 'WALL_X0') return numberValue(position?.[0]);
  if (wallId === 'WALL_XMAX') return room[0] - numberValue(position?.[0]);
  if (wallId === 'WALL_Z0') return numberValue(position?.[2]);
  if (wallId === 'WALL_ZMAX') return room[2] - numberValue(position?.[2]);
  return NaN;
}

function wallEntities() {
  return WALL_IDS.map((id) => ({
    id,
    role: 'wall',
    label: wallLabel(id),
  }));
}

function selectableEntities(document) {
  return [...geometryEntities(document), ...wallEntities()];
}

export function setMarkerWallDistance(document, markerId, wallId, requestedDistance) {
  const distance = Number(requestedDistance);
  if (!EDITABLE_IDS.includes(markerId) || !WALL_IDS.includes(wallId)) {
    return { document: clone(document || {}), error: 'Für diesen Abstand muss genau ein RX/TX und eine Wand ausgewählt sein.' };
  }
  if (!Number.isFinite(distance)) {
    return { document: clone(document || {}), error: 'Der Abstand muss ein endlicher Wert sein.' };
  }

  const room = vector3(document?.room_dimensions_m, [0, 0, 0]);
  if (room.some((value) => !Number.isFinite(value) || value <= 0)) {
    return { document: clone(document || {}), error: 'Der Abstand kann erst bei gültigen Raummaßen gesetzt werden.' };
  }
  const marker = entityPosition(document, markerId);
  const candidate = [...marker];
  if (wallId === 'WALL_X0') candidate[0] = distance;
  if (wallId === 'WALL_XMAX') candidate[0] = room[0] - distance;
  if (wallId === 'WALL_Z0') candidate[2] = distance;
  if (wallId === 'WALL_ZMAX') candidate[2] = room[2] - distance;
  if (!sensorPositionWithinBounds(candidate, room, sensorMountRadius(document))) {
    return {
      document: clone(document || {}),
      error: 'Der gewünschte Wandabstand passt nicht in den Raum oder Außenradius.',
    };
  }
  return { document: updateEntityPosition(document, markerId, candidate), error: '' };
}

export function validateGeometryDraft(document) {
  const room = vector3(document?.room_dimensions_m, [0, 0, 0]);
  const errors = [];
  if (room.some((value) => !Number.isFinite(value) || value <= 0)) {
    errors.push('Raummaße müssen drei endliche Werte größer als 0 enthalten.');
  }

  const rawRadius = document?.sensor_mount_radius_m;
  const radiusValue = rawRadius == null || rawRadius === '' ? DEFAULT_SENSOR_MOUNT_RADIUS_M : Number(rawRadius);
  if (!Number.isFinite(radiusValue) || radiusValue < 0 || radiusValue > MAX_SENSOR_MOUNT_RADIUS_M) {
    errors.push(`Außenradius muss zwischen 0 und ${MAX_SENSOR_MOUNT_RADIUS_M} m liegen.`);
  }
  const radius = Number.isFinite(radiusValue) && radiusValue >= 0
    ? radiusValue
    : DEFAULT_SENSOR_MOUNT_RADIUS_M;

  const entities = geometryEntities(document);
  const receiverIds = (document?.receivers || []).map((receiver) => receiver.id);
  if (receiverIds.join(',') !== 'RX1,RX2,RX3,RX4') {
    errors.push('Es werden genau RX1, RX2, RX3 und RX4 in dieser Reihenfolge benötigt.');
  }

  for (const entity of entities) {
    if (entity.position_m.some((value) => !Number.isFinite(value))) {
      errors.push(`${entity.id}: Koordinaten müssen endlich sein.`);
      continue;
    }
    if (room.every((value) => Number.isFinite(value) && value > 0)) {
      if (entity.position_m[1] < 0 || entity.position_m[1] > room[1]) {
        errors.push(`${entity.id}: Höhe liegt außerhalb des Raums.`);
      } else if (sensorOutsideDistance(entity.position_m, room) > radius + 0.000001) {
        errors.push(`${entity.id}: Außenradius von ${formatNumber(radius)} m überschritten.`);
      }
    }
  }

  const receiverPositions = entities
    .filter((entity) => entity.role === 'receiver')
    .map((entity) => entity.position_m.join('|'));
  if (new Set(receiverPositions).size !== receiverPositions.length) {
    errors.push('RX-Positionen müssen eindeutig sein.');
  }

  return { valid: errors.length === 0, errors, room, entities };
}

function formatNumber(value) {
  return numberValue(value).toFixed(2);
}

function gridStep(value) {
  if (value <= 5) return 0.5;
  if (value <= 12) return 1;
  if (value <= 24) return 2;
  return 5;
}

function cadBounds(room, radius) {
  const margin = Math.max(0, numberValue(radius, DEFAULT_SENSOR_MOUNT_RADIUS_M));
  return {
    xMin: -margin,
    xMax: room[0] + margin,
    zMin: -margin,
    zMax: room[2] + margin,
  };
}

function worldToSvg(position, room, radius = DEFAULT_SENSOR_MOUNT_RADIUS_M) {
  const bounds = cadBounds(room, radius);
  return {
    x: VIEWBOX.plot.x + ((numberValue(position?.[0]) - bounds.xMin) / (bounds.xMax - bounds.xMin)) * VIEWBOX.plot.width,
    y: VIEWBOX.plot.y + ((numberValue(position?.[2]) - bounds.zMin) / (bounds.zMax - bounds.zMin)) * VIEWBOX.plot.height,
  };
}

function svgToWorld(x, y, room, radius = DEFAULT_SENSOR_MOUNT_RADIUS_M) {
  const bounds = cadBounds(room, radius);
  return [
    Math.max(bounds.xMin, Math.min(bounds.xMax, bounds.xMin + ((x - VIEWBOX.plot.x) / VIEWBOX.plot.width) * (bounds.xMax - bounds.xMin))),
    0,
    Math.max(bounds.zMin, Math.min(bounds.zMax, bounds.zMin + ((y - VIEWBOX.plot.y) / VIEWBOX.plot.height) * (bounds.zMax - bounds.zMin))),
  ];
}

function markerMarkup(entity, room, selectedIds, radius) {
  const point = worldToSvg(entity.position_m, room, radius);
  const selected = selectedIds.includes(entity.id);
  const kind = entity.role === 'transmitter' ? 'tx' : 'rx';
  const colorClass = entity.id.toLowerCase();
  return `
    <g class="occ-cad-marker occ-cad-marker-${kind} occ-cad-marker-${colorClass} ${selected ? 'is-selected' : ''}"
       data-geometry-handle data-geometry-id="${escapeHTML(entity.id)}"
       tabindex="0" role="button" aria-label="${escapeHTML(`${entity.id} bei ${formatNumber(entity.position_m[0])} x ${formatNumber(entity.position_m[2])} m`)}"
       transform="translate(${point.x.toFixed(2)} ${point.y.toFixed(2)})">
      <circle class="occ-cad-marker-hit" r="18"></circle>
      <circle class="occ-cad-marker-core" r="${kind === 'tx' ? 9 : 7}"></circle>
      <text x="14" y="-10">${escapeHTML(entity.id)}</text>
    </g>`;
}

function wallProjection(position, wallId, room) {
  const point = vector3(position);
  if (wallId === 'WALL_X0') return [0, point[1], Math.max(0, Math.min(room[2], point[2]))];
  if (wallId === 'WALL_XMAX') return [room[0], point[1], Math.max(0, Math.min(room[2], point[2]))];
  if (wallId === 'WALL_Z0') return [Math.max(0, Math.min(room[0], point[0])), point[1], 0];
  if (wallId === 'WALL_ZMAX') return [Math.max(0, Math.min(room[0], point[0])), point[1], room[2]];
  return null;
}

function selectionConnectionPoints(selectedEntities, room, radius) {
  const markers = selectedEntities.filter((entity) => entity.role !== 'wall');
  const wall = selectedEntities.find((entity) => entity.role === 'wall');
  if (markers.length === 2) {
    return [markers[0].position_m, markers[1].position_m]
      .map((position) => worldToSvg(position, room, radius));
  }
  if (markers.length === 1 && wall) {
    const projection = wallProjection(markers[0].position_m, wall.id, room);
    return projection
      ? [markers[0].position_m, projection].map((position) => worldToSvg(position, room, radius))
      : null;
  }
  return null;
}

function selectionLineMarkup(selectedEntities, room, radius) {
  const points = selectionConnectionPoints(selectedEntities, room, radius);
  if (!points) return '';
  const [first, second] = points;
  const coordinates = `x1="${first.x.toFixed(2)}" y1="${first.y.toFixed(2)}" x2="${second.x.toFixed(2)}" y2="${second.y.toFixed(2)}"`;
  return `<line class="occ-cad-selection-line-backdrop" data-cad-selection-line aria-hidden="true" ${coordinates} /><line class="occ-cad-selection-line" aria-hidden="true" ${coordinates} />`;
}

function wallMarkup(room, selectedIds, radius) {
  const x0z0 = worldToSvg([0, 0, 0], room, radius);
  const xMaxz0 = worldToSvg([room[0], 0, 0], room, radius);
  const x0zMax = worldToSvg([0, 0, room[2]], room, radius);
  const xMaxzMax = worldToSvg([room[0], 0, room[2]], room, radius);
  const edges = {
    WALL_X0: [x0z0.x, x0z0.y, x0zMax.x, x0zMax.y],
    WALL_XMAX: [xMaxz0.x, xMaxz0.y, xMaxzMax.x, xMaxzMax.y],
    WALL_Z0: [x0z0.x, x0z0.y, xMaxz0.x, xMaxz0.y],
    WALL_ZMAX: [x0zMax.x, x0zMax.y, xMaxzMax.x, xMaxzMax.y],
  };
  return WALL_IDS.map((id) => {
    const [x1, y1, x2, y2] = edges[id];
    const selected = selectedIds.includes(id);
    return `<line class="occ-cad-wall-hit ${selected ? 'is-selected' : ''}" data-wall-handle data-wall-id="${id}" tabindex="0" role="button" aria-label="${escapeHTML(wallLabel(id))}" x1="${x1}" y1="${y1}" x2="${x2}" y2="${y2}" /><line class="occ-cad-wall-edge" aria-hidden="true" x1="${x1}" y1="${y1}" x2="${x2}" y2="${y2}" />`;
  }).join('');
}

function axisTicks(max, step, radius, minimumPixelGap) {
  const values = [];
  const min = -radius;
  const end = max + radius;
  for (let value = min; value <= end + 0.0001; value += step) values.push(Number(value.toFixed(6)));
  [0, max].forEach((value) => {
    if (!values.some((candidate) => Math.abs(candidate - value) < 0.0001)) values.push(value);
  });
  const sorted = values.sort((first, second) => first - second);
  const scale = VIEWBOX.plot.width / (max + (radius * 2));
  const kept = [];
  sorted.forEach((value) => {
    const previous = kept.at(-1);
    if (previous == null || (value - previous) * scale >= minimumPixelGap) kept.push(value);
  });
  return kept;
}

function rulerMarkup(room, radius) {
  const xStep = gridStep(room[0]);
  const zStep = gridStep(room[2]);
  const xTicks = [];
  const zTicks = [];
  for (const value of axisTicks(room[0], xStep, radius, 30)) {
    const point = worldToSvg([value, 0, 0], room, radius);
    xTicks.push(`<line x1="${point.x}" y1="${VIEWBOX.plot.y - 8}" x2="${point.x}" y2="${VIEWBOX.plot.y - 1}" /><text class="occ-cad-axis-tick" data-cad-axis-tick="x" data-cad-axis-value="${formatNumber(value)}" x="${point.x}" y="${VIEWBOX.plot.y - 18}" text-anchor="middle">${formatNumber(value)}</text>`);
  }
  for (const value of axisTicks(room[2], zStep, radius, 18)) {
    const point = worldToSvg([0, 0, value], room, radius);
    zTicks.push(`<line x1="${VIEWBOX.plot.x - 8}" y1="${point.y}" x2="${VIEWBOX.plot.x - 1}" y2="${point.y}" /><text class="occ-cad-axis-tick" data-cad-axis-tick="z" data-cad-axis-value="${formatNumber(value)}" x="${VIEWBOX.plot.x - 16}" y="${point.y + 3}" text-anchor="end">${formatNumber(value)}</text>`);
  }
  return `<g class="occ-cad-rulers"><g>${xTicks.join('')}</g><g>${zTicks.join('')}</g><text class="occ-cad-axis-label" x="${VIEWBOX.plot.x + VIEWBOX.plot.width / 2}" y="${VIEWBOX.plot.y - 39}" text-anchor="middle">X / LÄNGE (m)</text><text class="occ-cad-axis-label" x="${VIEWBOX.plot.x - 62}" y="${VIEWBOX.plot.y + VIEWBOX.plot.height / 2}" text-anchor="middle" transform="rotate(-90 ${VIEWBOX.plot.x - 62} ${VIEWBOX.plot.y + VIEWBOX.plot.height / 2})">Z / BREITE (m)</text></g>`;
}

function gridMarkup(room, radius) {
  const xStep = gridStep(room[0]);
  const zStep = gridStep(room[2]);
  const lines = [];
  for (const value of axisTicks(room[0], xStep, radius, 30)) {
    const point = worldToSvg([value, 0, 0], room, radius);
    lines.push(`<line x1="${point.x}" y1="${VIEWBOX.plot.y}" x2="${point.x}" y2="${VIEWBOX.plot.y + VIEWBOX.plot.height}" />`);
  }
  for (const value of axisTicks(room[2], zStep, radius, 18)) {
    const point = worldToSvg([0, 0, value], room, radius);
    lines.push(`<line x1="${VIEWBOX.plot.x}" y1="${point.y}" x2="${VIEWBOX.plot.x + VIEWBOX.plot.width}" y2="${point.y}" />`);
  }
  return lines.join('');
}

function axisMarkup(room, radius) {
  const origin = worldToSvg([0, 0, 0], room, radius);
  const originX = origin.x;
  const originY = origin.y;
  return `<g class="occ-cad-axes" aria-label="Achsenkonvention: X und Z in der Draufsicht, Y als Höhe">
    <line class="occ-cad-axis" data-axis="x" x1="${originX}" y1="${originY}" x2="${originX + 42}" y2="${originY}"></line>
    <line class="occ-cad-axis" data-axis="z" x1="${originX}" y1="${originY}" x2="${originX}" y2="${originY + 34}"></line>
    <line class="occ-cad-axis occ-cad-axis-height" data-axis="y" x1="${originX}" y1="${originY}" x2="${originX + 24}" y2="${originY - 24}"></line>
    <text class="occ-cad-axis-caption" x="${originX + 48}" y="${originY + 4}">+X</text>
    <text class="occ-cad-axis-caption" x="${originX - 6}" y="${originY + 48}">+Z</text>
    <text class="occ-cad-axis-caption occ-cad-axis-height-caption" x="${originX + 29}" y="${originY - 27}">+Y / H</text>
  </g>`;
}

function roomRectMarkup(room, radius) {
  const first = worldToSvg([0, 0, 0], room, radius);
  const second = worldToSvg([room[0], 0, room[2]], room, radius);
  const x = Math.min(first.x, second.x);
  const y = Math.min(first.y, second.y);
  const width = Math.abs(second.x - first.x);
  const height = Math.abs(second.y - first.y);
  return `<rect class="occ-cad-room" x="${x.toFixed(2)}" y="${y.toFixed(2)}" width="${width.toFixed(2)}" height="${height.toFixed(2)}"></rect>`;
}

function sensorZoneMarkup(radius) {
  return `<rect class="occ-cad-sensor-zone" data-cad-sensor-zone x="${VIEWBOX.plot.x}" y="${VIEWBOX.plot.y}" width="${VIEWBOX.plot.width}" height="${VIEWBOX.plot.height}"></rect><text class="occ-cad-sensor-zone-label" x="${VIEWBOX.plot.x + 10}" y="${VIEWBOX.plot.y + 15}">SENSORZONE ±${formatNumber(radius)} m</text>`;
}

function inspectorInput(label, value, attributeName) {
  return `<label class="occ-cad-input"><span>${label}</span><input type="number" step="0.01" data-cad-coordinate="${attributeName}" value="${escapeHTML(formatNumber(value))}"></label>`;
}

export class RoomGeometryEditor {
  constructor(container, { document, onChange, onSelect, selectedIds } = {}) {
    this.container = container;
    this.document = clone(document || {});
    this.onChange = onChange;
    this.onSelect = onSelect;
    this.selectedIds = (Array.isArray(selectedIds) ? selectedIds : ['TX'])
      .filter((id) => SELECTABLE_IDS.includes(id))
      .slice(0, 2);
    this.selectedId = this.selectedIds[0] || null;
    this.snap = true;
    this.drag = null;
    this.distanceDraft = null;
    this.distanceError = '';
    this._mounted = false;
    this._onClick = (event) => this._handleClick(event);
    this._onPointerDown = (event) => this._handlePointerDown(event);
    this._onPointerMove = (event) => this._handlePointerMove(event);
    this._onPointerUp = (event) => this._handlePointerUp(event);
    this._onKeyDown = (event) => this._handleKeyDown(event);
    this._onChange = (event) => this._handleFormChange(event);
  }

  mount() {
    if (!this.container || this._mounted) return;
    this._mounted = true;
    this.container.addEventListener('click', this._onClick);
    this.container.addEventListener('pointerdown', this._onPointerDown);
    this.container.addEventListener('pointermove', this._onPointerMove);
    this.container.addEventListener('pointerup', this._onPointerUp);
    this.container.addEventListener('pointercancel', this._onPointerUp);
    this.container.addEventListener('keydown', this._onKeyDown);
    this.container.addEventListener('change', this._onChange);
    this.render();
  }

  dispose() {
    if (!this.container || !this._mounted) return;
    this.container.removeEventListener('click', this._onClick);
    this.container.removeEventListener('pointerdown', this._onPointerDown);
    this.container.removeEventListener('pointermove', this._onPointerMove);
    this.container.removeEventListener('pointerup', this._onPointerUp);
    this.container.removeEventListener('pointercancel', this._onPointerUp);
    this.container.removeEventListener('keydown', this._onKeyDown);
    this.container.removeEventListener('change', this._onChange);
    this._mounted = false;
    this.drag = null;
  }

  setDocument(document) {
    this.document = clone(document || {});
    if (this._mounted && !this.drag) this.render();
  }

  _emitChange() {
    if (typeof this.onChange === 'function') this.onChange(clone(this.document));
  }

  _select(id, additive = false) {
    if (!SELECTABLE_IDS.includes(id)) return;
    const previous = Array.isArray(this.selectedIds) ? this.selectedIds : [];
    let next = [id];
    if (additive) {
      if (previous.includes(id)) {
        next = [...previous];
      } else if (previous.length >= 2) {
        next = [previous[0], id];
      } else {
        next = [...previous, id];
      }
    }
    this.selectedIds = next.slice(0, 2);
    this.selectedId = this.selectedIds[0] || id;
    this.distanceDraft = null;
    this.distanceError = '';
    if (typeof this.onSelect === 'function') this.onSelect(id);
    this.render();
  }

  _clearSelection() {
    if (!this.selectedIds.length && this.selectedId == null) return;
    this.selectedIds = [];
    this.selectedId = null;
    this.distanceDraft = null;
    this.distanceError = '';
    if (typeof this.onSelect === 'function') this.onSelect(null);
    this.render();
  }

  _handleClick(event) {
    const action = event.target.closest?.('[data-cad-action]')?.dataset.cadAction;
    if (action === 'toggle-snap') {
      this.snap = !this.snap;
      this._updateToolbar();
      return;
    }
    if (action === 'set-distance') {
      event.preventDefault();
      this._setSelectedDistance();
      return;
    }
    const handle = event.target.closest?.('[data-geometry-handle]');
    if (handle) {
      this._select(handle.dataset.geometryId, event.shiftKey);
      return;
    }
    const wall = event.target.closest?.('[data-wall-handle]');
    if (wall) {
      this._select(wall.dataset.wallId, event.shiftKey);
      return;
    }
    if (event.target.closest?.('[data-cad-svg]')) this._clearSelection();
  }

  _handlePointerDown(event) {
    const handle = event.target.closest?.('[data-geometry-handle]');
    const wall = event.target.closest?.('[data-wall-handle]');
    if (wall) {
      if (event.button === 0) {
        event.preventDefault();
        this._select(wall.dataset.wallId, event.shiftKey);
      }
      return;
    }
    if (!handle || event.button !== 0) return;
    event.preventDefault();
    const id = handle.dataset.geometryId;
    this.drag = { id, pointerId: event.pointerId };
    this._select(id, event.shiftKey);
    this.container.querySelector('[data-cad-svg]')?.setPointerCapture?.(event.pointerId);
  }

  _svgPoint(event) {
    const svg = this.container.querySelector('[data-cad-svg]');
    if (!svg) return null;
    const rect = svg.getBoundingClientRect();
    if (!rect.width || !rect.height) return null;
    return {
      x: ((event.clientX - rect.left) / rect.width) * VIEWBOX.width,
      y: ((event.clientY - rect.top) / rect.height) * VIEWBOX.height,
    };
  }

  _handlePointerMove(event) {
    if (!this.drag || event.pointerId !== this.drag.pointerId) return;
    const point = this._svgPoint(event);
    if (!point) return;
    const room = roomDimensions(this.document);
    let position = svgToWorld(point.x, point.y, room, sensorMountRadius(this.document));
    if (this.snap) {
      position[0] = Math.round(position[0] / 0.05) * 0.05;
      position[2] = Math.round(position[2] / 0.05) * 0.05;
    }
    this.document = updateEntityPosition(this.document, this.drag.id, position);
    this._updateLiveMarker(this.drag.id);
    this._updateInspector();
  }

  _handlePointerUp(event) {
    if (!this.drag || (event.pointerId != null && event.pointerId !== this.drag.pointerId)) return;
    this.drag = null;
    this._emitChange();
    this._updateValidation();
  }

  _handleKeyDown(event) {
    const wall = event.target.closest?.('[data-wall-handle]');
    if (wall && ['Enter', ' '].includes(event.key)) {
      event.preventDefault();
      this._select(wall.dataset.wallId, event.shiftKey);
      return;
    }
    const handle = event.target.closest?.('[data-geometry-handle]');
    if (!handle || !['ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight'].includes(event.key)) return;
    event.preventDefault();
    const id = handle.dataset.geometryId;
    const current = entityPosition(this.document, id);
    const step = this.snap ? 0.05 : 0.01;
    if (event.key === 'ArrowUp') current[2] -= step;
    if (event.key === 'ArrowDown') current[2] += step;
    if (event.key === 'ArrowLeft') current[0] -= step;
    if (event.key === 'ArrowRight') current[0] += step;
    const room = roomDimensions(this.document);
    const radius = sensorMountRadius(this.document);
    current[0] = Math.min(room[0] + radius, Math.max(-radius, current[0]));
    current[2] = Math.min(room[2] + radius, Math.max(-radius, current[2]));
    this.document = updateEntityPosition(this.document, id, current);
    this.render();
    this._emitChange();
  }

  _handleFormChange(event) {
    const distanceInput = event.target.closest?.('[data-cad-distance-input]');
    if (distanceInput) {
      this.distanceDraft = distanceInput.value;
      return;
    }
    const sensorRadius = event.target.closest?.('[data-cad-sensor-radius]');
    if (sensorRadius) {
      const raw = sensorRadius.value;
      this.document = {
        ...this.document,
        sensor_mount_radius_m: raw === '' ? raw : numberValue(raw, sensorMountRadius(this.document)),
      };
      this._emitChange();
      this.render();
      return;
    }
    const coordinate = event.target.closest?.('[data-cad-coordinate]')?.dataset.cadCoordinate;
    if (coordinate) {
      const [id, indexText] = coordinate.split('.');
      const index = Number(indexText);
      if (EDITABLE_IDS.includes(id) && [0, 1, 2].includes(index)) {
        const position = entityPosition(this.document, id);
        position[index] = numberValue(event.target.value, position[index]);
        this.document = updateEntityPosition(this.document, id, position);
        this._emitChange();
        this.render();
      }
      return;
    }
    const dimension = event.target.closest?.('[data-cad-dimension]')?.dataset.cadDimension;
    if (dimension != null) {
      const index = Number(dimension);
      if ([0, 1, 2].includes(index)) {
        const dimensions = roomDimensions(this.document);
        dimensions[index] = numberValue(event.target.value, dimensions[index]);
        this.document = updateRoomDimensions(this.document, dimensions);
        this._emitChange();
        this.render();
      }
    }
  }

  _selectedEntities() {
    const selectedIds = Array.isArray(this.selectedIds) ? this.selectedIds : [];
    return selectableEntities(this.document).filter((entity) => selectedIds.includes(entity.id));
  }

  _validationState() {
    const validation = validateGeometryDraft(this.document);
    const errors = [...validation.errors];
    if (this.distanceError) errors.push(this.distanceError);
    return { ...validation, errors, valid: errors.length === 0 };
  }

  _setSelectedDistance() {
    const selected = this._selectedEntities();
    if (selected.length !== 2) return;
    const input = this.container.querySelector('[data-cad-distance-input]');
    const raw = this.distanceDraft ?? input?.value ?? '';
    const marker = selected.find((entity) => entity.role !== 'wall');
    const wall = selected.find((entity) => entity.role === 'wall');
    const result = marker && wall
      ? setMarkerWallDistance(this.document, marker.id, wall.id, raw)
      : marker
        ? setPlanDistance(this.document, selected[0].id, selected[1].id, raw)
        : { document: clone(this.document), error: 'Wähle für den Abstand eine Wand und zusätzlich ein RX/TX aus.' };
    if (result.error) {
      this.distanceDraft = raw;
      this.distanceError = result.error;
      this.render();
      return;
    }
    this.document = result.document;
    this.distanceDraft = formatNumber(Number(raw));
    this.distanceError = '';
    this.render();
    this._emitChange();
  }

  _updateLiveMarker(id) {
    const marker = this.container.querySelector(`[data-geometry-id="${CSS.escape(id)}"]`);
    if (!marker) return;
    const point = worldToSvg(entityPosition(this.document, id), roomDimensions(this.document), sensorMountRadius(this.document));
    marker.setAttribute('transform', `translate(${point.x.toFixed(2)} ${point.y.toFixed(2)})`);
    marker.setAttribute('aria-label', `${id} bei ${formatNumber(entityPosition(this.document, id)[0])} x ${formatNumber(entityPosition(this.document, id)[2])} m`);
    this._updateSelectionLine();
  }

  _updateSelectionLine() {
    const line = this.container.querySelector('[data-cad-selection-line]');
    if (!line) return;
    const points = selectionConnectionPoints(this._selectedEntities(), roomDimensions(this.document), sensorMountRadius(this.document));
    if (!points) return;
    const [first, second] = points;
    const attributes = {
      x1: first.x.toFixed(2),
      y1: first.y.toFixed(2),
      x2: second.x.toFixed(2),
      y2: second.y.toFixed(2),
    };
    this.container.querySelectorAll('[data-cad-selection-line]').forEach((selectionLine) => {
      Object.entries(attributes).forEach(([name, value]) => selectionLine.setAttribute(name, value));
    });
  }

  _updateInspector() {
    const entity = geometryEntities(this.document).find((candidate) => candidate.id === this.selectedId);
    if (!entity) return;
    const ownerDocument = this.container.ownerDocument ?? globalThis.document;
    [0, 1, 2].forEach((index) => {
      const input = this.container.querySelector(`[data-cad-coordinate="${this.selectedId}.${index}"]`);
      if (input && ownerDocument?.activeElement !== input) input.value = formatNumber(entity.position_m[index]);
    });
    const radiusInput = this.container.querySelector('[data-cad-sensor-radius]');
    if (radiusInput && ownerDocument?.activeElement !== radiusInput) radiusInput.value = formatNumber(sensorMountRadius(this.document));
    const selection = this.container.querySelector('[data-cad-selection]');
    if (selection) selection.textContent = this.selectedIds.join(' · ');
    this._updateValidation();
  }

  _updateValidation() {
    const validation = this._validationState();
    const status = this.container.querySelector('[data-cad-validation]');
    if (status) {
      status.textContent = validation.valid ? 'GEOMETRIE GÜLTIG' : `${validation.errors.length} BLOCKER`;
      status.className = `occ-cad-validation ${validation.valid ? 'is-valid' : 'is-invalid'}`;
    }
    const errors = this.container.querySelector('[data-cad-errors]');
    if (errors) {
      errors.className = `occ-cad-errors ${validation.valid ? 'is-valid' : 'is-invalid'}`;
      errors.innerHTML = validation.errors.length
        ? validation.errors.map((error) => `<li>${escapeHTML(error)}</li>`).join('')
        : '<li>Marker liegen im Raum oder im Außenradius.</li>';
    }
  }

  _updateToolbar() {
    const button = this.container.querySelector('[data-cad-action="toggle-snap"]');
    if (button) button.textContent = `Rasterfang ${this.snap ? 'AN' : 'AUS'}`;
  }

  render() {
    if (!this.container) return;
    const room = roomDimensions(this.document);
    const radius = sensorMountRadius(this.document);
    const entities = geometryEntities(this.document);
    this.selectedIds = (Array.isArray(this.selectedIds) ? this.selectedIds : [])
      .filter((id) => SELECTABLE_IDS.includes(id))
      .slice(0, 2);
    if (this.selectedIds.length) {
      if (!this.selectedIds.includes(this.selectedId)) this.selectedId = this.selectedIds[0];
    } else {
      this.selectedId = null;
    }
    const selectedMarker = entities.find((entity) => this.selectedIds.includes(entity.id));
    const selectedEntities = this._selectedEntities();
    const validation = this._validationState();
    const selectedPair = selectedEntities.length === 2 ? selectedEntities : null;
    const selectionLine = selectionLineMarkup(selectedEntities, room, radius);
    const pairMarker = selectedPair?.find((entity) => entity.role !== 'wall');
    const pairWall = selectedPair?.find((entity) => entity.role === 'wall');
    const pairDistance = pairMarker && pairWall
      ? markerWallDistance(pairMarker.position_m, pairWall.id, room)
      : selectedPair && !pairWall
        ? planDistance(selectedPair[0].position_m, selectedPair[1].position_m)
        : null;
    const distanceValue = selectedPair
      ? this.distanceDraft ?? (pairDistance == null ? '' : formatNumber(pairDistance))
      : '';
    const coordinateMarker = selectedMarker || (selectedEntities.length === 1 && selectedEntities[0].role !== 'wall' ? selectedEntities[0] : null);
    const selectedPosition = coordinateMarker?.position_m || [0, 0, 0];
    const selectionLabel = this.selectedIds.length
      ? this.selectedIds.map((id) => wallLabel(id)).join(' · ')
      : 'Keine Auswahl';
    const pairLabel = selectedPair?.map((entity) => entity.role === 'wall' ? entity.label : entity.id).join(' · ');
    const distanceBounds = pairWall
      ? `min="${-radius}" max="${radius}"`
      : 'min="0.01"';
    const pairDistanceMarkup = selectedPair
      ? pairDistance == null
        ? `<div class="occ-cad-distance-selection"><strong>${escapeHTML(pairLabel)}</strong><span>Wandabstand über Raummaße.</span></div>`
        : `<div class="occ-cad-distance-selection"><strong>${escapeHTML(pairLabel)}</strong><span>aktuell ${formatNumber(pairDistance)} m</span></div><label class="occ-cad-input"><span>${pairWall ? 'Wandabstand (m)' : 'Abstand x/z (m)'}</span><input type="number" ${distanceBounds} step="0.01" data-cad-distance-input value="${escapeHTML(distanceValue)}"></label><button type="button" class="occ-button occ-button-primary" data-cad-action="set-distance">Setzen</button>${this.distanceError ? `<p class="occ-cad-distance-error" role="alert">${escapeHTML(this.distanceError)}</p>` : ''}<p class="occ-cad-helper">${pairWall ? `Senkrecht zu ${escapeHTML(pairWall.label)} · negativ = außen.` : 'Erstes Element bleibt der Anker.'}</p>`
      : '';
    const coordinateMarkup = coordinateMarker
      ? `<div class="occ-cad-inspector-section"><span class="occ-cad-section-label">${escapeHTML(coordinateMarker.id)} [x / y / z] m</span><div class="occ-cad-dimension-grid">${inspectorInput('x', selectedPosition[0], `${coordinateMarker.id}.0`)}${inspectorInput('y', selectedPosition[1], `${coordinateMarker.id}.1`)}${inspectorInput('z', selectedPosition[2], `${coordinateMarker.id}.2`)}</div></div>`
      : `<div class="occ-cad-inspector-section"><span class="occ-cad-section-label">${escapeHTML(selectionLabel)}</span><p class="occ-cad-helper">Shift: RX/TX + Wand für Abstand.</p></div>`;
    this.container.innerHTML = `
      <div class="occ-cad-toolbar">
          <div><span class="occ-cad-kicker">CAD / TOPPLAN</span><strong>Raum</strong><small>Klick: Auswahl · Leer: löschen · Shift: zweites Element · Drag: x/z · y: Höhe</small><div class="occ-cad-legend">${['TX', 'RX1', 'RX2', 'RX3', 'RX4'].map((id) => `<span class="occ-cad-legend-item"><i class="occ-cad-swatch occ-cad-swatch-${id.toLowerCase()}" aria-hidden="true"></i>${id}</span>`).join('')}</div></div>
        <div class="occ-cad-toolbar-actions"><span data-cad-validation class="occ-cad-validation ${validation.valid ? 'is-valid' : 'is-invalid'}">${validation.valid ? 'GEOMETRIE GÜLTIG' : `${validation.errors.length} BLOCKER`}</span><button type="button" class="occ-button occ-button-quiet" data-cad-action="toggle-snap">Rasterfang ${this.snap ? 'AN' : 'AUS'}</button></div>
      </div>
      <div class="occ-cad-layout">
        <div class="occ-cad-viewport">
          <svg data-cad-svg viewBox="0 0 ${VIEWBOX.width} ${VIEWBOX.height}" role="img" aria-label="CAD-Draufsicht des Raum-Setups. Klick in einen leeren Bereich hebt die Auswahl auf.">
            <defs><pattern id="occCadMinorGrid" width="20" height="20" patternUnits="userSpaceOnUse"><path d="M 20 0 L 0 0 0 20" fill="none" stroke="rgba(17,17,17,.08)" stroke-width="1" /></pattern></defs>
            <rect class="occ-cad-surface" x="0" y="0" width="${VIEWBOX.width}" height="${VIEWBOX.height}"></rect>
            ${sensorZoneMarkup(radius)}
            <rect class="occ-cad-minor-grid" x="${VIEWBOX.plot.x}" y="${VIEWBOX.plot.y}" width="${VIEWBOX.plot.width}" height="${VIEWBOX.plot.height}" fill="url(#occCadMinorGrid)"></rect>
            <g class="occ-cad-grid-lines">${gridMarkup(room, radius)}</g>
            ${rulerMarkup(room, radius)}
            ${roomRectMarkup(room, radius)}
            <g class="occ-cad-walls">${wallMarkup(room, this.selectedIds, radius)}</g>
            ${axisMarkup(room, radius)}
            ${selectionLine}
            <g class="occ-cad-markers">${entities.map((entity) => markerMarkup(entity, room, this.selectedIds, radius)).join('')}</g>
            <text class="occ-cad-room-label" x="${worldToSvg([0, 0, 0], room, radius).x + 12}" y="${worldToSvg([0, 0, 0], room, radius).y + 24}">${formatNumber(room[0])} × ${formatNumber(room[2])} m · H ${formatNumber(room[1])} m</text>
          </svg>
        </div>
        <aside class="occ-cad-inspector" aria-label="CAD Inspector">
          <div class="occ-cad-inspector-kicker">INSPECTOR</div>
          <div class="occ-cad-selection-row"><span>Auswahl</span><strong data-cad-selection>${escapeHTML(selectionLabel)}</strong></div>
          <div class="occ-cad-inspector-section"><span class="occ-cad-section-label">Raum [L / H / B] m</span><div class="occ-cad-dimension-grid">${[0, 1, 2].map((index) => `<label class="occ-cad-input"><span>${['L', 'H', 'B'][index]}</span><input type="number" min="0.1" step="0.01" data-cad-dimension="${index}" value="${escapeHTML(formatNumber(room[index]))}"></label>`).join('')}</div></div>
          <div class="occ-cad-inspector-section"><span class="occ-cad-section-label">Sensorzone</span><label class="occ-cad-input"><span>Außenradius (m)</span><input type="number" min="0" max="${MAX_SENSOR_MOUNT_RADIUS_M}" step="0.05" data-cad-sensor-radius value="${escapeHTML(formatNumber(radius))}"></label><p class="occ-cad-helper">X/Z außerhalb erlaubt; Y bleibt im Raum.</p></div>
          ${selectedPair ? `<div class="occ-cad-inspector-section occ-cad-distance-section"><span class="occ-cad-section-label">${pairWall ? 'Abstand zur Wand' : 'Abstand in der Draufsicht'}</span>${pairDistanceMarkup}</div>` : ''}
          ${coordinateMarkup}
          <p class="occ-cad-helper">Ziehen/Pfeile. Raster 5 cm. Speichern übernimmt.</p>
          <ul class="occ-cad-errors ${validation.valid ? 'is-valid' : 'is-invalid'}" data-cad-errors>${validation.errors.length ? validation.errors.map((error) => `<li>${escapeHTML(error)}</li>`).join('') : '<li>Marker liegen im Raum oder im Außenradius.</li>'}</ul>
        </aside>
      </div>
    `;
  }
}

export default RoomGeometryEditor;

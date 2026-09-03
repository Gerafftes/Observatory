/**
 * Dedicated mmWave/RX comparison view for the Sensing tab.
 *
 * The two sources deliberately stay separate:
 *   - signal red = mmWave target / Ground Truth
 *   - black      = WiFi CSI/RX position estimate
 *   - red outline = recent radar target rejected by room bounds
 *
 * This module has no dependency on the position-point contract. It is a
 * transport/debug view, so it can show a continuous radar coordinate even
 * while WiFi localization is still uncalibrated.
 */

export const MMWAVE_STATUS_ENDPOINT = '/api/v1/mmwave/status';
export const MMWAVE_STATUS_FRESH_MS = 1500;
export const MMWAVE_REJECTION_VISIBLE_MS = 3000;
export const RX_POSITION_FRESH_MS = 3000;

export const DEFAULT_ROOM_DIMENSIONS = [4.02, 2.59, 3.44];

const MM_TO_M = 0.001;
const EPSILON = 1e-6;
const SIGNAL_RED = 0xe51c23;
const INK_BLACK = 0x111111;
const HARDWARE_GREY = 0x68737a;
const ROOM_GREY = 0x707070;

function finiteNumber(value) {
  return typeof value === 'number' && Number.isFinite(value);
}

function finiteTriplet(value) {
  return Array.isArray(value) && value.length === 3 && value.every(finiteNumber);
}

function finitePair(value) {
  return Array.isArray(value) && value.length === 2 && value.every(finiteNumber);
}

export function isValidRoomDimensions(value) {
  return finiteTriplet(value) && value.every((dimension) => dimension > 0);
}

/** Return a safe [length, height, width] tuple for rendering. */
export function normalizeRoomDimensions(value, fallback = DEFAULT_ROOM_DIMENSIONS) {
  if (isValidRoomDimensions(value)) return value.slice(0, 3);
  if (isValidRoomDimensions(fallback)) return fallback.slice(0, 3);
  return DEFAULT_ROOM_DIMENSIONS.slice();
}

function pointInsideRoom(point, roomDimensions, allowBoundary = true) {
  if (!finiteTriplet(point)) return false;
  const room = normalizeRoomDimensions(roomDimensions);
  return point.every((coordinate, index) => {
    const upper = room[index];
    return coordinate >= (allowBoundary ? -EPSILON : 0)
      && coordinate <= upper + (allowBoundary ? EPSILON : 0);
  });
}

function positionMmInsideRoom(positionMm, roomDimensions) {
  if (!finitePair(positionMm)) return false;
  const room = normalizeRoomDimensions(roomDimensions);
  return positionMm[0] >= 0
    && positionMm[1] >= 0
    && positionMm[0] <= room[0] * 1000 + EPSILON
    && positionMm[1] <= room[2] * 1000 + EPSILON;
}

function parseLegacyRejectionPosition(reason) {
  if (typeof reason !== 'string') return null;
  const match = reason.match(/target\s*\[\s*(-?\d+)\s*,\s*(-?\d+)\s*\]/i);
  return match ? [Number(match[1]), Number(match[2])] : null;
}

function ageFromStatus(value) {
  return finiteNumber(value) && value >= 0 ? value : null;
}

function rejectionPosition(status) {
  const rejection = status && typeof status.last_rejection === 'object'
    ? status.last_rejection
    : null;
  if (!rejection || rejection.category !== 'room_bounds') return null;
  return finitePair(rejection.position_mm)
    ? rejection.position_mm.slice(0, 2)
    : parseLegacyRejectionPosition(rejection.reason);
}

function rejectionRawPosition(status) {
  const rejection = status && typeof status.last_rejection === 'object'
    ? status.last_rejection
    : null;
  return rejection?.category === 'room_bounds' && finitePair(rejection.raw_position_mm)
    ? rejection.raw_position_mm.slice(0, 2)
    : null;
}

/** Normalize the server response without making a missing target look valid. */
export function normalizeMmwaveDebugStatus(status, nowMs = Date.now()) {
  const raw = status && typeof status === 'object' ? status : {};
  const state = typeof raw.state === 'string' ? raw.state.toLowerCase() : 'disconnected';
  const roomDimensions = isValidRoomDimensions(raw.room_dimensions_m)
    ? raw.room_dimensions_m.slice(0, 3)
    : null;
  const packetAgeMs = ageFromStatus(raw.packet_age_ms);
  const packetFresh = packetAgeMs == null
    ? state === 'valid'
    : packetAgeMs <= MMWAVE_STATUS_FRESH_MS;
  const targetPositionMm = finitePair(raw.target_position_mm)
    ? raw.target_position_mm.slice(0, 2)
    : null;
  const targetRawPositionMm = finitePair(raw.target_raw_position_mm)
    ? raw.target_raw_position_mm.slice(0, 2)
    : null;
  const accepted = state === 'valid'
    && Number(raw.target_count) === 1
    && packetFresh
    && positionMmInsideRoom(targetPositionMm, roomDimensions || DEFAULT_ROOM_DIMENSIONS);
  const rejection = raw.last_rejection && typeof raw.last_rejection === 'object'
    ? raw.last_rejection
    : null;
  const rejectionAgeMs = rejection && ageFromStatus(rejection.age_ms);
  const rejectedPositionMm = rejectionPosition(raw);
  const rejectedRawPositionMm = rejectionRawPosition(raw);
  const rejectionVisible = rejection?.category === 'room_bounds'
    && rejectedPositionMm
    && rejectionAgeMs != null
    && rejectionAgeMs <= MMWAVE_REJECTION_VISIBLE_MS;
  const diagnosticRawPositionMm = rejectionVisible
    ? rejectedRawPositionMm
    : accepted
      ? targetRawPositionMm
      : null;
  const diagnosticRoomPositionMm = rejectionVisible
    ? rejectedPositionMm
    : accepted
      ? targetPositionMm
      : null;
  const diagnosticScenePositionM = diagnosticRoomPositionMm
    ? mmwavePositionToScene(
      diagnosticRoomPositionMm,
      roomDimensions || DEFAULT_ROOM_DIMENSIONS,
      rejectionVisible ? 0.1 : 0.08,
    )
    : null;

  return {
    raw,
    state,
    label: state === 'valid' && accepted
      ? 'VALID'
      : state === 'no_target'
        ? 'NO TARGET'
        : state === 'stale'
          ? 'STALE'
          : state === 'invalid'
            ? 'INVALID'
            : state.toUpperCase(),
    reason: typeof raw.reason === 'string' ? raw.reason : '',
    roomDimensions,
    mountingPositionM: finiteTriplet(raw.mounting_position_m)
      ? raw.mounting_position_m.slice(0, 3)
      : null,
    receiverPositionsM: Array.isArray(raw.receiver_positions_m)
      ? raw.receiver_positions_m.filter(finiteTriplet).map((position) => position.slice(0, 3))
      : [],
    transform: raw.transform && typeof raw.transform === 'object' ? raw.transform : null,
    packetAgeMs,
    packetFresh,
    accepted,
    targetRawPositionMm: accepted ? targetRawPositionMm : null,
    targetPositionMm: accepted ? targetPositionMm : null,
    targetCount: Number.isFinite(Number(raw.target_count)) ? Number(raw.target_count) : 0,
    rejectedPositionMm: rejectionVisible ? rejectedPositionMm : null,
    rejectedRawPositionMm: rejectionVisible ? rejectedRawPositionMm : null,
    rejectionVisible: Boolean(rejectionVisible),
    rejectionAgeMs,
    packetsLost: Number.isFinite(Number(raw.packets_lost)) ? Number(raw.packets_lost) : 0,
    packetsRejected: Number.isFinite(Number(raw.packets_rejected)) ? Number(raw.packets_rejected) : 0,
    rebootCount: Number.isFinite(Number(raw.reboot_count)) ? Number(raw.reboot_count) : 0,
    sequence: Number.isFinite(Number(raw.sequence)) ? Number(raw.sequence) : null,
    diagnosticRawPositionMm,
    diagnosticRoomPositionMm,
    diagnosticScenePositionM,
    lastSequenceGap: raw.last_sequence_gap && typeof raw.last_sequence_gap === 'object'
      ? raw.last_sequence_gap
      : null,
    nodeControl: raw.node_control && typeof raw.node_control === 'object'
      ? raw.node_control
      : null,
    receivedAtMs: nowMs,
    error: false,
  };
}

/** Convert a WiFi frame into a marker-safe state. Simulated frames never pass. */
export function normalizeRxDebugState(frame, receivedAtMs = Date.now(), nowMs = Date.now()) {
  const raw = frame && typeof frame === 'object' ? frame : {};
  const source = typeof raw.source === 'string' ? raw.source.toLowerCase() : '';
  const simulated = raw._simulated === true || source === 'simulated' || source === 'simulate';
  const roomDimensions = normalizeRoomDimensions(raw.room_dimensions);
  const ageMs = finiteNumber(receivedAtMs) && finiteNumber(nowMs)
    ? Math.max(0, nowMs - receivedAtMs)
    : Infinity;
  const estimate = raw.position_estimate && typeof raw.position_estimate === 'object'
    ? raw.position_estimate
    : null;
  const coordinates = finiteTriplet(estimate?.coordinates_m)
    ? estimate.coordinates_m.slice(0, 3)
    : null;
  const fresh = ageMs <= RX_POSITION_FRESH_MS;
  const validPosition = !simulated
    && source === 'esp32'
    && estimate?.state === 'position'
    && fresh
    && pointInsideRoom(coordinates, roomDimensions);
  let state = simulated ? 'simulated' : (estimate?.state || 'uncalibrated');
  if (!validPosition && !simulated && ageMs > RX_POSITION_FRESH_MS) state = 'stale';
  return {
    raw,
    source,
    simulated,
    ageMs,
    fresh,
    state,
    coordinates: validPosition ? coordinates : null,
    pointId: typeof estimate?.point_id === 'string' ? estimate.point_id : null,
    nodes: Array.isArray(raw.nodes)
      ? raw.nodes
        .filter((node) => pointInsideRoom(node?.position, roomDimensions))
        .map((node) => ({
          id: node.node_id ?? node.id ?? '?',
          position: node.position.slice(0, 3),
        }))
      : [],
    txPosition: pointInsideRoom(raw.tx_position, roomDimensions)
      ? raw.tx_position.slice(0, 3)
      : null,
    roomDimensions: isValidRoomDimensions(raw.room_dimensions)
      ? raw.room_dimensions.slice(0, 3)
      : null,
    validPosition,
  };
}

/**
 * Map sealed room coordinates to the same display orientation as the main
 * Gaussian-splat view, then center them for this scene. The current Sensing
 * UI mirrors the physical x-axis so the wall/RX order stays consistent with
 * the calibrated view. Rejected radar coordinates remain untouched in the
 * facts panel; only their visual marker is clamped to the room edge below.
 */
export function roomPositionToScene(position, roomDimensions = DEFAULT_ROOM_DIMENSIONS) {
  if (!finiteTriplet(position)) return null;
  const room = normalizeRoomDimensions(roomDimensions);
  const displayX = room[0] - position[0];
  return [displayX - room[0] / 2, position[1], position[2] - room[2] / 2];
}

export function mmwavePositionToScene(positionMm, roomDimensions, floorY = 0.08) {
  if (!finitePair(positionMm)) return null;
  return roomPositionToScene(
    [positionMm[0] * MM_TO_M, floorY, positionMm[1] * MM_TO_M],
    roomDimensions,
  );
}

/**
 * Keep a rejected target marker inside the visible room silhouette.
 *
 * This is a presentation-only guard. The diagnostic/status coordinates are
 * never changed, so a room-bounds reject remains auditable in the facts panel.
 */
export function clampScenePositionToRoom(position, roomDimensions, margin = 0.14) {
  if (!finiteTriplet(position)) return null;
  const room = normalizeRoomDimensions(roomDimensions);
  const halfLength = Math.max(0, room[0] / 2 - margin);
  const halfWidth = Math.max(0, room[2] / 2 - margin);
  return [
    Math.max(-halfLength, Math.min(halfLength, position[0])),
    position[1],
    Math.max(-halfWidth, Math.min(halfWidth, position[2])),
  ];
}

function formatCoordinates(position, unit = 'm') {
  if (!finiteTriplet(position)) return '—';
  return unit === 'mm'
    ? `[${position.map((coordinate) => Math.round(coordinate)).join(', ')}] mm`
    : `[${position.map((coordinate) => coordinate.toFixed(2)).join(', ')}] m`;
}

function formatRadarCoordinates(positionMm) {
  if (!finitePair(positionMm)) return '—';
  return `[${positionMm.map((coordinate) => Math.round(coordinate)).join(', ')}] mm`;
}

function sameTuple(a, b, tolerance = 1e-4) {
  if (a == null && b == null) return true;
  return finiteTriplet(a) && finiteTriplet(b)
    && a.every((value, index) => Math.abs(value - b[index]) <= tolerance);
}

function distanceOnFloor(a, b) {
  if (!finiteTriplet(a) || !finiteTriplet(b)) return null;
  return Math.hypot(a[0] - b[0], a[2] - b[2]);
}

function getThree() {
  return typeof window !== 'undefined' ? window.THREE : null;
}

/**
 * Samaritan label: off-white field, black rule, mono uppercase type and a
 * narrow source stripe. Labels stay readable without glow or sci-fi chrome.
 */
function createMarkerLabel(text, color, THREE) {
  if (typeof document === 'undefined' || !THREE) return null;
  const labelText = String(text);
  const canvas = document.createElement('canvas');
  canvas.width = Math.max(192, Math.ceil(labelText.length * 24 + 28));
  canvas.height = 80;
  const context = canvas.getContext('2d');
  if (!context) return null;
  context.fillStyle = 'rgba(250, 250, 248, 0.96)';
  context.fillRect(0, 0, canvas.width, canvas.height);
  context.strokeStyle = '#111111';
  context.lineWidth = 3;
  context.strokeRect(2, 2, canvas.width - 4, canvas.height - 4);
  context.fillStyle = `#${color.toString(16).padStart(6, '0')}`;
  context.fillRect(2, 2, 9, canvas.height - 4);
  context.fillStyle = '#111111';
  context.font = `bold ${labelText.length > 8 ? 30 : 38}px monospace`;
  context.textAlign = 'center';
  context.textBaseline = 'middle';
  context.fillText(labelText, canvas.width / 2, canvas.height / 2);

  const texture = new THREE.CanvasTexture(canvas);
  texture.minFilter = THREE.LinearFilter;
  const sprite = new THREE.Sprite(new THREE.SpriteMaterial({
    map: texture,
    transparent: true,
    depthTest: false,
  }));
  sprite.position.set(0, 0.28, 0);
  sprite.scale.set(0.56 * (canvas.width / 192), 0.23, 1);
  sprite.renderOrder = 10;
  return sprite;
}

export class MmwaveDebugView {
  constructor(container, options = {}) {
    this.container = container;
    this.options = options;
    this._getSetupGeometry = typeof options.getSetupGeometry === 'function'
      ? options.getSetupGeometry
      : null;
    this._configuredGeometry = null;
    this.status = normalizeMmwaveDebugStatus(null);
    this.rx = normalizeRxDebugState(null, 0, 1);
    this.connectionState = 'connecting';
    this.roomDimensions = DEFAULT_ROOM_DIMENSIONS.slice();
    this._roomDimensionsSource = 'fallback';
    this._mounted = false;
    this._disposed = false;
    this._raf = null;
    this._resizeObserver = null;
    this._listeners = [];
    this._drag = null;
    this._statusError = null;
    this._scene = null;
    this._camera = null;
    this._renderer = null;
    this._roomGroup = null;
    this._hardwareGroup = null;
    this._markerGroup = null;
    this._deltaLine = null;
    this._mmwaveMarker = null;
    this._rejectedMarker = null;
    this._sensorMarker = null;
    this._rxMarker = null;
    this._rxNodeMeshes = [];
    this._txMarker = null;
    this._showReferenceNodes = false;
    this._canvas = null;
    this._viewport = null;
    this._viewMode = '3d';
    // Match the main Sensing viewport: look in from the +z side and keep the
    // same calibrated x-axis mirror (see roomPositionToScene()).
    this._yaw = 0;
    this._pitch = 0.61;
    this._zoom = 1;
  }

  mount() {
    if (this._mounted || !this.container) return this;
    this._disposed = false;
    this._mounted = true;
    this.container.innerHTML = `
      <section class="mmwave-assistant sensing-mmwave-debug" data-mmwave-debug-visual="samaritan-v2" aria-labelledby="sensingMmwaveDebugTitle">
        <div class="mmwave-assistant-header sensing-mmwave-debug-header">
          <div>
            <div class="mmwave-eyebrow sensing-mmwave-debug-kicker">SOURCE COMPARISON</div>
            <h3 id="sensingMmwaveDebugTitle">Radar / RX-Debug</h3>
            <p>Getrennte Messspuren: mmWave liefert die Referenz, RX/CSI die spätere WiFi-only-Schätzung.</p>
          </div>
          <div class="mmwave-state sensing-mmwave-debug-state" data-mmwave-debug="state" role="status" aria-live="polite">VERBINDE …</div>
        </div>
        <div class="mmwave-assistant-grid sensing-mmwave-debug-body">
          <div class="sensing-mmwave-debug-viewport" data-mmwave-debug="viewport">
            <canvas data-mmwave-debug="canvas" aria-label="3D-Vergleich von mmWave- und RX-Position"></canvas>
            <div class="sensing-mmwave-debug-overlay" data-mmwave-debug="overlay" role="status" aria-live="polite">Warte auf Messdaten …</div>
          </div>
          <aside class="sensing-mmwave-debug-facts" aria-label="Messquellen und Laufzeitstatus">
            <div class="sensing-mmwave-debug-fact sensing-mmwave-debug-fact-radar">
              <span>mmWave · Referenz</span><strong data-mmwave-debug="radar">NO TARGET</strong><small data-mmwave-debug="radar-position">—</small>
            </div>
            <div class="sensing-mmwave-debug-fact"><span>Sensor raw · [x rechts, y vor]</span><strong data-mmwave-debug="raw-position">—</strong></div>
            <div class="sensing-mmwave-debug-fact"><span>Raum transformiert · [x, z]</span><strong data-mmwave-debug="room-position">—</strong></div>
            <div class="sensing-mmwave-debug-fact"><span>UI-Szene · [x, y, z]</span><strong data-mmwave-debug="scene-position">—</strong></div>
            <div class="sensing-mmwave-debug-fact"><span>Transform</span><strong data-mmwave-debug="transform">—</strong></div>
            <div class="sensing-mmwave-debug-fact sensing-mmwave-debug-fact-rx">
              <span>RX/CSI · WiFi-Schätzung</span><strong data-mmwave-debug="rx">UNCALIBRATED</strong><small data-mmwave-debug="rx-position">Kein kalibrierter Marker</small>
            </div>
            <div class="sensing-mmwave-debug-fact"><span>Packet age</span><strong data-mmwave-debug="age">—</strong></div>
            <div class="sensing-mmwave-debug-fact"><span>Sequence / Reboots</span><strong data-mmwave-debug="transport">—</strong></div>
            <div class="sensing-mmwave-debug-fact"><span>Δ floor · keine Fusion</span><strong data-mmwave-debug="delta">—</strong></div>
          </aside>
        </div>
        <div class="sensing-mmwave-debug-footer">
          <div class="sensing-mmwave-debug-legend" aria-label="Legende">
            <span><i class="sensing-mmwave-debug-swatch is-radar"></i>mmWave · Ground Truth</span>
            <span><i class="sensing-mmwave-debug-swatch is-rx"></i>RX/CSI · WiFi-Schätzung</span>
            <span><i class="sensing-mmwave-debug-swatch is-rejected"></i>Radar verworfen · außerhalb Raum</span>
            <span><i class="sensing-mmwave-debug-swatch is-room"></i>Raum / Hardware</span>
          </div>
          <div class="sensing-mmwave-debug-controls" role="group" aria-label="Ansicht und Hardware">
            <button class="mmwave-secondary-button" type="button" data-mmwave-debug-view="3d">3D</button>
            <button class="mmwave-secondary-button" type="button" data-mmwave-debug-view="top">Draufsicht</button>
            <button class="mmwave-secondary-button" type="button" data-mmwave-debug-view="reset">Reset</button>
            <button class="mmwave-secondary-button" type="button" data-mmwave-debug-hardware aria-pressed="false">RX/TX anzeigen</button>
          </div>
        </div>
      </section>
    `;
    this._viewport = this.container.querySelector('[data-mmwave-debug="viewport"]');
    this._canvas = this.container.querySelector('[data-mmwave-debug="canvas"]');
    this._refreshConfiguredGeometry();
    this._bindControls();
    this._initScene();
    this.setView('reset');
    this._renderFacts();
    return this;
  }

  _bindControls() {
    if (!this._viewport) return;
    const buttonNodes = this.container.querySelectorAll('[data-mmwave-debug-view]');
    buttonNodes.forEach((button) => {
      const listener = () => this.setView(button.dataset.mmwaveDebugView);
      button.addEventListener('click', listener);
      this._listeners.push(() => button.removeEventListener('click', listener));
    });
    const hardwareButton = this.container.querySelector('[data-mmwave-debug-hardware]');
    if (hardwareButton) {
      const listener = () => {
        this._showReferenceNodes = !this._showReferenceNodes;
        this._syncHardwareVisibility();
      };
      hardwareButton.addEventListener('click', listener);
      this._listeners.push(() => hardwareButton.removeEventListener('click', listener));
    }
    const pointerDown = (event) => {
      if (event.button !== undefined && event.button !== 0) return;
      this._drag = { x: event.clientX, y: event.clientY, yaw: this._yaw, pitch: this._pitch };
      this._viewport.classList.add('is-dragging');
      this._viewport.setPointerCapture?.(event.pointerId);
    };
    const pointerMove = (event) => {
      if (!this._drag) return;
      this._yaw = this._drag.yaw - (event.clientX - this._drag.x) * 0.008;
      this._pitch = Math.max(-0.05, Math.min(1.45, this._drag.pitch + (event.clientY - this._drag.y) * 0.008));
      this._applyCamera();
    };
    const pointerUp = () => {
      this._drag = null;
      this._viewport.classList.remove('is-dragging');
    };
    const wheel = (event) => {
      event.preventDefault();
      this._zoom = Math.max(0.65, Math.min(2.4, this._zoom * (event.deltaY > 0 ? 0.92 : 1.08)));
      this._applyCamera();
    };
    this._viewport.addEventListener('pointerdown', pointerDown);
    this._viewport.addEventListener('pointermove', pointerMove);
    this._viewport.addEventListener('pointerup', pointerUp);
    this._viewport.addEventListener('pointercancel', pointerUp);
    this._viewport.addEventListener('wheel', wheel, { passive: false });
    this._listeners.push(() => this._viewport.removeEventListener('pointerdown', pointerDown));
    this._listeners.push(() => this._viewport.removeEventListener('pointermove', pointerMove));
    this._listeners.push(() => this._viewport.removeEventListener('pointerup', pointerUp));
    this._listeners.push(() => this._viewport.removeEventListener('pointercancel', pointerUp));
    this._listeners.push(() => this._viewport.removeEventListener('wheel', wheel));
  }

  _initScene() {
    const THREE = getThree();
    if (!THREE || !this._canvas) {
      this._setFallback('3D-Engine nicht verfügbar · Status bleibt lesbar');
      return;
    }
    try {
      this._scene = new THREE.Scene();
      this._scene.background = new THREE.Color(0xedede8);
      // Match GaussianSplatRenderer's perspective and orbit composition.
      this._camera = new THREE.PerspectiveCamera(55, 1, 0.01, 100);
      this._renderer = new THREE.WebGLRenderer({
        canvas: this._canvas,
        antialias: true,
        alpha: true,
      });
      this._renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 2));
      this._roomGroup = new THREE.Group();
      this._hardwareGroup = new THREE.Group();
      this._markerGroup = new THREE.Group();
      this._scene.add(this._roomGroup, this._hardwareGroup, this._markerGroup);
      this._buildRoom();
      this._buildMarkers();
      this._resizeScene();
      if (typeof ResizeObserver !== 'undefined' && this._viewport) {
        this._resizeObserver = new ResizeObserver(() => this._resizeScene());
        this._resizeObserver.observe(this._viewport);
      }
      this._animate();
    } catch (error) {
      console.warn('[MmwaveDebugView] Three.js unavailable:', error);
      this._setFallback('3D-Rendering nicht verfügbar · Status bleibt lesbar');
      this._scene = null;
      this._renderer = null;
    }
  }

  _setFallback(message) {
    if (this._viewport) this._viewport.dataset.rendering = 'fallback';
    const overlay = this.container?.querySelector('[data-mmwave-debug="overlay"]');
    if (overlay) overlay.textContent = message;
  }

  _clearGroup(group) {
    if (!group) return;
    while (group.children.length) {
      const child = group.children[0];
      group.remove(child);
      if (child.children?.length) this._clearGroup(child);
      child.geometry?.dispose?.();
      if (Array.isArray(child.material)) child.material.forEach((material) => material.dispose?.());
      else child.material?.dispose?.();
    }
  }

  _buildRoom() {
    const THREE = getThree();
    if (!THREE || !this._roomGroup) return;
    this._clearGroup(this._roomGroup);
    const [length, height, width] = this.roomDimensions;
    // Keep the room primitive equivalent to GaussianSplatRenderer: a grid
    // and a wireframe box, with the canvas providing the neutral background.
    const gridSize = Math.max(length, width);
    const grid = new THREE.GridHelper(gridSize, 20, 0xa3a39d, 0xc9c9c3);
    grid.scale.set(length / gridSize, 1, width / gridSize);
    grid.position.y = 0.002;
    this._roomGroup.add(grid);
    const box = new THREE.LineSegments(
      new THREE.EdgesGeometry(new THREE.BoxGeometry(length, height, width)),
      new THREE.LineBasicMaterial({ color: ROOM_GREY, transparent: true, opacity: 0.72 }),
    );
    box.position.y = height / 2;
    this._roomGroup.add(box);
    this._applyCamera();
  }

  _buildMarkers() {
    const THREE = getThree();
    if (!THREE || !this._hardwareGroup || !this._markerGroup) return;
    this._rxNodeMeshes = [];
    this._clearGroup(this._hardwareGroup);
    this._clearGroup(this._markerGroup);
    const [length, height, width] = this.roomDimensions;
    const mount = this._configuredGeometry?.mountingPositionM
      || this.status.mountingPositionM
      || (this.status.transform && finiteNumber(this.status.transform.origin_x_mm)
        && finiteNumber(this.status.transform.origin_z_mm)
        ? [this.status.transform.origin_x_mm * MM_TO_M, Math.min(height * 0.6, 1.8), this.status.transform.origin_z_mm * MM_TO_M]
        : [0, Math.min(height * 0.6, 1.8), 0]);
    const sensorPosition = roomPositionToScene(mount, this.roomDimensions);
    // Fixed hardware marker: one small diamond and one label. The previous
    // pole/cone assembly implied a field-of-view shape instead of a sensor.
    this._sensorMarker = new THREE.Mesh(
      new THREE.OctahedronGeometry(0.13, 0),
      new THREE.MeshBasicMaterial({ color: HARDWARE_GREY, transparent: true, opacity: 0.9 }),
    );
    this._sensorMarker.position.set(...sensorPosition);
    const sensorLabel = createMarkerLabel('MMWAVE1', HARDWARE_GREY, THREE);
    if (sensorLabel) this._sensorMarker.add(sensorLabel);
    this._hardwareGroup.add(this._sensorMarker);

    const tx = this._configuredGeometry?.txPosition || this.rx.txPosition;
    if (tx) {
      const txPosition = roomPositionToScene(tx, this.roomDimensions);
      this._txMarker = new THREE.Mesh(
        new THREE.OctahedronGeometry(0.16, 0),
        new THREE.MeshBasicMaterial({ color: INK_BLACK, transparent: true, opacity: 0.82 }),
      );
      this._txMarker.position.set(...txPosition);
      const txLabel = createMarkerLabel('TX', INK_BLACK, THREE);
      if (txLabel) this._txMarker.add(txLabel);
      this._hardwareGroup.add(this._txMarker);
    } else {
      this._txMarker = null;
    }

    const accepted = new THREE.Mesh(
      new THREE.SphereGeometry(0.12, 16, 16),
      new THREE.MeshBasicMaterial({ color: SIGNAL_RED }),
    );
    const acceptedLabel = createMarkerLabel('RADAR TARGET', SIGNAL_RED, THREE);
    if (acceptedLabel) accepted.add(acceptedLabel);
    this._mmwaveMarker = accepted;
    this._mmwaveMarker.visible = false;
    this._markerGroup.add(this._mmwaveMarker);

    const rejected = new THREE.Mesh(
      new THREE.OctahedronGeometry(0.14, 0),
      new THREE.MeshBasicMaterial({ color: SIGNAL_RED, wireframe: true, transparent: true, opacity: 0.95 }),
    );
    const rejectedLabel = createMarkerLabel('REJECTED', SIGNAL_RED, THREE);
    if (rejectedLabel) rejected.add(rejectedLabel);
    this._rejectedMarker = rejected;
    this._rejectedMarker.visible = false;
    this._markerGroup.add(rejected);

    const rx = new THREE.Mesh(
      new THREE.SphereGeometry(0.12, 16, 16),
      new THREE.MeshBasicMaterial({ color: INK_BLACK }),
    );
    const rxLabel = createMarkerLabel('RX/CSI', INK_BLACK, THREE);
    if (rxLabel) rx.add(rxLabel);
    rx.visible = false;
    this._rxMarker = rx;
    this._markerGroup.add(rx);

    this._deltaLine = new THREE.Line(
      new THREE.BufferGeometry(),
      new THREE.LineBasicMaterial({ color: 0xd8dde2, transparent: true, opacity: 0.75 }),
    );
    this._deltaLine.visible = false;
    this._markerGroup.add(this._deltaLine);
    this._syncNodes();
    this._syncHardwareVisibility();
  }

  _syncNodes() {
    const THREE = getThree();
    if (!THREE || !this._hardwareGroup) return;
    this._rxNodeMeshes.forEach((mesh) => {
      this._clearGroup(mesh);
      mesh.geometry?.dispose?.();
      mesh.material?.dispose?.();
      this._hardwareGroup.remove(mesh);
    });
    this._rxNodeMeshes = [];
    const configuredNodes = this._configuredGeometry?.receiverPositionsM || [];
    const nodes = configuredNodes.length
      ? configuredNodes
      : this.rx.nodes.length
        ? this.rx.nodes
        : this.status.receiverPositionsM.map((position, index) => ({ id: index + 1, position }));
    for (const [index, node] of nodes.entries()) {
      const scenePosition = roomPositionToScene(node.position, this.roomDimensions);
      if (!scenePosition) continue;
      const mesh = new THREE.Mesh(
        new THREE.SphereGeometry(0.12, 16, 16),
        new THREE.MeshBasicMaterial({ color: HARDWARE_GREY, transparent: true, opacity: 0.82 }),
      );
      mesh.position.set(...scenePosition);
      const label = createMarkerLabel(`RX${node.id}`, HARDWARE_GREY, THREE);
      if (label) mesh.add(label);
      this._hardwareGroup.add(mesh);
      this._rxNodeMeshes.push(mesh);
    }
    this._syncHardwareVisibility();
  }

  _syncHardwareVisibility() {
    if (this._txMarker) this._txMarker.visible = this._showReferenceNodes;
    this._rxNodeMeshes.forEach((mesh) => { mesh.visible = this._showReferenceNodes; });
    const button = this.container?.querySelector('[data-mmwave-debug-hardware]');
    if (button) {
      button.textContent = this._showReferenceNodes ? 'RX/TX ausblenden' : 'RX/TX anzeigen';
      button.setAttribute('aria-pressed', String(this._showReferenceNodes));
      button.classList.toggle('is-active', this._showReferenceNodes);
    }
  }

  _readSetupGeometry() {
    if (!this._getSetupGeometry) return null;
    try {
      const geometry = this._getSetupGeometry();
      if (!geometry || typeof geometry !== 'object') return null;
      const roomDimensions = isValidRoomDimensions(geometry.roomDimensions)
        ? geometry.roomDimensions.slice(0, 3)
        : null;
      const mountingPositionM = finiteTriplet(geometry.mountingPositionM)
        ? geometry.mountingPositionM.slice(0, 3)
        : null;
      const txPosition = finiteTriplet(geometry.txPosition)
        ? geometry.txPosition.slice(0, 3)
        : null;
      const receiverPositionsM = Array.isArray(geometry.receiverPositionsM)
        ? geometry.receiverPositionsM
          .map((node, index) => ({
            id: node?.id || index + 1,
            position: finiteTriplet(node?.position) ? node.position.slice(0, 3) : null,
          }))
          .filter((node) => node.position)
        : [];
      if (!roomDimensions && !mountingPositionM && !txPosition && receiverPositionsM.length === 0) {
        return null;
      }
      return { roomDimensions, mountingPositionM, txPosition, receiverPositionsM };
    } catch {
      return null;
    }
  }

  _refreshConfiguredGeometry() {
    const next = this._readSetupGeometry();
    if (!next) return false;
    const previous = this._configuredGeometry;
    const dimensionsChanged = !sameTuple(previous?.roomDimensions, next.roomDimensions);
    const mountChanged = !sameTuple(previous?.mountingPositionM, next.mountingPositionM);
    const txChanged = !sameTuple(previous?.txPosition, next.txPosition);
    const receiversChanged = JSON.stringify(previous?.receiverPositionsM || [])
      !== JSON.stringify(next.receiverPositionsM || []);
    this._configuredGeometry = next;
    if (next.roomDimensions && dimensionsChanged) {
      this.roomDimensions = next.roomDimensions;
      this._roomDimensionsSource = 'profile';
      if (this._scene) this._buildRoom();
    }
    if (this._scene && (mountChanged || txChanged || receiversChanged)) this._buildMarkers();
    return dimensionsChanged || mountChanged || txChanged || receiversChanged;
  }

  _resizeScene() {
    if (!this._renderer || !this._camera || !this._viewport) return;
    const width = Math.max(1, this._viewport.clientWidth || 640);
    const height = Math.max(1, this._viewport.clientHeight || 360);
    this._camera.aspect = width / height;
    this._camera.updateProjectionMatrix();
    this._renderer.setSize(width, height, false);
    this._applyCamera();
  }

  _applyCamera() {
    if (!this._camera) return;
    const [length, height, width] = this.roomDimensions;
    const radius = Math.max(length, width, height) * 1.8 / this._zoom;
    const targetY = Math.min(height * 0.25, 0.65);
    const horizontal = Math.cos(this._pitch) * radius;
    this._camera.position.set(
      Math.sin(this._yaw) * horizontal,
      targetY + Math.sin(this._pitch) * radius,
      Math.cos(this._yaw) * horizontal,
    );
    this._camera.lookAt(0, targetY, 0);
  }

  _animate() {
    if (this._disposed || !this._renderer || !this._scene || !this._camera) return;
    this._renderer.render(this._scene, this._camera);
    this._raf = requestAnimationFrame(() => this._animate());
  }

  setView(mode) {
    if (mode === 'reset') {
      this._yaw = 0;
      this._pitch = 0.61;
      this._zoom = 1;
      this._viewMode = '3d';
    } else if (mode === 'top') {
      this._pitch = 1.45;
      this._viewMode = 'top';
    } else if (mode === '3d') {
      this._pitch = 0.61;
      this._viewMode = '3d';
    }
    this._applyCamera();
    this.container?.querySelectorAll('[data-mmwave-debug-view]')?.forEach((button) => {
      button.classList.toggle('is-active', button.dataset.mmwaveDebugView === this._viewMode);
    });
  }

  setStatusError(error) {
    // Keep the fixed hardware marker aligned with the CAD editor even while
    // the live mmWave status endpoint is temporarily unavailable.
    this._refreshConfiguredGeometry();
    this._statusError = error ? String(error) : null;
    if (this._statusError) {
      this.status = {
        ...this.status,
        error: true,
        label: 'OFFLINE',
        reason: this._statusError,
        accepted: false,
        targetPositionMm: null,
        targetRawPositionMm: null,
        rejectionVisible: false,
        rejectedPositionMm: null,
        rejectedRawPositionMm: null,
        diagnosticRawPositionMm: null,
        diagnosticRoomPositionMm: null,
        diagnosticScenePositionM: null,
      };
      this._renderMarkers();
      this._renderFacts();
    }
  }

  setConnectionState(state) {
    this.connectionState = state || 'disconnected';
    this._renderMarkers();
    this._renderFacts();
  }

  updateStatus(status, nowMs = Date.now()) {
    this._refreshConfiguredGeometry();
    const previousMount = this.status.mountingPositionM;
    this._statusError = null;
    this.status = normalizeMmwaveDebugStatus(status, nowMs);
    const dimensions = this._configuredGeometry?.roomDimensions || this.status.roomDimensions;
    const mountChanged = !sameTuple(previousMount, this.status.mountingPositionM);
    if (dimensions && !sameTuple(dimensions, this.roomDimensions)) {
      this.roomDimensions = dimensions;
      this._roomDimensionsSource = this._configuredGeometry?.roomDimensions ? 'profile' : 'setup';
      this._buildRoom();
      this._buildMarkers();
    } else if (dimensions && !this._configuredGeometry?.roomDimensions && this._roomDimensionsSource !== 'setup') {
      this.roomDimensions = dimensions;
      this._roomDimensionsSource = 'setup';
      this._buildRoom();
      this._buildMarkers();
    } else if (mountChanged && this._scene) {
      this._buildMarkers();
    }
    this._syncNodes();
    this._renderMarkers();
    this._renderFacts();
  }

  updateSensingFrame(frame, receivedAtMs = Date.now(), nowMs = Date.now()) {
    this._refreshConfiguredGeometry();
    const nextRx = normalizeRxDebugState(frame, receivedAtMs, nowMs);
    const txChanged = !sameTuple(this.rx.txPosition, nextRx.txPosition);
    this.rx = nextRx;
    const dimensions = this._configuredGeometry?.roomDimensions || this.rx.roomDimensions;
    if (!this.status.roomDimensions && dimensions && !sameTuple(dimensions, this.roomDimensions)) {
      this.roomDimensions = dimensions;
      this._roomDimensionsSource = 'frame';
      this._buildRoom();
      this._buildMarkers();
    } else if (txChanged && this._scene) {
      this._buildMarkers();
    }
    this._syncNodes();
    this._renderMarkers(nowMs);
    this._renderFacts(nowMs);
  }

  _renderMarkers(nowMs = Date.now()) {
    if (!this._scene) return;
    const radarScene = this.status.accepted
      ? mmwavePositionToScene(this.status.targetPositionMm, this.roomDimensions)
      : null;
    if (radarScene && this._mmwaveMarker) {
      this._mmwaveMarker.visible = true;
      this._mmwaveMarker.position.set(...radarScene);
    } else if (this._mmwaveMarker) {
      this._mmwaveMarker.visible = false;
    }

    const rejectedScene = this.status.rejectionVisible
      ? mmwavePositionToScene(this.status.rejectedPositionMm, this.roomDimensions, 0.1)
      : null;
    const rejectedBoundaryScene = rejectedScene
      ? clampScenePositionToRoom(rejectedScene, this.roomDimensions)
      : null;
    if (rejectedBoundaryScene && this._rejectedMarker) {
      this._rejectedMarker.visible = true;
      this._rejectedMarker.position.set(...rejectedBoundaryScene);
    } else if (this._rejectedMarker) {
      this._rejectedMarker.visible = false;
    }

    const rxScene = this.connectionState === 'connected' && this.rx.validPosition
      ? roomPositionToScene(this.rx.coordinates, this.roomDimensions)
      : null;
    if (rxScene && this._rxMarker) {
      this._rxMarker.visible = true;
      this._rxMarker.position.set(...rxScene);
    } else if (this._rxMarker) {
      this._rxMarker.visible = false;
    }
    const deltaPoints = radarScene && rxScene ? [radarScene, rxScene] : [];
    this._updateLine(this._deltaLine, deltaPoints);
    if (this._deltaLine) this._deltaLine.visible = deltaPoints.length === 2;
  }

  _updateLine(line, points) {
    const THREE = getThree();
    if (!THREE || !line) return;
    line.geometry.dispose?.();
    line.geometry = new THREE.BufferGeometry();
    if (points.length >= 2) {
      line.geometry.setFromPoints(points.map((point) => new THREE.Vector3(...point)));
    }
  }

  _renderFacts(nowMs = Date.now()) {
    if (!this._mounted || !this.container) return;
    const status = this.status;
    const radarValue = this.container.querySelector('[data-mmwave-debug="radar"]');
    const radarPosition = this.container.querySelector('[data-mmwave-debug="radar-position"]');
    const rawPosition = this.container.querySelector('[data-mmwave-debug="raw-position"]');
    const roomPosition = this.container.querySelector('[data-mmwave-debug="room-position"]');
    const scenePosition = this.container.querySelector('[data-mmwave-debug="scene-position"]');
    const transform = this.container.querySelector('[data-mmwave-debug="transform"]');
    const rxValue = this.container.querySelector('[data-mmwave-debug="rx"]');
    const rxPosition = this.container.querySelector('[data-mmwave-debug="rx-position"]');
    const age = this.container.querySelector('[data-mmwave-debug="age"]');
    const transport = this.container.querySelector('[data-mmwave-debug="transport"]');
    const delta = this.container.querySelector('[data-mmwave-debug="delta"]');
    const badge = this.container.querySelector('[data-mmwave-debug="state"]');
    const overlay = this.container.querySelector('[data-mmwave-debug="overlay"]');
    if (!radarValue || !rxValue || !badge) return;

    const radarLabel = this._statusError
      ? 'OFFLINE'
      : status.rejectionVisible
        ? 'REJECTED'
        : status.label;
    radarValue.textContent = radarLabel;
    radarValue.dataset.state = radarLabel.toLowerCase().replace(/\s+/g, '-');
    radarPosition.textContent = status.accepted
      ? formatRadarCoordinates(status.targetPositionMm)
      : status.rejectionVisible
        ? `verworfen ${formatRadarCoordinates(status.rejectedPositionMm)}`
        : status.reason || 'Kein gültiger Zielpunkt';
    rawPosition.textContent = formatRadarCoordinates(status.diagnosticRawPositionMm);
    roomPosition.textContent = formatRadarCoordinates(status.diagnosticRoomPositionMm);
    scenePosition.textContent = formatCoordinates(status.diagnosticScenePositionM);
    transform.textContent = status.transform
      ? `O [${status.transform.origin_x_mm}, ${status.transform.origin_z_mm}] mm · yaw ${(status.transform.yaw_mdeg / 1000).toFixed(1)}° · raw X ${status.transform.raw_x_inverted ? 'invertiert' : 'normal'}`
      : '—';
    const rxLabel = this.rx.simulated
      ? 'SIMULATION'
      : this.rx.validPosition && this.connectionState === 'connected'
        ? 'POSITION'
        : this.rx.state === 'stale' || this.connectionState !== 'connected'
          ? 'STALE'
          : 'UNCALIBRATED';
    rxValue.textContent = rxLabel;
    rxValue.dataset.state = rxLabel.toLowerCase();
    rxPosition.textContent = this.rx.validPosition && this.connectionState === 'connected'
      ? `${this.rx.pointId || 'ohne Punkt-ID'} · ${formatCoordinates(this.rx.coordinates)}`
      : this.rx.simulated
        ? 'Nicht gemessen · Simulation'
        : 'Kein kalibrierter Marker';
    age.textContent = status.packetAgeMs == null ? '—' : `${Math.round(status.packetAgeMs)} ms`;
    transport.textContent = `${status.sequence == null ? '—' : status.sequence} / ${status.rebootCount}`;
    const radarPositionM = status.accepted
      ? [status.targetPositionMm[0] * MM_TO_M, 0, status.targetPositionMm[1] * MM_TO_M]
      : null;
    const floorDelta = radarPositionM && this.rx.validPosition && this.connectionState === 'connected'
      ? distanceOnFloor(radarPositionM, this.rx.coordinates)
      : null;
    delta.textContent = floorDelta == null ? '—' : `${floorDelta.toFixed(2)} m`;
    badge.textContent = this._statusError ? 'SERVER OFFLINE' : status.label;
    badge.dataset.state = this._statusError ? 'offline' : status.state;
    badge.classList.remove('is-valid', 'is-no_target', 'is-invalid', 'is-multi_target');
    const badgeState = badge.dataset.state;
    if (badgeState === 'valid' || badgeState === 'no_target') {
      badge.classList.add(`is-${badgeState}`);
    } else if (badgeState === 'offline' || badgeState === 'invalid' || badgeState === 'rejected') {
      badge.classList.add('is-invalid');
    }
    const sourceNote = this._roomDimensionsSource === 'fallback' ? ' · Raummaß aus Fallback' : '';
    overlay.dataset.state = this._statusError
      ? 'offline'
      : status.rejectionVisible
        ? 'rejected'
        : status.accepted
          ? 'valid'
          : 'idle';
    overlay.textContent = this._statusError
      ? `Server nicht erreichbar${sourceNote}`
      : status.accepted
        ? `mmWave VALID · ${formatRadarCoordinates(status.targetPositionMm)}${this.rx.validPosition ? ' · RX separat' : ''}`
        : status.rejectionVisible
          ? `Radar verworfen · außerhalb Raum · ${formatRadarCoordinates(status.rejectedPositionMm)}`
          : `mmWave ${status.label}${sourceNote}`;
  }

  dispose() {
    this._disposed = true;
    this._mounted = false;
    if (this._raf != null) cancelAnimationFrame(this._raf);
    this._raf = null;
    this._resizeObserver?.disconnect?.();
    this._resizeObserver = null;
    this._listeners.splice(0).forEach((remove) => remove());
    this._clearGroup(this._roomGroup);
    this._clearGroup(this._hardwareGroup);
    this._clearGroup(this._markerGroup);
    this._renderer?.dispose?.();
    this._renderer = null;
    this._scene = null;
    this._camera = null;
    this._canvas = null;
    this._viewport = null;
  }
}

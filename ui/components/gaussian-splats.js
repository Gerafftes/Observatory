/**
 * Gaussian Splat Renderer for WiFi Sensing Visualization
 *
 * Renders a 3D signal field using Three.js Points with custom ShaderMaterial.
 * Each "splat" is a screen-space disc whose size, color and opacity are driven
 * by the sensing data:
 *   - Size  : signal variance / disruption magnitude
 *   - Color : blue (quiet) -> green (presence) -> red (active motion)
 *   - Opacity: classification confidence
 */

// Use global THREE from CDN (loaded in SensingTab)
const getThree = () => window.THREE;

// ---- Custom Splat Shaders ------------------------------------------------

const SPLAT_VERTEX = `
  attribute float splatSize;
  attribute vec3  splatColor;
  attribute float splatOpacity;

  varying vec3  vColor;
  varying float vOpacity;

  void main() {
    vColor   = splatColor;
    vOpacity = splatOpacity;

    vec4 mvPosition = modelViewMatrix * vec4(position, 1.0);
    gl_PointSize = splatSize * (300.0 / -mvPosition.z);
    gl_Position  = projectionMatrix * mvPosition;
  }
`;

const SPLAT_FRAGMENT = `
  varying vec3  vColor;
  varying float vOpacity;

  void main() {
    // Circular soft-edge disc
    float dist = length(gl_PointCoord - vec2(0.5));
    if (dist > 0.5) discard;
    float alpha = smoothstep(0.5, 0.2, dist) * vOpacity;
    gl_FragColor = vec4(vColor, alpha);
  }
`;

// ---- Color helpers -------------------------------------------------------

/** Map a scalar 0-1 to blue -> green -> red gradient */
function valueToColor(v) {
  const clamped = Math.max(0, Math.min(1, v));
  // blue(0) -> cyan(0.25) -> green(0.5) -> yellow(0.75) -> red(1)
  let r, g, b;
  if (clamped < 0.5) {
    const t = clamped * 2;
    r = 0;
    g = t;
    b = 1 - t;
  } else {
    const t = (clamped - 0.5) * 2;
    r = t;
    g = 1 - t;
    b = 0;
  }
  return [r, g, b];
}

// ---- Node marker color palette -------------------------------------------

const NODE_MARKER_COLORS = [0x00ccff, 0xff7a00, 0xff00cc, 0xffcc00, 0x8b5cf6, 0x00ffcc, 0xff0044];
const DEFAULT_ROOM_DIMENSIONS = [20, 6, 20];
const PROBABILITY_DISPLAY_SCALE_PER_CELL = 0.2;
const POSITION_ESTIMATE_STATES = new Set([
  'position',
  'unknown',
  'ambiguous',
  'insufficient',
  'uncalibrated',
  'stale',
]);
const POSITION_STATE_LABELS = {
  position: 'POSITION',
  unknown: 'UNKNOWN',
  ambiguous: 'AMBIGUOUS',
  insufficient: 'INSUFFICIENT EVIDENCE',
  uncalibrated: 'UNCALIBRATED',
  stale: 'STALE',
};
const POSITION_STATE_REASONS = {
  unknown: 'No reference point met the position acceptance gates.',
  ambiguous: 'Several reference points fit the current measurement.',
  insufficient: 'Not enough valid receiver evidence is available.',
  uncalibrated: 'No validated position fingerprint model is active.',
  stale: 'The previous position estimate is no longer current.',
};
const POSITION_POINT_ID_PATTERN = /^P0[1-9]$/;

function nonEmptyString(value) {
  return typeof value === 'string' && value.trim().length > 0
    ? value.trim()
    : null;
}

function safeRoomDimensions(dimensions, fallbackDimensions = DEFAULT_ROOM_DIMENSIONS) {
  const isValid = (value) =>
    Array.isArray(value) &&
    value.length >= 3 &&
    value
      .slice(0, 3)
      .every(
        (coordinate) =>
          typeof coordinate === 'number' &&
          Number.isFinite(coordinate) &&
          coordinate > 0
      );
  if (isValid(dimensions)) {
    return dimensions.slice(0, 3);
  }
  return isValid(fallbackDimensions)
    ? fallbackDimensions.slice(0, 3)
    : [...DEFAULT_ROOM_DIMENSIONS];
}

/**
 * Convert sealed room coordinates into the physical left/right UI view.
 * Only x is mirrored; y and z retain their measured orientation.
 */
export function displayCoordinatesForRoom(
  coordinates,
  roomDimensions = DEFAULT_ROOM_DIMENSIONS
) {
  const bounds = safeRoomDimensions(roomDimensions);
  if (
    !Array.isArray(coordinates) ||
    coordinates.length !== 3 ||
    !coordinates.every(
      (coordinate, index) =>
        typeof coordinate === 'number' &&
        Number.isFinite(coordinate) &&
        coordinate >= 0 &&
        coordinate <= bounds[index]
    )
  ) {
    return null;
  }
  return [bounds[0] - coordinates[0], coordinates[1], coordinates[2]];
}

export function normalizePositionEstimate(positionEstimate) {
  return normalizePositionEstimateForRoom(
    positionEstimate,
    DEFAULT_ROOM_DIMENSIONS
  );
}

export function normalizePositionEstimateForRoom(
  positionEstimate,
  roomDimensions
) {
  if (!positionEstimate || typeof positionEstimate !== 'object') {
    return {
      state: 'unknown',
      label: POSITION_STATE_LABELS.unknown,
      pointId: null,
      coordinates: null,
      reason: 'No validated position estimate is present in this live frame.',
    };
  }

  const requestedState = nonEmptyString(positionEstimate.state);
  const state = POSITION_ESTIMATE_STATES.has(requestedState)
    ? requestedState
    : 'unknown';
  const suppliedReason = nonEmptyString(positionEstimate.reason);

  if (state === 'position') {
    const pointId =
      typeof positionEstimate.point_id === 'string' &&
      POSITION_POINT_ID_PATTERN.test(positionEstimate.point_id)
        ? positionEstimate.point_id
        : null;
    const coordinates = positionEstimate.coordinates_m;
    const bounds = safeRoomDimensions(roomDimensions);
    if (
      pointId &&
      Array.isArray(coordinates) &&
      coordinates.length === 3 &&
      coordinates.every(
        (coordinate, index) =>
          typeof coordinate === 'number' &&
          Number.isFinite(coordinate) &&
          coordinate >= 0 &&
          coordinate <= bounds[index]
      )
    ) {
      return {
        state,
        label: POSITION_STATE_LABELS[state],
        pointId,
        coordinates,
        reason: suppliedReason,
      };
    }

    return {
      state: 'unknown',
      label: POSITION_STATE_LABELS.unknown,
      pointId: null,
      coordinates: null,
      reason: 'The position payload is incomplete or invalid.',
    };
  }

  return {
    state,
    label: POSITION_STATE_LABELS[state],
    pointId: null,
    coordinates: null,
    reason: suppliedReason || POSITION_STATE_REASONS[state],
  };
}

export function isSimulatedSensingData(data) {
  const source = nonEmptyString(data?.source)?.toLowerCase();
  return source === 'simulated' || source === 'simulate' || source === 'demo';
}

export function positionEstimateViewModel(
  data,
  fallbackRoomDimensions = DEFAULT_ROOM_DIMENSIONS
) {
  const roomDimensions = safeRoomDimensions(
    data?.room_dimensions,
    fallbackRoomDimensions
  );
  const estimate = normalizePositionEstimateForRoom(
    data?.position_estimate,
    roomDimensions
  );
  if (!isSimulatedSensingData(data)) {
    return estimate;
  }

  const [length, , width] = roomDimensions;
  return {
    state: 'simulated',
    label: 'SIMULATED DEMO',
    pointId: estimate.state === 'position' ? estimate.pointId : 'DEMO',
    coordinates:
      estimate.state === 'position'
        ? estimate.coordinates
        : [length / 2, 0, width / 2],
    reason: 'Synthetic demonstration; not a measured person position.',
  };
}

export function resolvedBodyPosition(
  data,
  fallbackRoomDimensions = DEFAULT_ROOM_DIMENSIONS
) {
  const estimate = positionEstimateViewModel(data, fallbackRoomDimensions);
  return estimate.state === 'position' || estimate.state === 'simulated'
    ? estimate.coordinates
    : null;
}

export function resolveFieldGridGeometry(
  signalField,
  localization,
  roomDimensions = DEFAULT_ROOM_DIMENSIONS,
  fallbackColumns = 20,
  fallbackRows = 20
) {
  const probabilityMap = localization?.probability_map;
  const mapColumns = Number(probabilityMap?.columns);
  const mapRows = Number(probabilityMap?.rows);
  const mapOriginX = Number(probabilityMap?.origin?.x);
  const mapOriginZ = Number(probabilityMap?.origin?.z);
  const mapCellSizeX = Number(probabilityMap?.cell_size_x_m);
  const mapCellSizeZ = Number(probabilityMap?.cell_size_z_m);
  const hasProbabilityMap =
    Number.isInteger(mapColumns) &&
    mapColumns > 0 &&
    Number.isInteger(mapRows) &&
    mapRows > 0 &&
    Number.isFinite(mapOriginX) &&
    Number.isFinite(mapOriginZ) &&
    Number.isFinite(mapCellSizeX) &&
    mapCellSizeX > 0 &&
    Number.isFinite(mapCellSizeZ) &&
    mapCellSizeZ > 0 &&
    Array.isArray(probabilityMap?.values) &&
    probabilityMap.values.length >= mapColumns * mapRows;

  if (hasProbabilityMap) {
    return {
      columns: mapColumns,
      rows: mapRows,
      originX: mapOriginX,
      originZ: mapOriginZ,
      cellSizeX: mapCellSizeX,
      cellSizeZ: mapCellSizeZ,
      values: probabilityMap.values,
      isProbability: true,
    };
  }

  const gridSize = Array.isArray(signalField?.grid_size)
    ? signalField.grid_size
    : [fallbackColumns, 1, fallbackRows];
  const requestedColumns = Number(gridSize[0]);
  const requestedRows = Number(gridSize[2]);
  const columns =
    Number.isInteger(requestedColumns) && requestedColumns > 0
      ? requestedColumns
      : fallbackColumns;
  const rows =
    Number.isInteger(requestedRows) && requestedRows > 0
      ? requestedRows
      : fallbackRows;
  const length = Number(roomDimensions?.[0]);
  const width = Number(roomDimensions?.[2]);
  const safeLength = Number.isFinite(length) && length > 0
    ? length
    : DEFAULT_ROOM_DIMENSIONS[0];
  const safeWidth = Number.isFinite(width) && width > 0
    ? width
    : DEFAULT_ROOM_DIMENSIONS[2];

  return {
    columns,
    rows,
    originX: safeLength / (2 * columns),
    originZ: safeWidth / (2 * rows),
    cellSizeX: safeLength / columns,
    cellSizeZ: safeWidth / rows,
    values: Array.isArray(signalField?.values) ? signalField.values : [],
    isProbability: false,
  };
}

// ---- GaussianSplatRenderer -----------------------------------------------

export class GaussianSplatRenderer {
  /**
   * @param {HTMLElement} container - DOM element to attach the renderer to
   * @param {object}      [opts]
   * @param {number}      [opts.width]  - canvas width  (default container width)
   * @param {number}      [opts.height] - canvas height (default 500)
   */
  constructor(container, opts = {}) {
    const THREE = getThree();
    if (!THREE) throw new Error('Three.js not loaded');
    this._THREE = THREE;

    this.container = container;
    this.width  = opts.width  || container.clientWidth || 800;
    this.height = opts.height || 500;
    this.roomDimensions = [...DEFAULT_ROOM_DIMENSIONS];
    this.roomCenter = new THREE.Vector3(10, 0.75, 10);
    this.cameraState = { azimuth: 0, elevation: 55, radius: 20 };

    // Scene
    this.scene = new THREE.Scene();
    this.scene.background = new THREE.Color(0x0a0a12);

    // Camera — perspective looking down at the room
    this.camera = new THREE.PerspectiveCamera(55, this.width / this.height, 0.1, 200);
    this.camera.position.set(10, 14, 24);
    this.camera.lookAt(this.roomCenter);

    // Renderer
    this.renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true });
    this.renderer.setSize(this.width, this.height);
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    container.appendChild(this.renderer.domElement);

    // Grid & room
    this._createRoom(THREE);

    // Signal field splats. Live D6 maps may change dimensions at runtime.
    this.gridColumns = 20;
    this.gridRows = 20;
    this.fieldOriginX = 0.5;
    this.fieldOriginZ = 0.5;
    this.fieldCellSizeX = 1;
    this.fieldCellSizeZ = 1;
    this.fieldUsesProbabilityGeometry = false;
    this._createFieldSplats(THREE);

    // Node markers (ESP32 / router positions)
    this._createNodeMarkers(THREE);

    // Dynamic per-node markers (multi-node support)
    this.nodeMarkers = new Map(); // nodeId -> THREE.Mesh

    // Body disruption blob
    this._createBodyBlob(THREE);

    // Simple orbit-like mouse rotation
    this._setupMouseControls();

    // Animation state
    this._animFrame = null;
    this._lastData = null;

    // Start render loop
    this._animate();
  }

  // ---- Scene setup -------------------------------------------------------

  _createRoom(THREE) {
    if (this.roomGroup) {
      this.scene.remove(this.roomGroup);
      this.roomGroup.traverse((child) => {
        child.geometry?.dispose();
        if (Array.isArray(child.material)) {
          child.material.forEach((material) => material.dispose());
        } else {
          child.material?.dispose();
        }
      });
    }

    const [length, height, width] = this.roomDimensions;
    const footprint = Math.max(length, width);
    this.roomGroup = new THREE.Group();

    // Floor grid
    const grid = new THREE.GridHelper(footprint, 20, 0x1a3a4a, 0x0d1f28);
    grid.scale.set(length / footprint, 1, width / footprint);
    grid.position.set(length / 2, 0, width / 2);
    this.roomGroup.add(grid);

    // Room boundary wireframe
    const boxGeo = new THREE.BoxGeometry(length, height, width);
    const edges  = new THREE.EdgesGeometry(boxGeo);
    const line   = new THREE.LineSegments(
      edges,
      new THREE.LineBasicMaterial({ color: 0x1a4a5a, opacity: 0.3, transparent: true })
    );
    line.position.set(length / 2, height / 2, width / 2);
    this.roomGroup.add(line);
    this.scene.add(this.roomGroup);
  }

  _configureRoom(dimensions) {
    if (!Array.isArray(dimensions) || dimensions.length < 3) return;
    const next = dimensions.slice(0, 3).map(Number);
    if (!next.every((value) => Number.isFinite(value) && value > 0)) return;
    if (next.every((value, index) => Math.abs(value - this.roomDimensions[index]) < 1e-6)) return;

    this.roomDimensions = next;
    const [length, height, width] = next;
    this.roomCenter.set(length / 2, Math.min(height * 0.25, 0.65), width / 2);
    this.cameraState.radius = Math.max(length, width, height) * 1.8;
    if (!this.fieldUsesProbabilityGeometry) {
      this._setLegacyFieldGeometry(this.gridColumns, this.gridRows);
    }
    if (this.bodyBlob) {
      this.bodyBlob.position.set(length / 2, 0, width / 2);
    }
    this._createRoom(this._THREE);
    this._layoutFieldSplats();
    this._updateCamera();
  }

  _setLegacyFieldGeometry(columns, rows) {
    const [length, , width] = this.roomDimensions;
    this.fieldOriginX = length / (2 * columns);
    this.fieldOriginZ = width / (2 * rows);
    this.fieldCellSizeX = length / columns;
    this.fieldCellSizeZ = width / rows;
    this.fieldUsesProbabilityGeometry = false;
  }

  _configureFieldGrid(signalField, localization) {
    const {
      columns,
      rows,
      originX,
      originZ,
      cellSizeX,
      cellSizeZ,
      values,
      isProbability,
    } = resolveFieldGridGeometry(
      signalField,
      localization,
      this.roomDimensions,
      this.gridColumns,
      this.gridRows
    );

    const dimensionsChanged = columns !== this.gridColumns || rows !== this.gridRows;
    const geometryChanged =
      dimensionsChanged ||
      Math.abs(originX - this.fieldOriginX) > 1e-9 ||
      Math.abs(originZ - this.fieldOriginZ) > 1e-9 ||
      Math.abs(cellSizeX - this.fieldCellSizeX) > 1e-9 ||
      Math.abs(cellSizeZ - this.fieldCellSizeZ) > 1e-9;

    this.gridColumns = columns;
    this.gridRows = rows;
    this.fieldOriginX = originX;
    this.fieldOriginZ = originZ;
    this.fieldCellSizeX = cellSizeX;
    this.fieldCellSizeZ = cellSizeZ;
    this.fieldUsesProbabilityGeometry = isProbability;

    if (dimensionsChanged) {
      this._createFieldSplats(this._THREE);
    } else if (geometryChanged) {
      this._layoutFieldSplats();
    }

    return { values, isProbability };
  }

  _layoutFieldSplats() {
    if (!this.fieldPoints) return;
    const positions = this.fieldPoints.geometry.attributes.position.array;
    const [length] = this.roomDimensions;

    for (let iz = 0; iz < this.gridRows; iz++) {
      for (let ix = 0; ix < this.gridColumns; ix++) {
        const idx = iz * this.gridColumns + ix;
        const sourceX = this.fieldOriginX + ix * this.fieldCellSizeX;
        positions[idx * 3] = length - sourceX;
        positions[idx * 3 + 1] = 0.02;
        positions[idx * 3 + 2] = this.fieldOriginZ + iz * this.fieldCellSizeZ;
      }
    }
    this.fieldPoints.geometry.attributes.position.needsUpdate = true;
  }

  _createFieldSplats(THREE) {
    if (this.fieldPoints) {
      this.scene.remove(this.fieldPoints);
      this.fieldPoints.geometry.dispose();
      this.fieldPoints.material.dispose();
    }

    const count = this.gridColumns * this.gridRows;

    const positions = new Float32Array(count * 3);
    const sizes     = new Float32Array(count);
    const colors    = new Float32Array(count * 3);
    const opacities = new Float32Array(count);

    for (let idx = 0; idx < count; idx++) {
      sizes[idx] = 0.35;
      colors[idx * 3] = 0.1;
      colors[idx * 3 + 1] = 0.2;
      colors[idx * 3 + 2] = 0.6;
      opacities[idx] = 0.0;
    }

    const geo = new THREE.BufferGeometry();
    geo.setAttribute('position',    new THREE.BufferAttribute(positions, 3));
    geo.setAttribute('splatSize',   new THREE.BufferAttribute(sizes, 1));
    geo.setAttribute('splatColor',  new THREE.BufferAttribute(colors, 3));
    geo.setAttribute('splatOpacity',new THREE.BufferAttribute(opacities, 1));

    const mat = new THREE.ShaderMaterial({
      vertexShader:   SPLAT_VERTEX,
      fragmentShader: SPLAT_FRAGMENT,
      transparent: true,
      depthWrite: false,
      blending: THREE.AdditiveBlending,
    });

    this.fieldPoints = new THREE.Points(geo, mat);
    this.scene.add(this.fieldPoints);
    this._layoutFieldSplats();
  }

  _createNodeMarkers(THREE) {
    // Transmitter — distinct green diamond, moved to its configured position in update().
    const routerGeo = new THREE.OctahedronGeometry(0.16, 0);
    const routerMat = new THREE.MeshBasicMaterial({ color: 0x00ff88, transparent: true, opacity: 0.8 });
    this.routerMarker = new THREE.Mesh(routerGeo, routerMat);
    this.routerMarker.position.set(0, 0.5, 0);
    this.routerMarker.add(this._createMarkerLabel('TX', 0x00ff88, THREE));
    this.scene.add(this.routerMarker);
  }

  _createMarkerLabel(text, color, THREE) {
    const canvas = document.createElement('canvas');
    canvas.width = 192;
    canvas.height = 80;
    const context = canvas.getContext('2d');
    context.fillStyle = 'rgba(5, 10, 18, 0.78)';
    context.fillRect(0, 0, canvas.width, canvas.height);
    context.strokeStyle = `#${color.toString(16).padStart(6, '0')}`;
    context.lineWidth = 4;
    context.strokeRect(2, 2, canvas.width - 4, canvas.height - 4);
    context.fillStyle = '#ffffff';
    context.font = 'bold 38px monospace';
    context.textAlign = 'center';
    context.textBaseline = 'middle';
    context.fillText(text, canvas.width / 2, canvas.height / 2);

    const texture = new THREE.CanvasTexture(canvas);
    texture.minFilter = THREE.LinearFilter;
    const sprite = new THREE.Sprite(new THREE.SpriteMaterial({
      map: texture,
      transparent: true,
      depthTest: false,
    }));
    sprite.position.set(0, 0.28, 0);
    sprite.scale.set(0.56, 0.23, 1);
    sprite.renderOrder = 10;
    return sprite;
  }

  _createBodyBlob(THREE) {
    // A cluster of splats representing body disruption
    const count = 64;
    const positions = new Float32Array(count * 3);
    const sizes     = new Float32Array(count);
    const colors    = new Float32Array(count * 3);
    const opacities = new Float32Array(count);
    this.bodyBaseSizes = new Float32Array(count);

    for (let i = 0; i < count; i++) {
      // Human-sized vertical column instead of a room-sized sphere.
      const angle = Math.random() * Math.PI * 2;
      const radius = Math.sqrt(Math.random()) * 0.32;
      positions[i * 3] = Math.cos(angle) * radius;
      positions[i * 3 + 1] = 0.18 + Math.random() * 1.55;
      positions[i * 3 + 2] = Math.sin(angle) * radius;

      sizes[i] = 0.35 + Math.random() * 0.35;
      this.bodyBaseSizes[i] = sizes[i];
      colors[i * 3]     = 0.2;
      colors[i * 3 + 1] = 0.8;
      colors[i * 3 + 2] = 0.3;
      opacities[i] = 0.0; // hidden until presence detected
    }

    const geo = new THREE.BufferGeometry();
    geo.setAttribute('position',    new THREE.BufferAttribute(positions, 3));
    geo.setAttribute('splatSize',   new THREE.BufferAttribute(sizes, 1));
    geo.setAttribute('splatColor',  new THREE.BufferAttribute(colors, 3));
    geo.setAttribute('splatOpacity',new THREE.BufferAttribute(opacities, 1));

    const mat = new THREE.ShaderMaterial({
      vertexShader:   SPLAT_VERTEX,
      fragmentShader: SPLAT_FRAGMENT,
      transparent: true,
      depthWrite: false,
      blending: THREE.AdditiveBlending,
    });

    this.bodyBlob = new THREE.Points(geo, mat);
    this.bodyBlob.position.set(this.roomCenter.x, 0, this.roomCenter.z);
    this.scene.add(this.bodyBlob);
  }

  _updateBodyPosition(data, presence) {
    const estimate = positionEstimateViewModel(data, this.roomDimensions);
    const position = resolvedBodyPosition(data, this.roomDimensions);
    this.container.dataset.cloudState = estimate.state;

    if (presence !== true || !position) {
      delete this.container.dataset.cloudTarget;
      delete this.container.dataset.cloudPointId;
      const [length, , width] = this.roomDimensions;
      this.bodyBlob?.position.set(length / 2, 0, width / 2);
      const opacity = this.bodyBlob?.geometry?.attributes?.splatOpacity;
      if (opacity) {
        opacity.array.fill(0);
        opacity.needsUpdate = true;
      }
      return false;
    }

    const displayPosition = displayCoordinatesForRoom(position, this.roomDimensions);
    if (!displayPosition) return false;
    this.bodyBlob?.position.set(...displayPosition);
    this.container.dataset.cloudTarget = position
      .map((value) => value.toFixed(3))
      .join(',');
    this.container.dataset.cloudPointId = estimate.pointId || 'DEMO';
    return true;
  }

  /**
   * Clear measured output immediately when the data connection is not live.
   * Static room/radio geometry remains visible, but stale field mass and a
   * previously measured body must not look like current evidence.
   */
  invalidatePositionEstimate(status = 'stale') {
    this._lastData = null;
    this.container.dataset.cloudState = status;
    delete this.container.dataset.cloudTarget;
    delete this.container.dataset.cloudPointId;
    const [length, , width] = this.roomDimensions;
    this.bodyBlob?.position.set(length / 2, 0, width / 2);

    for (const points of [this.fieldPoints, this.bodyBlob]) {
      const opacity = points?.geometry?.attributes?.splatOpacity;
      if (!opacity) continue;
      opacity.array.fill(0);
      opacity.needsUpdate = true;
    }
  }

  // ---- Mouse controls (simple orbit) -------------------------------------

  _setupMouseControls() {
    let isDragging = false;
    let prevX = 0, prevY = 0;

    const canvas = this.renderer.domElement;
    canvas.addEventListener('mousedown', (e) => {
      isDragging = true;
      prevX = e.clientX;
      prevY = e.clientY;
    });
    canvas.addEventListener('mousemove', (e) => {
      if (!isDragging) return;
      this.cameraState.azimuth += (e.clientX - prevX) * 0.4;
      this.cameraState.elevation -= (e.clientY - prevY) * 0.4;
      this.cameraState.elevation = Math.max(15, Math.min(85, this.cameraState.elevation));
      prevX = e.clientX;
      prevY = e.clientY;
      this._updateCamera();
    });
    canvas.addEventListener('mouseup',   () => { isDragging = false; });
    canvas.addEventListener('mouseleave',() => { isDragging = false; });

    // Scroll to zoom
    canvas.addEventListener('wheel', (e) => {
      e.preventDefault();
      const delta = e.deltaY > 0 ? 1.05 : 0.95;
      const roomScale = Math.max(...this.roomDimensions);
      this.cameraState.radius = Math.max(
        roomScale * 1.15,
        Math.min(roomScale * 4, this.cameraState.radius * delta)
      );
      this._updateCamera();
    }, { passive: false });

    this._updateCamera();
  }

  _updateCamera() {
    const { azimuth, elevation, radius } = this.cameraState;
    const phi = (elevation * Math.PI) / 180;
    const theta = (azimuth * Math.PI) / 180;
    this.camera.position.set(
      this.roomCenter.x + radius * Math.sin(phi) * Math.sin(theta),
      this.roomCenter.y + radius * Math.cos(phi),
      this.roomCenter.z + radius * Math.sin(phi) * Math.cos(theta)
    );
    this.camera.lookAt(this.roomCenter);
  }

  // ---- Data update -------------------------------------------------------

  /**
   * Update the visualization with new sensing data.
   * @param {object} data - sensing_update JSON from ws_server
   */
  update(data) {
    this._lastData = data;
    if (!data) return;

    const features = data.features || {};
    const classification = data.classification || {};
    const signalField = data.signal_field || {};
    const localization = data.localization || {};
    const nodes = Array.isArray(data.nodes) ? data.nodes : [];

    this._configureRoom(data.room_dimensions);
    const fieldData = this._configureFieldGrid(signalField, localization);

    if (this.routerMarker && Array.isArray(data.tx_position)) {
      const [x, y, z] = data.tx_position;
      if ([x, y, z].every(Number.isFinite)) {
        const displayPosition = displayCoordinatesForRoom([x, y, z], this.roomDimensions);
        if (displayPosition) this.routerMarker.position.set(...displayPosition);
      }
    }

    // -- Update signal field splats ----------------------------------------
    if (this.fieldPoints) {
      const geo    = this.fieldPoints.geometry;
      const clr    = geo.attributes.splatColor.array;
      const sizes  = geo.attributes.splatSize.array;
      const opac   = geo.attributes.splatOpacity.array;
      const vals   = fieldData.values;
      const fieldCellCount = this.gridColumns * this.gridRows;
      const count  = Math.min(vals.length, fieldCellCount);
      const displayScale = fieldData.isProbability
        ? fieldCellCount * PROBABILITY_DISPLAY_SCALE_PER_CELL
        : 1;

      for (let i = 0; i < count; i++) {
        // Backend values are normalized map mass (sum = 1), not an
        // arbitrary per-frame maximum. Scaling by cell count keeps the display
        // stable when the probability grid resolution changes.
        const rawValue = Number(vals[i]);
        const safeValue = Number.isFinite(rawValue) ? rawValue : 0;
        const v = Math.max(0, Math.min(1, safeValue * displayScale));
        const [r, g, b] = valueToColor(v);
        clr[i * 3]     = r;
        clr[i * 3 + 1] = g;
        clr[i * 3 + 2] = b;
        sizes[i] = 0.25 + v * 0.9;
        opac[i]  = v > 0 ? 0.04 + v * 0.66 : 0.0;
      }
      for (let i = count; i < fieldCellCount; i++) {
        sizes[i] = 0.25;
        opac[i] = 0.0;
      }

      geo.attributes.splatColor.needsUpdate  = true;
      geo.attributes.splatSize.needsUpdate   = true;
      geo.attributes.splatOpacity.needsUpdate = true;
    }

    // -- Update body blob --------------------------------------------------
    if (this.bodyBlob) {
      const bGeo  = this.bodyBlob.geometry;
      const bOpac = bGeo.attributes.splatOpacity.array;
      const bClr  = bGeo.attributes.splatColor.array;
      const bSize = bGeo.attributes.splatSize.array;

      const presence   = classification.presence === true;
      const motionLvl  = classification.motion_level || 'absent';
      const breathing  = features.breathing_band_power || 0;
      const showBody = this._updateBodyPosition(data, presence);

      // Breathing pulsation
      const breathPulse = 1.0 + Math.sin(Date.now() * 0.004) * Math.min(breathing * 3, 0.4);

      for (let i = 0; i < bOpac.length; i++) {
        if (showBody) {
          bOpac[i] = 0.32;

          // Color by motion level
          if (motionLvl === 'active' || motionLvl === 'present_moving') {
            bClr[i * 3]     = 1.0;
            bClr[i * 3 + 1] = 0.2;
            bClr[i * 3 + 2] = 0.1;
          } else {
            bClr[i * 3]     = 0.1;
            bClr[i * 3 + 1] = 0.8;
            bClr[i * 3 + 2] = 0.4;
          }

          bSize[i] = this.bodyBaseSizes[i] * breathPulse;
        } else {
          bOpac[i] = 0.0;
        }
      }

      bGeo.attributes.splatOpacity.needsUpdate = true;
      bGeo.attributes.splatColor.needsUpdate   = true;
      bGeo.attributes.splatSize.needsUpdate    = true;
    }

    // -- Update dynamic per-node markers (multi-node support) --------------
    if (this.scene) {
      const THREE = this._THREE || window.THREE;
      if (THREE) {
        const activeIds = new Set();
        for (const node of nodes) {
          activeIds.add(node.node_id);
          if (!this.nodeMarkers.has(node.node_id)) {
            const colorIndex = Math.max(0, Number(node.node_id) - 1) % NODE_MARKER_COLORS.length;
            const markerColor = NODE_MARKER_COLORS[colorIndex];
            const geo = new THREE.SphereGeometry(0.12, 16, 16);
            const mat = new THREE.MeshBasicMaterial({
              color: markerColor,
              transparent: true,
              opacity: 0.8,
            });
            const marker = new THREE.Mesh(geo, mat);
            marker.add(this._createMarkerLabel(`RX${node.node_id}`, markerColor, THREE));
            this.scene.add(marker);
            this.nodeMarkers.set(node.node_id, marker);
          }
          const marker = this.nodeMarkers.get(node.node_id);
          const pos = Array.isArray(node.position) ? node.position : [0, 0, 0];
          const coordinates = pos.slice(0, 3).map(Number);
          if (coordinates.length === 3 && coordinates.every(Number.isFinite)) {
            const displayPosition = displayCoordinatesForRoom(coordinates, this.roomDimensions);
            if (displayPosition) marker.position.set(...displayPosition);
          }
        }
        // Remove stale markers
        for (const [id, marker] of this.nodeMarkers) {
          if (!activeIds.has(id)) {
            this.scene.remove(marker);
            this.nodeMarkers.delete(id);
          }
        }
      }
    }
  }

  // ---- Render loop -------------------------------------------------------

  _animate() {
    this._animFrame = requestAnimationFrame(() => this._animate());

    // Gentle router glow pulse
    if (this.routerMarker) {
      const pulse = 0.6 + 0.3 * Math.sin(Date.now() * 0.003);
      this.routerMarker.material.opacity = pulse;
    }

    this.renderer.render(this.scene, this.camera);
  }

  // ---- Resize / cleanup --------------------------------------------------

  resize(width, height) {
    this.width  = width;
    this.height = height;
    this.camera.aspect = width / height;
    this.camera.updateProjectionMatrix();
    this.renderer.setSize(width, height);
  }

  dispose() {
    if (this._animFrame) {
      cancelAnimationFrame(this._animFrame);
    }
    this.renderer.dispose();
    if (this.renderer.domElement.parentNode) {
      this.renderer.domElement.parentNode.removeChild(this.renderer.domElement);
    }
  }
}

/**
 * Trust boundary for Observatory live sensing data.
 *
 * The WebSocket transport is not evidence of a hardware measurement. A frame
 * is considered live hardware only when its source is explicitly `esp32`, it
 * is fresh, and it does not carry a simulation marker. Geometry and position
 * are validated independently so malformed or legacy/coarse payloads fail
 * closed instead of being rendered as measured room positions.
 */

export const OBSERVATORY_LIVE_FRAME_TIMEOUT_MS = 3000;

const HARDWARE_SOURCE = 'esp32';
const SIMULATED_SOURCES = new Set(['demo', 'simulate', 'simulated']);
const POSITION_POINT_PATTERN = /^P0[1-9]$/;
const MAX_FIELD_POINTS = 4096;

function isFiniteNumber(value) {
  return typeof value === 'number' && Number.isFinite(value);
}

function isExactVector3(value) {
  return Array.isArray(value) &&
    value.length === 3 &&
    value.every(isFiniteNumber);
}

function isWithinRoom(position, roomDimensions) {
  return position.every((coordinate, index) =>
    coordinate >= 0 && coordinate <= roomDimensions[index]
  );
}

export function isExplicitEsp32Frame(frame) {
  return frame?.source === HARDWARE_SOURCE && frame?._simulated !== true;
}

export function isExplicitSimulatedFrame(frame) {
  return SIMULATED_SOURCES.has(frame?.source) || frame?._simulated === true;
}

/**
 * Resolve the transport/source status displayed by Observatory.
 *
 * `receivedAtMs` is the browser receipt time. Remote timestamps are not used
 * for freshness because devices and the browser do not share a trusted clock.
 */
export function resolveObservatorySourceState({
  selectedSource,
  connectionState,
  connectionOpenedAtMs,
  frame,
  receivedAtMs,
  nowMs,
  timeoutMs = OBSERVATORY_LIVE_FRAME_TIMEOUT_MS,
}) {
  if (selectedSource !== 'ws') {
    return {
      state: 'simulated',
      label: 'SIMULATED',
      reason: 'Local demonstration data; not an ESP32 measurement.',
    };
  }

  if (receivedAtMs == null || frame == null) {
    if (connectionState === 'closed') {
      return {
        state: 'stale',
        label: 'STALE',
        reason: 'The live connection closed before a usable frame arrived.',
      };
    }
    if (
      connectionState === 'open' &&
      isFiniteNumber(connectionOpenedAtMs) &&
      isFiniteNumber(nowMs) &&
      nowMs - connectionOpenedAtMs > timeoutMs
    ) {
      return {
        state: 'stale',
        label: 'STALE',
        reason: 'The live connection did not deliver a sensing frame in time.',
      };
    }
    return {
      state: 'connecting',
      label: 'CONNECTING',
      reason: 'Waiting for the first explicit sensing frame.',
    };
  }

  const ageMs = nowMs - receivedAtMs;
  if (
    !isFiniteNumber(receivedAtMs) ||
    !isFiniteNumber(nowMs) ||
    !isFiniteNumber(timeoutMs) ||
    timeoutMs <= 0 ||
    ageMs < 0 ||
    ageMs > timeoutMs ||
    connectionState !== 'open'
  ) {
    return {
      state: 'stale',
      label: 'STALE',
      reason: 'The last sensing frame is no longer fresh.',
    };
  }

  if (isExplicitSimulatedFrame(frame)) {
    return {
      state: 'simulated',
      label: 'SIMULATED',
      reason: 'Synthetic frame received over WebSocket; not an ESP32 measurement.',
    };
  }

  if (isExplicitEsp32Frame(frame)) {
    return {
      state: 'live',
      label: 'LIVE ESP32',
      reason: 'Fresh frame from the explicit ESP32 source.',
    };
  }

  return {
    state: 'stale',
    label: 'STALE',
    reason: `Untrusted sensing source: ${String(frame?.source || 'missing')}.`,
  };
}

export function validateObservatoryGeometry(frame) {
  if (!isExplicitEsp32Frame(frame)) {
    return {
      valid: false,
      reason: 'Room geometry is accepted only from an explicit ESP32 frame.',
    };
  }

  const roomDimensions = frame.room_dimensions;
  if (
    !isExactVector3(roomDimensions) ||
    roomDimensions.some((dimension) => dimension <= 0)
  ) {
    return {
      valid: false,
      reason: 'room_dimensions must contain exactly three finite positive numbers.',
    };
  }

  const txPosition = frame.tx_position;
  if (!isExactVector3(txPosition) || !isWithinRoom(txPosition, roomDimensions)) {
    return {
      valid: false,
      reason: 'tx_position must contain exactly three finite in-room coordinates.',
    };
  }

  if (!Array.isArray(frame.nodes) || frame.nodes.length !== 4) {
    return {
      valid: false,
      reason: 'The fixed-room contract requires exactly RX1 through RX4.',
    };
  }

  const seenNodeIds = new Set();
  const receivers = [];
  for (const node of frame.nodes) {
    if (
      !Number.isInteger(node?.node_id) ||
      node.node_id < 1 ||
      node.node_id > 4 ||
      seenNodeIds.has(node.node_id) ||
      !isExactVector3(node?.position) ||
      !isWithinRoom(node.position, roomDimensions)
    ) {
      return {
        valid: false,
        reason: 'Every receiver needs a unique integer node_id and three finite in-room coordinates.',
      };
    }
    seenNodeIds.add(node.node_id);
    receivers.push({
      nodeId: node.node_id,
      position: [...node.position],
    });
  }
  if (![1, 2, 3, 4].every((nodeId) => seenNodeIds.has(nodeId))) {
    return {
      valid: false,
      reason: 'The fixed-room contract requires exactly RX1 through RX4.',
    };
  }

  return {
    valid: true,
    roomDimensions: [...roomDimensions],
    txPosition: [...txPosition],
    receivers,
    reason: null,
  };
}

export function validateObservatoryPosition(frame, geometry) {
  if (!isExplicitEsp32Frame(frame)) {
    return {
      valid: false,
      state: 'unknown',
      reason: 'Position is accepted only from an explicit ESP32 frame.',
    };
  }

  if (frame?.classification?.presence !== true) {
    return {
      valid: false,
      state: 'absent',
      reason: 'No current presence confirmation; position marker cleared.',
    };
  }

  if (!geometry?.valid) {
    return {
      valid: false,
      state: 'invalid_geometry',
      reason: geometry?.reason || 'Validated room geometry is missing.',
    };
  }

  const estimate = frame.position_estimate;
  if (estimate?.state !== 'position') {
    return {
      valid: false,
      state: typeof estimate?.state === 'string' ? estimate.state : 'unknown',
      reason: estimate?.reason || 'No exact discrete position is available.',
    };
  }

  if (!POSITION_POINT_PATTERN.test(estimate.point_id || '')) {
    return {
      valid: false,
      state: 'unknown',
      reason: 'position_estimate.point_id must be one of P01 through P09.',
    };
  }

  if (
    !isExactVector3(estimate.coordinates_m) ||
    !isWithinRoom(estimate.coordinates_m, geometry.roomDimensions)
  ) {
    return {
      valid: false,
      state: 'unknown',
      reason: 'position_estimate coordinates must be exactly three finite in-room numbers.',
    };
  }

  return {
    valid: true,
    state: 'position',
    pointId: estimate.point_id,
    coordinates: [...estimate.coordinates_m],
    reason: null,
  };
}

export function validateObservatorySignalField(frame, geometry) {
  if (!geometry?.valid) {
    return {
      valid: false,
      reason: 'Validated room geometry is required for the signal field.',
    };
  }

  const field = frame?.signal_field;
  const gridSize = field?.grid_size;
  if (
    !Array.isArray(gridSize) ||
    gridSize.length !== 3 ||
    !gridSize.every(Number.isInteger) ||
    gridSize[0] < 2 ||
    gridSize[1] !== 1 ||
    gridSize[2] < 2
  ) {
    return {
      valid: false,
      reason: 'signal_field.grid_size must be [columns, 1, rows].',
    };
  }

  const columns = gridSize[0];
  const rows = gridSize[2];
  const count = columns * rows;
  if (
    count > MAX_FIELD_POINTS ||
    !Array.isArray(field.values) ||
    field.values.length !== count ||
    !field.values.every(isFiniteNumber)
  ) {
    return {
      valid: false,
      reason: 'signal_field.values must match the finite bounded grid exactly.',
    };
  }

  return {
    valid: true,
    columns,
    rows,
    values: [...field.values],
    cellSizeX: geometry.roomDimensions[0] / (columns - 1),
    cellSizeZ: geometry.roomDimensions[2] / (rows - 1),
    reason: null,
  };
}

export function resolveObservatoryRenderContract(frame) {
  const geometry = validateObservatoryGeometry(frame);
  const position = validateObservatoryPosition(frame, geometry);
  const signalField = validateObservatorySignalField(frame, geometry);

  return {
    geometry,
    position,
    signalField,
    showHardwareMarker: position.valid,
    showHardwareField: position.valid && signalField.valid,
  };
}

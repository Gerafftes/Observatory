/**
 * Canonical hardware identities shared by every RuView UI surface.
 *
 * Positions deliberately do not live here.  A device contributes only its
 * stable identity; the active sealed setup maps that identity to geometry.
 */
export const DEVICE_IDENTITIES = Object.freeze({
  TX1: Object.freeze({ id: 'TX1', role: 'transmitter', color: '#ff9f0a', threeColor: 0xff9f0a }),
  RX1: Object.freeze({ id: 'RX1', role: 'receiver', wireId: 1, color: '#0a84ff', threeColor: 0x0a84ff }),
  RX2: Object.freeze({ id: 'RX2', role: 'receiver', wireId: 2, color: '#30d158', threeColor: 0x30d158 }),
  RX3: Object.freeze({ id: 'RX3', role: 'receiver', wireId: 3, color: '#ff375f', threeColor: 0xff375f }),
  RX4: Object.freeze({ id: 'RX4', role: 'receiver', wireId: 4, color: '#bf5af2', threeColor: 0xbf5af2 }),
});

export const UNKNOWN_DEVICE_COLOR = '#68737a';
export const UNKNOWN_DEVICE_THREE_COLOR = 0x68737a;

/** Convert the numeric RX wire ID (or an RX label) to its canonical identity. */
export function receiverIdentity(value) {
  const id = typeof value === 'number' || /^\d+$/.test(String(value ?? ''))
    ? `RX${Number(value)}`
    : String(value ?? '').toUpperCase();
  const identity = DEVICE_IDENTITIES[id];
  return identity?.role === 'receiver' ? identity : null;
}

/** Resolve a known device. `TX` is the one supported legacy alias for TX1. */
export function deviceIdentity(value) {
  const id = String(value ?? '').toUpperCase();
  return DEVICE_IDENTITIES[id === 'TX' ? 'TX1' : id] || null;
}

export function deviceColor(value) {
  return deviceIdentity(value)?.color || UNKNOWN_DEVICE_COLOR;
}

export function deviceThreeColor(value) {
  return deviceIdentity(value)?.threeColor ?? UNKNOWN_DEVICE_THREE_COLOR;
}

export function receiverLabel(value) {
  return receiverIdentity(value)?.id || 'RX?';
}

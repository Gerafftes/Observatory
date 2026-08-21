'use strict';

function parseIqHex(hex) {
  if (typeof hex !== 'string' || hex.length % 2 !== 0 || /[^0-9a-f]/i.test(hex)) {
    throw new Error('IQ data must be an even-length hexadecimal string');
  }
  const values = new Int8Array(hex.length / 2);
  for (let index = 0; index < values.length; index++) {
    const unsigned = Number.parseInt(hex.slice(index * 2, index * 2 + 2), 16);
    values[index] = unsigned > 127 ? unsigned - 256 : unsigned;
  }
  return values;
}

function unwrapPhase(values) {
  if (values.length === 0) return [];
  const result = new Array(values.length);
  result[0] = values[0];
  let offset = 0;
  for (let index = 1; index < values.length; index++) {
    const delta = values[index] - values[index - 1];
    if (delta > Math.PI) offset -= 2 * Math.PI;
    if (delta < -Math.PI) offset += 2 * Math.PI;
    result[index] = values[index] + offset;
  }
  return result;
}

function medianFilter(values, radius = 1) {
  return values.map((_, index) => {
    const start = Math.max(0, index - radius);
    const end = Math.min(values.length, index + radius + 1);
    const window = values.slice(start, end).sort((a, b) => a - b);
    const middle = Math.floor(window.length / 2);
    return window.length % 2 === 0
      ? (window[middle - 1] + window[middle]) / 2
      : window[middle];
  });
}

function removeLinearTrend(values) {
  const count = values.length;
  if (count < 2) return values.map(() => 0);
  const meanX = (count - 1) / 2;
  const meanY = values.reduce((sum, value) => sum + value, 0) / count;
  let numerator = 0;
  let denominator = 0;
  for (let index = 0; index < count; index++) {
    numerator += (index - meanX) * (values[index] - meanY);
    denominator += (index - meanX) ** 2;
  }
  const slope = denominator === 0 ? 0 : numerator / denominator;
  return values.map((value, index) => value - (meanY + slope * (index - meanX)));
}

function preprocessIq(iqValues, subcarriers, { skipDc = true } = {}) {
  const start = skipDc ? 2 : 0;
  if (!Number.isInteger(subcarriers) || subcarriers <= 0) {
    throw new Error('subcarriers must be a positive integer');
  }
  if (iqValues.length < start + subcarriers * 2) {
    throw new Error(`IQ frame has ${iqValues.length} values; expected at least ${start + subcarriers * 2}`);
  }

  const amplitude = new Array(subcarriers);
  const rawPhase = new Array(subcarriers);
  for (let carrier = 0; carrier < subcarriers; carrier++) {
    const index = start + carrier * 2;
    const i = iqValues[index];
    const q = iqValues[index + 1];
    amplitude[carrier] = Math.hypot(i, q);
    rawPhase[carrier] = Math.atan2(q, i);
  }

  const phase = removeLinearTrend(medianFilter(unwrapPhase(rawPhase)));
  return { amplitude, rawPhase, phase };
}

module.exports = {
  medianFilter,
  parseIqHex,
  preprocessIq,
  removeLinearTrend,
  unwrapPhase,
};

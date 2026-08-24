'use strict';

const TORSO_SCHEMA = 'torso-v1';
const RX_ORDER = Object.freeze(['RX1', 'RX2', 'RX3', 'RX4']);
const TORSO_KEYPOINTS = Object.freeze([
  'left_shoulder',
  'right_shoulder',
  'left_hip',
  'right_hip',
]);
const COCO_TORSO_INDICES = Object.freeze([5, 6, 11, 12]);

function canonicalRxId(nodeId) {
  const match = String(nodeId ?? '').toUpperCase().match(/(?:RX)?([1-4])$/);
  return match ? `RX${match[1]}` : null;
}

function torsoFromCoco(keypoints, fallbackConfidence = 0) {
  const torso = {};
  for (let index = 0; index < TORSO_KEYPOINTS.length; index++) {
    const point = keypoints[COCO_TORSO_INDICES[index]];
    if (!Array.isArray(point) || point.length < 2) {
      throw new Error(`missing COCO torso keypoint at index ${COCO_TORSO_INDICES[index]}`);
    }
    torso[TORSO_KEYPOINTS[index]] = {
      x: Number(point[0]),
      y: Number(point[1]),
      confidence: Number(point[2] ?? fallbackConfidence),
    };
  }
  return torso;
}

function flattenTimeMajorChannel(framesByRx, channel, timePoints = 20, subcarriers = 64) {
  const data = [];
  for (let time = 0; time < timePoints; time++) {
    for (const rxId of RX_ORDER) {
      const frame = framesByRx[rxId]?.[time];
      const values = frame?.[channel];
      if (!Array.isArray(values) || values.length !== subcarriers) {
        throw new Error(`${rxId} time ${time} must contain ${subcarriers} ${channel} values`);
      }
      data.push(...values);
    }
  }
  return data;
}

function timeMajorToFeatureMajor(data, timePoints, featureCount) {
  if (data.length !== timePoints * featureCount) {
    throw new Error(`channel has ${data.length} values; expected ${timePoints * featureCount}`);
  }
  const result = new Float32Array(data.length);
  for (let time = 0; time < timePoints; time++) {
    for (let feature = 0; feature < featureCount; feature++) {
      result[feature * timePoints + time] = data[time * featureCount + feature];
    }
  }
  return result;
}

function splitBySession(samples, evalFraction, seed = 42) {
  const groups = new Map();
  for (const sample of samples) {
    if (typeof sample.sessionId !== 'string' || sample.sessionId.length === 0) {
      throw new Error('torso-v1 samples require a non-empty session_id');
    }
    if (!groups.has(sample.sessionId)) groups.set(sample.sessionId, []);
    groups.get(sample.sessionId).push(sample);
  }
  if (groups.size < 2) {
    throw new Error('session-separated evaluation requires at least two session_id groups');
  }

  const sessionIds = [...groups.keys()].sort();
  let state = seed | 0;
  for (let index = sessionIds.length - 1; index > 0; index--) {
    state ^= state << 13; state ^= state >> 17; state ^= state << 5;
    const swap = (state >>> 0) % (index + 1);
    [sessionIds[index], sessionIds[swap]] = [sessionIds[swap], sessionIds[index]];
  }
  const evalCount = Math.max(1, Math.min(sessionIds.length - 1, Math.round(sessionIds.length * evalFraction)));
  const evalIds = new Set(sessionIds.slice(0, evalCount));
  return {
    train: samples.filter(sample => !evalIds.has(sample.sessionId)),
    eval: samples.filter(sample => evalIds.has(sample.sessionId)),
  };
}

module.exports = {
  COCO_TORSO_INDICES,
  RX_ORDER,
  TORSO_KEYPOINTS,
  TORSO_SCHEMA,
  canonicalRxId,
  flattenTimeMajorChannel,
  splitBySession,
  timeMajorToFeatureMajor,
  torsoFromCoco,
};

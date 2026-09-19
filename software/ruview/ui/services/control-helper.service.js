import { getServerControlOrigin } from './server-control.service.js';

export function getControlHelperOrigin() {
  return getServerControlOrigin();
}

export async function controlHelperRequest(path, options = {}) {
  const method = options.method || 'GET';
  const response = await fetch(`${getControlHelperOrigin()}${path}`, {
    method,
    cache: 'no-store',
    headers: {
      Accept: 'application/json',
      ...(method !== 'GET' ? { 'Content-Type': 'application/json' } : {}),
      ...(options.headers || {}),
    },
    ...(options.body === undefined ? {} : { body: options.body }),
  });
  const payload = await response.json().catch(() => ({}));
  if (!response.ok) {
    throw new Error(payload.error || `HTTP ${response.status}`);
  }
  return payload;
}

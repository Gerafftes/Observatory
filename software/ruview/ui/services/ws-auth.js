// Credentials stay in this tab, explicitly scoped to each server hostname.
// HTTP/WS ports may differ, but credentials never migrate to another host.
export function getApiToken(url = globalThis.location?.href) {
  if (!globalThis.location || !url) return '';
  const target = new URL(url, globalThis.location.href);
  try {
    return globalThis.sessionStorage?.getItem(`ruview.api-token:${target.hostname}`) || '';
  } catch {
    return '';
  }
}

export function setApiToken(token, url = globalThis.location.href) {
  const target = new URL(url, globalThis.location.href);
  if (!['http:', 'https:', 'ws:', 'wss:'].includes(target.protocol)) throw new Error('Use an HTTP or WebSocket server URL');
  const key = `ruview.api-token:${target.hostname}`;
  if (token) globalThis.sessionStorage.setItem(key, token);
  else globalThis.sessionStorage.removeItem(key);
}

export function websocketProtocols(token) {
  if (!token) return [];
  const hex = Array.from(new TextEncoder().encode(token), byte => byte.toString(16).padStart(2, '0')).join('');
  return ['ruview.v1', `ruview.bearer.${hex}`];
}

export function sensingProtocols(url, token = getApiToken(url)) {
  const path = new URL(url, globalThis.location?.href).pathname;
  return ['/ws/sensing', '/ws/introspection', '/api/v1/stream/pose'].includes(path)
    ? websocketProtocols(token) : [];
}

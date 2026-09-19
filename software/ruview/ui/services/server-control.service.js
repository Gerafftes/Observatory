const DEFAULT_CONTROL_PORT = 8090;

export function getServerControlOrigin() {
  const configured = globalThis?.__RUVIEW_SERVER_CONTROL_ORIGIN__;
  if (typeof configured === 'string' && configured.trim()) {
    return configured.trim().replace(/\/$/, '');
  }

  const search = typeof window !== 'undefined' ? new URLSearchParams(window.location.search) : null;
  const configuredPort = Number(search?.get('server_control_port'));
  const port = Number.isInteger(configuredPort) && configuredPort > 0
    ? configuredPort
    : DEFAULT_CONTROL_PORT;
  const protocol = typeof window !== 'undefined' && window.location.protocol === 'https:'
    ? 'https:'
    : 'http:';
  return `${protocol}//127.0.0.1:${port}`;
}
export async function serverControlRequest(action = 'status', options = {}) {
  const method = options.method || (action === 'status' ? 'GET' : 'POST');
  const response = await fetch(`${getServerControlOrigin()}/${action}`, {
    method,
    cache: 'no-store',
    headers: {
      Accept: 'application/json',
      ...(method === 'POST' ? { 'Content-Type': 'application/json' } : {}),
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

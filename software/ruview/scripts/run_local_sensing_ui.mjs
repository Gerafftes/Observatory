#!/usr/bin/env node

import { createReadStream } from 'node:fs';
import { stat } from 'node:fs/promises';
import { createServer, request as httpRequest } from 'node:http';
import { extname, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

const listenHost = '127.0.0.1';
const listenPort = Number.parseInt(process.env.RUVIEW_UI_PORT ?? '3002', 10);
const backendHost = '127.0.0.1';
const backendHttpPort = 8080;
const backendWebSocketPort = 8765;
const uiRoot = resolve(fileURLToPath(new URL('../ui/', import.meta.url)));

const contentTypes = new Map([
  ['.css', 'text/css; charset=utf-8'],
  ['.html', 'text/html; charset=utf-8'],
  ['.ico', 'image/x-icon'],
  ['.jpeg', 'image/jpeg'],
  ['.jpg', 'image/jpeg'],
  ['.js', 'text/javascript; charset=utf-8'],
  ['.json', 'application/json; charset=utf-8'],
  ['.mjs', 'text/javascript; charset=utf-8'],
  ['.png', 'image/png'],
  ['.svg', 'image/svg+xml'],
  ['.wasm', 'application/wasm'],
  ['.webp', 'image/webp'],
]);

function isBackendPath(pathname) {
  return pathname === '/health' || pathname.startsWith('/health/') || pathname.startsWith('/api/');
}

function proxyHttp(clientRequest, clientResponse) {
  const headers = { ...clientRequest.headers, host: `${backendHost}:${backendHttpPort}` };
  const proxyRequest = httpRequest(
    {
      hostname: backendHost,
      port: backendHttpPort,
      method: clientRequest.method,
      path: clientRequest.url,
      headers,
    },
    (proxyResponse) => {
      clientResponse.writeHead(proxyResponse.statusCode ?? 502, proxyResponse.headers);
      proxyResponse.pipe(clientResponse);
    },
  );
  proxyRequest.on('error', (error) => {
    if (!clientResponse.headersSent) {
      clientResponse.writeHead(502, { 'content-type': 'application/json; charset=utf-8' });
    }
    clientResponse.end(JSON.stringify({ error: `Backend unavailable: ${error.message}` }));
  });
  clientRequest.pipe(proxyRequest);
}

async function serveStatic(clientRequest, clientResponse, pathname) {
  let decodedPath;
  try {
    decodedPath = decodeURIComponent(pathname);
  } catch {
    clientResponse.writeHead(400).end('Invalid URL encoding');
    return;
  }

  const relativePath = decodedPath === '/' ? 'index.html' : decodedPath.replace(/^\/+/, '');
  const filePath = resolve(uiRoot, relativePath);
  if (filePath !== uiRoot && !filePath.startsWith(`${uiRoot}${sep}`)) {
    clientResponse.writeHead(403).end('Forbidden');
    return;
  }

  try {
    const fileStat = await stat(filePath);
    if (!fileStat.isFile()) {
      clientResponse.writeHead(404).end('Not found');
      return;
    }
    clientResponse.writeHead(200, {
      'cache-control': 'no-store',
      'content-length': fileStat.size,
      'content-type': contentTypes.get(extname(filePath).toLowerCase()) ?? 'application/octet-stream',
    });
    createReadStream(filePath).pipe(clientResponse);
  } catch (error) {
    clientResponse.writeHead(error.code === 'ENOENT' ? 404 : 500).end(
      error.code === 'ENOENT' ? 'Not found' : 'Static file error',
    );
  }
}

function proxyWebSocket(clientRequest, clientSocket, clientHead) {
  const headers = {
    ...clientRequest.headers,
    connection: 'Upgrade',
    host: `${backendHost}:${backendWebSocketPort}`,
    upgrade: 'websocket',
  };
  const proxyRequest = httpRequest({
    hostname: backendHost,
    port: backendWebSocketPort,
    method: clientRequest.method,
    path: clientRequest.url,
    headers,
  });

  proxyRequest.on('upgrade', (proxyResponse, proxySocket, proxyHead) => {
    const statusLine = `HTTP/${proxyResponse.httpVersion} ${proxyResponse.statusCode} ${proxyResponse.statusMessage}\r\n`;
    const responseHeaders = Object.entries(proxyResponse.headers)
      .flatMap(([name, value]) => {
        const values = Array.isArray(value) ? value : [value];
        return values.filter(Boolean).map((item) => `${name}: ${item}\r\n`);
      })
      .join('');
    clientSocket.write(`${statusLine}${responseHeaders}\r\n`);
    if (clientHead.length > 0) proxySocket.write(clientHead);
    if (proxyHead.length > 0) clientSocket.write(proxyHead);
    proxySocket.pipe(clientSocket);
    clientSocket.pipe(proxySocket);
  });
  proxyRequest.on('response', (proxyResponse) => {
    clientSocket.end(`HTTP/1.1 ${proxyResponse.statusCode ?? 502} Bad Gateway\r\n\r\n`);
  });
  proxyRequest.on('error', () => clientSocket.destroy());
  proxyRequest.end();
}

const server = createServer((clientRequest, clientResponse) => {
  const pathname = new URL(clientRequest.url ?? '/', 'http://localhost').pathname;
  if (isBackendPath(pathname)) {
    proxyHttp(clientRequest, clientResponse);
    return;
  }
  void serveStatic(clientRequest, clientResponse, pathname);
});

server.on('upgrade', (request, socket, head) => {
  const pathname = new URL(request.url ?? '/', 'http://localhost').pathname;
  if (pathname !== '/ws/sensing') {
    socket.end('HTTP/1.1 404 Not Found\r\n\r\n');
    return;
  }
  proxyWebSocket(request, socket, head);
});

server.listen(listenPort, listenHost, () => {
  console.log(`RuView sensing UI: http://${listenHost}:${listenPort}/`);
  console.log(`HTTP backend: http://${backendHost}:${backendHttpPort}/`);
  console.log(`WebSocket backend: ws://${backendHost}:${backendWebSocketPort}/ws/sensing`);
});

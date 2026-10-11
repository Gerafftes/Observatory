const assert = require('node:assert/strict');
const { test } = require('node:test');
const fs = require('node:fs');
const path = require('node:path');
const os = require('node:os');
const vm = require('node:vm');
const https = require('node:https');
const { execFileSync } = require('node:child_process');

const root = path.resolve(__dirname, '../..');
const read = name => fs.readFileSync(path.join(root, name), 'utf8');
const viewer = read('v2/crates/wifi-densepose-pointcloud/src/viewer.html');
const wall = read('examples/through-wall/index.html');

// A sink-sensitive DOM fixture: any attempted HTML parsing fails the test.
class Element {
  constructor(tag = 'div') { this.tagName = tag; this.children = []; this.style = {}; }
  set innerHTML(_) { throw new Error('Untrusted display path must not parse HTML'); }
  set textContent(value) { this.children = [String(value)]; }
  get textContent() { return this.children.map(child => typeof child === 'string' ? child : child.textContent).join(''); }
  append(...children) { this.children.push(...children.map(child => typeof child === 'object' ? child : String(child))); }
  replaceChildren(...children) { this.children = []; this.append(...children); }
}

function viewerContext(options = {}) {
  const stats = new Element();
  const context = vm.createContext({
    document: { createElement: tag => new Element(tag), getElementById: () => stats },
    backendArg: 'demo', requireLive: false, transportMode: 'demo', networkDisabled: false,
    lastFrame: -1, prevTimestamp: 0, frameRateVal: '0.0', faceMeshState: 'idle',
    latestFaceLandmarks: null, BASE_INTERVAL_MS: 250, MAX_INTERVAL_MS: 30000,
    currentIntervalMs: 250, consecutiveFailures: 0, fetchTimer: null,
    updateSplats() {}, drawSkeleton() {}, clearSkeleton() {}, pushFaceSplats() {},
    setTimeout() { return 1; }, clearTimeout() {}, buildSplatsUrl() { return 'https://unused.invalid/api/splats'; },
    pickDemoFrame() { return { splats: [], frame: 1 }; },
    fetch: async () => { throw new Error('offline'); },
    ...options,
  });
  vm.runInContext(viewer.slice(viewer.indexOf('        function scheduleNextFetch('), viewer.indexOf('        // Wire the camera CTA.')), context);
  return { context, stats };
}

test('both HTML pages retain syntactically valid inline JavaScript', () => {
  for (const html of [viewer, wall]) {
    for (const [, script] of html.matchAll(/<script\b[^>]*>([\s\S]*?)<\/script>/g)) new vm.Script(script);
  }
});

test('pointcloud stats render query, remote fields and error values only as text', async () => {
  for (const payload of ['<img src=x onerror=alert(1)>', '</span><svg onload=alert(1)>', '&lt;script&gt; & " \'']) {
    const backendArg = new URLSearchParams(`backend=${encodeURIComponent(payload)}`).get('backend');
    const { context, stats } = viewerContext({ backendArg });
    await context.fetchCloud();
    assert.ok(stats.textContent.includes(payload));
    assert.match(stats.textContent, /unreachable.*retry in 1s/s);
    context.transportMode = 'remote';
    context.handleData({ splats: [], frame: payload, pipeline: {
      weather: { temperature: payload, conditions: payload },
      geo: { building_count: payload }, vitals: { breathing_rate: payload },
    } });
    assert.ok(stats.textContent.includes(`REMOTE ${payload}`));
    assert.ok(stats.textContent.includes(`Frame: ${payload}`));
    assert.ok(stats.textContent.includes(`Weather: ${payload}°C ${payload}`));
    assert.ok(stats.textContent.includes(`Buildings: ${payload}`));
    assert.ok(stats.textContent.includes(`BR ${payload}/min`));
    const strict = viewerContext({ backendArg, requireLive: true, fetch: async () => { throw new Error(payload); } });
    await strict.context.fetchCloud();
    assert.ok(strict.stats.textContent.includes(`OFFLINE`));
    assert.ok(strict.stats.textContent.includes(payload));
  }
});

test('pointcloud transport badges, fallback, recovery and overlays remain intact', async () => {
  const demo = viewerContext();
  await demo.context.fetchCloud();
  assert.match(demo.stats.textContent, /DEMO SyntheticSplats: 0Frame: 1CSI Rate: 0.0 fps/);
  const auto = viewerContext({ backendArg: 'auto' });
  await auto.context.fetchCloud();
  assert.equal(auto.context.networkDisabled, true);
  assert.equal(auto.context.transportMode, 'demo');
  for (const backendArg of ['auto', 'https://seed.example']) {
    const { context, stats } = viewerContext({ backendArg, consecutiveFailures: 3,
      fetch: async () => ({ ok: true, json: async () => ({ splats: [], frame: 2, pipeline: {
        skeleton: { keypoints: [], confidence: 0.8 }, weather: { temperature: 21, conditions: 'clear' },
        geo: { building_count: 2 }, vitals: { breathing_rate: 12, motion_score: 0.1 },
      } }) }),
    });
    await context.fetchCloud();
    assert.match(stats.textContent, backendArg === 'auto' ? /LIVE Local Backend/ : /REMOTE https:\/\/seed.example/);
    assert.equal(context.consecutiveFailures, 0);
    assert.equal(context.currentIntervalMs, 250);
    assert.match(stats.textContent, /80% confidenceWeather: 21°C clearBuildings: 2Vitals: BR 12\/min Motion 10%/);
  }
  const face = viewerContext({ faceMeshState: 'running', latestFaceLandmarks: [] });
  face.context.handleData({ splats: [], frame: 1 });
  assert.match(face.stats.textContent, /\+ face overlay/);
  face.context.handleData({ splats: [], frame: 2, source: 'face-mesh' });
  assert.match(face.stats.textContent, /DEMO Your Face \(MediaPipe\)/);
});

test('through-wall banner uses literal text and preserves LIVE classification and pluralization', () => {
  const banner = new Element();
  const context = vm.createContext({ banner, document: { createElement: tag => new Element(tag) } });
  vm.runInContext(wall.slice(wall.indexOf('function setBannerLive('), wall.indexOf('function setBannerNoServer(')), context);
  for (const source of ['<img src=x onerror=alert(1)>', '</span><svg onload=alert(1)>', '&lt;img&gt;', 'unknown']) {
    context.setBannerLive(source, 1);
    assert.equal(banner.className, 'sim');
    assert.ok(banner.textContent.includes(`source=${source}`));
    assert.equal(banner.children[1].className, 'src');
  }
  for (const count of [0, 1, 2]) {
    context.setBannerLive('esp32', count);
    assert.equal(banner.className, 'live');
    assert.equal(banner.textContent, `LIVE — real ESP32 CSI (source=esp32, ${count} node${count === 1 ? '' : 's'})`);
  }
});

function seedCalls(transport, baseUrl) {
  const camera = read('scripts/train-camera-free.js');
  const spectrogram = read('scripts/csi-spectrogram.js');
  const context = vm.createContext({
    https: transport, URL, Buffer, process: { env: {} }, console,
    require(name) { assert.equal(name, 'https'); return transport; },
    args: { 'seed-url': baseUrl, 'seed-token': 'synthetic-test-token' }, setTimeout,
  });
  vm.runInContext(camera.slice(camera.indexOf('class SeedClient {'), camera.indexOf('// Data loading (')) + '\nthis.SeedClient = SeedClient;', context);
  vm.runInContext(spectrogram.slice(spectrogram.indexOf('async function ingestToSeed('), spectrogram.indexOf('// File Mode:')), context);
  const client = new context.SeedClient(baseUrl, 'synthetic-test-token');
  return {
    get: () => client._get('/read'), post: () => client._post('/write', { synthetic: true }),
    stream: () => client.streamSensors(30),
    ingest: () => context.ingestToSeed(new Float32Array([0.5]), { nodeId: 1, windowIdx: 0, timestamp: 0, rssi: -50, nSubcarriers: 1 }),
  };
}

test('all four Seed paths reject untrusted/mismatched certificates and accept a trusted local peer', async t => {
  const temporary = fs.mkdtempSync(path.join(process.env.SECURITY_TEST_TMPDIR || os.tmpdir(), 'seed-tls-'));
  // Disposable synthetic credentials, never a device certificate or recording.
  t.after(() => fs.rmSync(temporary, { recursive: true, force: true }));
  const keyPath = path.join(temporary, 'key.pem');
  const certPath = path.join(temporary, 'cert.pem');
  execFileSync('openssl', ['req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-days', '1',
    '-subj', '/CN=localhost', '-addext', 'subjectAltName=DNS:localhost', '-keyout', keyPath, '-out', certPath], { stdio: 'ignore' });
  const cert = fs.readFileSync(certPath);
  const requests = [];
  const server = https.createServer({ key: fs.readFileSync(keyPath), cert }, (req, res) => {
    let body = '';
    req.on('data', chunk => { body += chunk; });
    req.on('end', () => {
      requests.push({ url: req.url, method: req.method, authorization: req.headers.authorization, body });
      res.end(req.url.endsWith('/stream') ? 'data: {"synthetic":true}\n\n' : '{"synthetic":true}');
    });
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  t.after(() => new Promise(resolve => { server.closeAllConnections(); server.close(resolve); }));
  const port = server.address().port;
  for (const scenario of ['untrusted', 'wrong-host', 'trusted']) {
    const agent = new https.Agent(scenario === 'untrusted' ? {} : { ca: cert });
    t.after(() => agent.destroy());
    const transport = {};
    for (const method of ['get', 'request']) {
      transport[method] = (...args) => {
        const optionIndex = typeof args[0] === 'object' && !(args[0] instanceof URL) ? 0 : 1;
        assert.equal(args[optionIndex].rejectUnauthorized, true);
        args[optionIndex] = { ...args[optionIndex], agent, family: 4 };
        return https[method](...args);
      };
    }
    const baseUrl = `https://${scenario === 'wrong-host' ? '127.0.0.1' : 'localhost'}:${port}`;
    const calls = seedCalls(transport, baseUrl);
    if (scenario === 'trusted') {
      assert.equal((await calls.get()).synthetic, true);
      assert.equal((await calls.post()).synthetic, true);
      assert.equal((await calls.ingest()).synthetic, true);
      assert.equal((await calls.stream())[0].synthetic, true);
      assert.deepEqual(requests.map(request => request.method), ['GET', 'POST', 'POST', 'GET']);
      assert.ok(requests.every(request => request.authorization === 'Bearer synthetic-test-token'));
      assert.equal(JSON.parse(requests[1].body).synthetic, true);
      assert.equal(JSON.parse(requests[2].body).vectors[0].values[0], 0.5);
    } else {
      assert.equal(await calls.get(), null);
      assert.equal(await calls.post(), null);
      assert.equal((await calls.stream()).length, 0);
      await assert.rejects(calls.ingest(), { code: scenario === 'untrusted' ? 'DEPTH_ZERO_SELF_SIGNED_CERT' : 'ERR_TLS_CERT_ALTNAME_INVALID' });
      assert.equal(requests.length, 0, 'no authenticated HTTP payload reaches a rejected peer');
    }
  }
});

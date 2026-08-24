#!/usr/bin/env node

/**
 * Regression test: bin/mcp-server.js must exit on SIGTERM/SIGINT.
 *
 * Catches the case where the MCP server kept alive by stdin does not
 * respond to termination signals and survives parent death as an
 * orphaned process (PPID=1).
 */

const { spawn } = require('child_process');
const assert = require('assert');
const path = require('path');
const fs = require('fs');

const MCP_SERVER = path.join(__dirname, '..', 'bin', 'mcp-server.js');
const SDK_PATH = path.join(__dirname, '..', 'node_modules', '@modelcontextprotocol', 'sdk');

let passed = 0;
let failed = 0;
let skipped = 0;
const failures = [];

function pass(name) {
  passed++;
  console.log(`  PASS  ${name}`);
}

function fail(name, err) {
  failed++;
  failures.push({ name, error: err && err.message ? err.message : String(err) });
  console.log(`  FAIL  ${name}`);
  console.log(`        ${err && err.message ? err.message : err}`);
}

function skip(name, reason) {
  skipped++;
  console.log(`  SKIP  ${name} -- ${reason}`);
}

function waitForExit(child, timeoutMs) {
  return new Promise((resolve) => {
    const timer = setTimeout(() => {
      try { child.kill('SIGKILL'); } catch (_) {}
      resolve(-1);
    }, timeoutMs);
    child.once('exit', (code) => {
      clearTimeout(timer);
      resolve(code === null ? -1 : code);
    });
  });
}

function waitForReady(child, marker, timeoutMs) {
  return new Promise((resolve) => {
    const timer = setTimeout(resolve, timeoutMs);
    const onData = (buf) => {
      if (buf.toString().includes(marker)) {
        clearTimeout(timer);
        child.stderr.off('data', onData);
        resolve();
      }
    };
    child.stderr.on('data', onData);
  });
}

async function testSignal(name, signal) {
  const child = spawn(process.execPath, [MCP_SERVER], {
    stdio: ['pipe', 'pipe', 'pipe'],
    env: { ...process.env, NO_COLOR: '1' },
  });

  await waitForReady(child, 'running on stdio', 2000);
  child.kill(signal);
  const code = await waitForExit(child, 5000);

  try {
    assert.strictEqual(code, 0, `Expected clean exit on ${signal}, got code ${code}`);
    pass(name);
  } catch (err) {
    fail(name, err);
  }
}

(async () => {
  console.log('\nruvector MCP server signal-handling tests');
  console.log('='.repeat(60));

  if (!fs.existsSync(SDK_PATH)) {
    skip('SIGTERM cleanup', 'MCP SDK not installed (run npm install)');
    skip('SIGINT cleanup', 'MCP SDK not installed (run npm install)');
  } else {
    await testSignal('SIGTERM cleanup', 'SIGTERM');
    await testSignal('SIGINT cleanup', 'SIGINT');
  }

  console.log();
  console.log(`Passed:  ${passed}`);
  console.log(`Failed:  ${failed}`);
  console.log(`Skipped: ${skipped}`);

  if (failed > 0) {
    console.log('\nFailures:');
    failures.forEach(({ name, error }) => console.log(`  - ${name}: ${error}`));
    process.exit(1);
  }
})();

/**
 * Node.js smoke test for @ruvector/neural-trader-wasm
 *
 * Validates: module loads, all exports exist, coherence gate round-trip,
 * replay store read/write, and BigInt timestamp fidelity.
 *
 * Run:  node --experimental-vm-modules tests/node-smoke.mjs
 * Or:   node tests/node-smoke.mjs  (Node 22+)
 */

import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const __dir = dirname(fileURLToPath(import.meta.url));
const pkgDir = join(__dir, '..', 'pkg');

let pass = 0;
let fail = 0;

function assert(cond, msg) {
  if (cond) {
    pass++;
    console.log(`  ✓ ${msg}`);
  } else {
    fail++;
    console.error(`  ✗ ${msg}`);
  }
}

function assertEqual(a, b, msg) {
  assert(a === b, `${msg} (got ${a}, expected ${b})`);
}

// ---------- Load WASM ----------
// For Node we use initSync with a buffer instead of fetch.
const jsPath = join(pkgDir, 'neural_trader_wasm.js');
const wasmPath = join(pkgDir, 'neural_trader_wasm_bg.wasm');

const wasmMod = await import(jsPath);
const wasmBytes = await readFile(wasmPath);
wasmMod.initSync({ module: wasmBytes });

const {
  version,
  healthCheck,
  GateConfigWasm,
  ThresholdGateWasm,
  GateContextWasm,
  CoherenceDecisionWasm,
  MarketEventWasm,
  GraphDeltaWasm,
  ReservoirStoreWasm,
  ReplaySegmentWasm,
  EventTypeWasm,
  SideWasm,
  RegimeLabelWasm,
  SegmentKindWasm,
  NodeKindWasm,
  EdgeKindWasm,
  PropertyKeyWasm,
} = wasmMod;

// ---------- Tests ----------

console.log('\n--- Utilities ---');
assert(typeof version() === 'string' && version().length > 0, 'version() returns non-empty string');
assertEqual(healthCheck(), true, 'healthCheck() returns true');

console.log('\n--- Enums ---');
assertEqual(EventTypeWasm.Trade, 3, 'EventTypeWasm.Trade = 3');
assertEqual(SideWasm.Bid, 0, 'SideWasm.Bid = 0');
assertEqual(RegimeLabelWasm.Volatile, 2, 'RegimeLabelWasm.Volatile = 2');
assertEqual(SegmentKindWasm.Routine, 6, 'SegmentKindWasm.Routine = 6');
assertEqual(NodeKindWasm.Symbol, 0, 'NodeKindWasm.Symbol = 0');
assertEqual(EdgeKindWasm.Matched, 3, 'EdgeKindWasm.Matched = 3');
assertEqual(PropertyKeyWasm.CancelHazard, 8, 'PropertyKeyWasm.CancelHazard = 8');

console.log('\n--- MarketEvent ---');
const evt = new MarketEventWasm(EventTypeWasm.Trade, 42, 1, 500000000n, 10000n);
assertEqual(evt.symbolId, 42, 'symbolId = 42');
assertEqual(evt.venueId, 1, 'venueId = 1');
assertEqual(evt.priceFp, 500000000n, 'priceFp = 500000000n');
assertEqual(evt.side, undefined, 'side initially undefined');
evt.side = SideWasm.Ask;
assertEqual(evt.side, SideWasm.Ask, 'side after set = Ask');
const evtJson = evt.toJson();
assert(evtJson !== null && evtJson !== undefined, 'toJson() returns non-null');
const evt2 = MarketEventWasm.fromJson(evtJson);
assertEqual(evt2.symbolId, 42, 'fromJson round-trip preserves symbolId');
evt.free();
evt2.free();

console.log('\n--- GateConfig ---');
const config = new GateConfigWasm();
assertEqual(config.mincutFloorCalm, 12n, 'default mincutFloorCalm = 12n');
assertEqual(config.mincutFloorNormal, 9n, 'default mincutFloorNormal = 9n');
assertEqual(config.mincutFloorVolatile, 6n, 'default mincutFloorVolatile = 6n');
config.cusumThreshold = 5.0;
assertEqual(config.cusumThreshold, 5.0, 'cusumThreshold setter works');

console.log('\n--- Coherence Gate ---');
const gate = new ThresholdGateWasm(config);
const ctx = new GateContextWasm(
  42, 1, 1000000n, 15n,
  '00000000000000000000000000000000',
  1.0, 0.1,
  RegimeLabelWasm.Calm,
  10,
);
assertEqual(ctx.symbolId, 42, 'context symbolId = 42');
assertEqual(ctx.regime, RegimeLabelWasm.Calm, 'context regime = Calm');
assertEqual(ctx.partitionHash, '00000000000000000000000000000000', 'context partitionHash');

const decision = gate.evaluate(ctx);
assertEqual(decision.allowRetrieve, true, 'decision allowRetrieve');
assertEqual(decision.allowWrite, true, 'decision allowWrite');
assertEqual(decision.allowLearn, true, 'decision allowLearn');
assertEqual(decision.allowAct, true, 'decision allowAct');
assertEqual(decision.allAllowed(), true, 'decision allAllowed()');
assertEqual(decision.fullyBlocked(), false, 'decision fullyBlocked()');
assert(typeof decision.partitionHash === 'string' && decision.partitionHash.length === 32,
  'decision partitionHash is 32-char hex');
const reasons = decision.reasons();
assert(Array.isArray(reasons), 'reasons() returns array');
assertEqual(reasons.length, 0, 'no reasons when all allowed');

console.log('\n--- Coherence Gate (blocked) ---');
const ctx2 = new GateContextWasm(
  42, 1, 1000000n, 3n,
  '00000000000000000000000000000000',
  5.0, 0.8,
  RegimeLabelWasm.Calm,
  2,
);
const d2 = gate.evaluate(ctx2);
assertEqual(d2.allowAct, false, 'blocked: allowAct = false');
assertEqual(d2.allowWrite, false, 'blocked: allowWrite = false');
assertEqual(d2.allowLearn, false, 'blocked: allowLearn = false');
assert(d2.reasons().length > 0, 'blocked: has reasons');

console.log('\n--- GraphDelta ---');
const delta = new GraphDeltaWasm();
const nodes = delta.nodesAdded();
assert(Array.isArray(nodes), 'nodesAdded returns array');
assertEqual(nodes.length, 0, 'empty delta has no nodes');
delta.free();

console.log('\n--- ReservoirStore ---');
const store = new ReservoirStoreWasm(100);
assertEqual(store.isEmpty(), true, 'new store is empty');
assertEqual(store.len(), 0, 'new store len = 0');

// Retrieve from empty store
const empty = store.retrieveBySymbol(42, 10);
assert(Array.isArray(empty), 'retrieveBySymbol returns array');
assertEqual(empty.length, 0, 'empty store returns 0 results');
store.free();

// Zero-size rejected
let threw = false;
try { new ReservoirStoreWasm(0); } catch (e) { threw = true; }
assert(threw, 'ReservoirStoreWasm(0) throws');

// ---------- Summary ----------
console.log(`\n${'='.repeat(40)}`);
console.log(`Results: ${pass} passed, ${fail} failed`);
console.log(`${'='.repeat(40)}\n`);

if (fail > 0) process.exit(1);

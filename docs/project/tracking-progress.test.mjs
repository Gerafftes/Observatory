import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';

const html = readFileSync(new URL('./tracking-progress.html', import.meta.url), 'utf8');
const core = html.match(/<script id="progress-core">([\s\S]*?)<\/script>/)[1];
const context = {};
runInNewContext(core, context);
const { PLAN, SCHEDULE_ID, TASK_IDS, REVIEW_REVISION, restoreChecks, initialChecks, dayDifference, calculateProgress, validateSnapshot } = context.ProgressTracker;
const blank = () => Object.fromEntries([...TASK_IDS].map(id => [id, false]));
const allDone = () => Object.fromEntries([...TASK_IDS].map(id => [id, true]));
function finishThrough(checked, date) {
  for (const day of PLAN) if (day.date <= date) for (const task of day.tasks) checked[task.id] = true;
  return checked;
}

test('the reviewed initial snapshot completes all five day-14 gates while tracking remains open', () => {
  const checked = initialChecks();
  const model = calculateProgress(checked, 'single', '2026-10-14');
  assert.equal(checked['rx2-state'], true);
  assert.equal(checked['rx2-running'], true);
  assert.equal(checked['rx2-transport'], true);
  assert.equal(checked['rx2-grid-fix'], true);
  assert.equal(checked['loss-fix'], true);
  assert.equal(checked['identities'], true);
  assert.equal(checked['joint-diagnostic'], true);
  assert.equal(checked['peer-feasibility'], true);
  assert.equal(checked['peer-implementation'], true);
  assert.equal(checked['g1'], true);
  assert.equal(checked['peer-trial'], true);
  for (const id of ['data-rules', 'data-accounting', 'peer-pair-test']) assert.equal(checked[id], true);
  for (const id of ['peer-benefit-decision', 'peer-rollout']) assert.equal(checked[id], false);
  assert.equal(checked['assistant-offline'], true);
  assert.equal(checked['model-compare'], false);
  assert.equal(checked['coordinate-output'], false);
  assert.equal(model.completed, 18);
  assert.equal(model.todayOpen, 0);
  assert.equal(model.state, 'ahead');
  assert.equal(model.actual, (3 + 1 / 4) / 14);
});

test('unfinished peer test, benefit decision and decision execution prevent complete required progress', () => {
  const checked = allDone();
  for (const id of ['peer-implementation', 'peer-pair-test', 'peer-benefit-data', 'peer-benefit-decision', 'peer-rollout']) checked[id] = false;
  const model = calculateProgress(checked, 'single', '2026-10-25');
  assert.equal(checked['peer-feasibility'], true);
  assert.equal(model.overdue, 5);
  assert.equal(model.completed, model.total - 5);
  assert.ok(model.actual < 1);
  assert.equal(model.state, 'late');
});

test('today is not overdue; the next date makes unfinished work overdue', () => {
  assert.equal(calculateProgress(blank(), 'single', '2026-10-12').overdue, 0);
  const tomorrow = calculateProgress({ ...initialChecks(), 'rx2-state':false }, 'single', '2026-10-13');
  assert.equal(tomorrow.overdue, 1);
  assert.equal(tomorrow.state, 'late');
  assert.equal(tomorrow.planned, 1 / 14);
});

test('future achievements cannot hide an overdue task', () => {
  const checked = allDone();
  checked['rx2-state'] = false;
  const model = calculateProgress(checked, 'single', '2026-10-13');
  assert.ok(model.futureDone > 0);
  assert.equal(model.overdue, 1);
  assert.equal(model.state, 'late');
});

test('being ahead requires finishing today as well as some future work', () => {
  const checked = finishThrough(blank(), '2026-10-12');
  checked['data-rules'] = true;
  assert.equal(calculateProgress(checked, 'single', '2026-10-13').state, 'on-track');
  finishThrough(checked, '2026-10-13');
  assert.equal(calculateProgress(checked, 'single', '2026-10-13').state, 'ahead');
});

test('the single-person deadline excludes two-person tasks', () => {
  const checked = blank();
  for (const day of PLAN) for (const task of day.tasks) if (task.goal === 'multi') checked[task.id] = true;
  assert.equal(calculateProgress(checked, 'single', '2026-10-12').actual, 0);
  assert.ok(calculateProgress(checked, 'all', '2026-10-12').actual > 0);
  assert.equal(calculateProgress(checked, 'single', '2026-10-12').days.length, 14);
  assert.equal(calculateProgress(checked, 'all', '2026-10-12').days.length, 16);
});

test('deadline day and the day after use calendar-day boundaries', () => {
  assert.equal(calculateProgress(blank(), 'single', '2026-10-12').daysLeft, 13);
  assert.equal(calculateProgress(blank(), 'single', '2026-10-25').daysLeft, 0);
  assert.equal(calculateProgress(blank(), 'single', '2026-10-26').daysLeft, -1);
  assert.equal(calculateProgress(blank(), 'single', '2026-10-26').planned, 1);
});

test('the German daylight-saving transition does not shift the day count', () => {
  assert.equal(dayDifference('2026-10-24', '2026-10-26'), 2);
  assert.equal(dayDifference('2026-10-25', '2026-10-27'), 2);
});

test('completed scope reaches exactly 100 percent and has no overdue tasks', () => {
  for (const goal of ['single', 'all']) {
    const model = calculateProgress(allDone(), goal, '2026-10-28');
    assert.equal(model.actual, 1);
    assert.equal(model.state, 'complete');
    assert.equal(model.overdue, 0);
    assert.equal(model.completed, model.total);
  }
});

test('invalid dates are rejected instead of silently rolling into the next month', () => {
  for (const date of ['', '2026-02-30', '2026-13-01', '12.10.2026']) {
    assert.throws(() => calculateProgress(blank(), 'single', date), /Ungültiges Datum/);
  }
});

test('snapshots round-trip without interpreting task strings as HTML', () => {
  const checked = initialChecks();
  const restored = validateSnapshot(JSON.parse(JSON.stringify({ schemaVersion:1, scheduleId:SCHEDULE_ID, checked })));
  assert.equal(JSON.stringify(restored), JSON.stringify(checked));
  assert.equal(Object.keys(restored).length, TASK_IDS.size);
});

test('previous snapshots preserve manual checks without treating old trial preparation as a completed pair test', () => {
  const checked = { 'rx2-state':false, 'loss-review':true, 'peer-feasibility':true, 'peer-trial':true };
  const restored = validateSnapshot({ schemaVersion:1, scheduleId:SCHEDULE_ID, checked });
  for (const [id, done] of Object.entries(checked)) assert.equal(restored[id], done);
  for (const id of ['rx2-transport', 'peer-implementation', 'peer-pair-test', 'peer-benefit-data', 'peer-benefit-decision', 'peer-rollout']) assert.equal(restored[id], false);
  assert.equal(Object.keys(restored).length, TASK_IDS.size);
});

test('foreign snapshots, unknown tasks, and non-boolean values are rejected', () => {
  for (const value of [null, {}, { schemaVersion:1, scheduleId:'foreign', checked:{} },
    { schemaVersion:1, scheduleId:SCHEDULE_ID, checked:{ injected:'<script>' } },
    { schemaVersion:1, scheduleId:SCHEDULE_ID, checked:{ 'rx2-state':'false' } },
    { schemaVersion:1, scheduleId:SCHEDULE_ID, checked:[] }]) {
    assert.throws(() => validateSnapshot(value));
  }
});

test('the calendar has unique task IDs and consecutive dates starting on Monday', () => {
  const tasks = PLAN.flatMap(day => day.tasks);
  assert.equal(TASK_IDS.size, tasks.length);
  assert.equal(new Date(PLAN[0].date + 'T00:00:00Z').getUTCDay(), 1);
  for (let i = 1; i < PLAN.length; i++) assert.equal(dayDifference(PLAN[i - 1].date, PLAN[i].date), 1);
});

test('the page is self-contained and cannot connect to the sensing application', () => {
  assert.match(html, /connect-src 'none'/);
  assert.doesNotMatch(html, /<script[^>]+src=|<link\b|\bfetch\s*\(|WebSocket|XMLHttpRequest|EventSource|localhost:8080/);
  assert.match(html, /timeZone:'Europe\/Berlin'/);
  for (const script of html.matchAll(/<script[^>]*>([\s\S]*?)<\/script>/g)) assert.doesNotThrow(() => new Function(script[1]));
});


test('old browser and imported snapshots receive only the three reviewed day-14 completions', () => {
  const old = { schemaVersion:1, scheduleId:SCHEDULE_ID, checked:blank() };
  old.checked['radar-individual'] = true;
  const restored = restoreChecks(old);
  for (const id of TASK_IDS) assert.equal(restored[id], old.checked[id] || ['data-rules', 'data-accounting', 'peer-pair-test'].includes(id));
  const manual = { ...restored, 'peer-pair-test':false };
  assert.equal(restoreChecks({ ...old, reviewRevision:REVIEW_REVISION, checked:manual })['peer-pair-test'], false);
  assert.throws(() => restoreChecks({ ...old, reviewRevision:99 }));
});

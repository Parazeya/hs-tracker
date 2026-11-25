import assert from 'node:assert/strict';
import test from 'node:test';
import { dominantZone, medianRate, perHour, runComparison, runDifficulty } from '../src/run-comparison.js';

const run = (started_ms, other = {}) => ({
  started_ms, character: 'Hero', secs: 360, difficulty: 2, hell_sub: 3,
  zones: [['Act_01_01', 10], ['Act_02_01', 300]], gold: 100, ...other,
});

test('selects only comparable runs with the same dominant zone and Hell grade', () => {
  const active = run(1, { gold: 280 });
  const history = [
    active,
    run(2, { gold: 120 }),
    run(3, { gold: 100 }),
    run(4, { gold: 180 }),
    run(5, { secs: 299 }),
    run(6, { zones: [['Act_01_02', 20], ['Act_02_01', 10]] }),
    run(7, { hell_sub: 4 }),
    run(8, { hell_sub: undefined }),
    run(9, { difficulty: 3 }),
  ];
  const comparison = runComparison(active, history);
  assert.equal(comparison.eligible, true);
  assert.equal(comparison.longEnough, true);
  assert.deepEqual(comparison.peers.map((r) => r.started_ms), [2, 3, 4]);
  assert.equal(medianRate(comparison.peers, (r) => r.gold), 1200);
  assert.equal(medianRate(comparison.peers, (r) => r.started_ms === 4 ? null : r.gold), null);
  assert.equal(perHour(null, 300), null);
});

test('keeps older or incomplete records visible under explicit unknown filter groups', () => {
  const old = run(1, { zones: undefined, hell_sub: undefined });
  assert.equal(dominantZone(old), '');
  assert.deepEqual(runDifficulty(old), { key: '2:0', difficulty: 2, hellSub: 0, comparable: false });
  assert.deepEqual(runComparison(old, [run(2)]), { peers: [], eligible: false, longEnough: true });
  assert.equal(runDifficulty(run(4, { difficulty: 0, character: null })).key, '');
  assert.equal(runDifficulty(run(5, { difficulty: 0 })).key, '0');
});

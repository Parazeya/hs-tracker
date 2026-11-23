import { readFileSync } from 'node:fs';
import vm from 'node:vm';
import assert from 'node:assert/strict';
import test from 'node:test';
import { mergeSettings } from '../src/settings-merge.js';

const clone = (value) => structuredClone(value);
const tick = () => new Promise((resolve) => setImmediate(resolve));

async function harness(initial = { opacity: 0.7, locked: false, sound: { volume: 0.5, enabled: true } }) {
  let current = clone(initial), listener, pendingTimer, blocked, fail = false;
  const calls = [];
  const invoke = async (name, args) => {
    if (name === 'get_settings') return clone(current);
    assert.equal(name, 'patch_settings');
    calls.push(clone(args));
    if (blocked) await blocked;
    if (fail) throw new Error('disk is read only');
    current = mergeSettings(args.base, args.settings, current);
    listener({ payload: clone(current) });
  };
  const state = (value) => value;
  state.snapshot = clone;
  const context = vm.createContext({
    invoke, listen: (_name, fn) => { listener = fn; }, mergeSettings,
    $state: state, JSON, String,
    setTimeout: (fn) => { pendingTimer = fn; return fn; },
    clearTimeout: (id) => { if (pendingTimer === id) pendingTimer = null; },
  });
  const source = readFileSync(new URL('../src/settings.svelte.js', import.meta.url), 'utf8')
    .replace(/^import .*;\r?\n/gm, '').replace(/export /g, '');
  vm.runInContext(`${source}\nglobalThis.api = { store, startSettings, save, flush };`, context);
  const api = context.api;
  api.startSettings();
  await tick();
  return {
    ...api, calls,
    external(patch) { current = mergeSettings(current, { ...current, ...patch }, current); listener({ payload: clone(current) }); },
    fire() { const fn = pendingTimer; pendingTimer = null; fn?.(); },
    disk: () => clone(current),
    fail(on) { fail = on; },
    block() { let release; blocked = new Promise((resolve) => { release = resolve; }); return () => { blocked = null; release(); }; },
  };
}

test('pending edits survive repeated native broadcasts and nested changes', async () => {
  const h = await harness();
  h.store.settings.opacity = 0.9;
  h.store.settings.sound.volume = 0.8;
  h.save();
  h.external({ locked: true, sound: { volume: 0.5, enabled: false } });
  h.external({ locked: false });
  await h.flush();
  assert.deepEqual(h.disk(), { opacity: 0.9, locked: false, sound: { volume: 0.8, enabled: false } });
});

test('an in-flight save and a newer edit survive native changes and serial flush', async () => {
  const h = await harness();
  const release = h.block();
  h.store.settings.opacity = 0.8; h.save(); h.fire();
  await tick();
  h.external({ locked: true });
  h.store.settings.opacity = 0.9; h.save();
  const flushed = h.flush();
  await tick();
  assert.equal(h.calls.length, 1, 'the second request must wait');
  release();
  await flushed;
  assert.equal(h.calls.length, 2);
  assert.equal(h.disk().opacity, 0.9);
  assert.equal(h.disk().locked, true);
  assert.equal(h.store.settings.opacity, 0.9);
});

test('failed saves propagate through flush, retain edits through broadcasts, and can retry', async () => {
  const h = await harness();
  h.fail(true);
  h.store.settings.opacity = 0.9; h.save();
  await assert.rejects(h.flush(), /read only/);
  assert.match(h.store.error, /read only/);
  h.external({ locked: true });
  assert.equal(h.store.settings.opacity, 0.9);
  h.fail(false); h.save(); await h.flush();
  assert.equal(h.disk().opacity, 0.9);
  assert.equal(h.disk().locked, true);
  assert.equal(h.store.error, '');
});

test('arrays are explicit edits and untouched current fields survive', () => {
  assert.deepEqual(mergeSettings({ filters: [1], locked: false }, { filters: [1, 2], locked: false }, { filters: [1], locked: true, added: 3 }), { filters: [1, 2], locked: true, added: 3 });
});

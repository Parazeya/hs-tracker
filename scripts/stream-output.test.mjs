import assert from 'node:assert/strict';
import test from 'node:test';
import { streamClient } from '../src/stream-client.js';
import { currentEntries, reading } from '../src/stream-model.js';
import { mapEnvelope, mirrorBox, sourceUrl, validMapState } from '../src/stream-protocol.js';

const map = () => ({ camera: { center: [.2, .7], scale: 1.4 }, viewport: { width: 2560, height: 1388 },
  selection: { roomId: 'Act_01_01' }, presentation: { query: 'Musket', lang: 'ru', source: null } });

test('a map mirror preserves the source document proportions and exact camera scale', () => {
  const state = map();
  assert.equal(validMapState(state), true);
  assert.deepEqual(mirrorBox(state.viewport, { width: 1280, height: 720 }), { width: 2560, height: 1388, scale: .5, left: 0, top: 13 });
  assert.deepEqual(state.camera, { center: [.2, .7], scale: 1.4 });
  const url = new URL(sourceUrl('http://127.0.0.1:17265', 'mirror', 'http://127.0.0.1:17265'));
  assert.equal(url.pathname, '/hs-map/index.html');
  assert.equal(url.searchParams.get('parentOrigin'), 'http://127.0.0.1:17265');
});

test('map state rejects commands, extra fields, nonfinite positions and unbounded presentation', () => {
  for (const change of [state => state.command = 'quit', state => state.camera.center[0] = Infinity,
    state => state.viewport.height = 0, state => state.presentation.query = 'x'.repeat(257),
    state => state.presentation.lang = 'javascript:alert(1)', state => state.selection.roomId = '\n']) {
    const state = map(); change(state); assert.equal(validMapState(state), false);
  }
  assert.equal(mapEnvelope({ channel: 'other', version: 1, type: 'apply', revision: 1 }, 'apply'), false);
  assert.equal(mapEnvelope({ channel: 'hs-map', version: 1, type: 'apply', revision: 1.5 }, 'apply'), false);
});

test('reconnect bootstrap delivers only new per-topic state and survives a Tracker restart', () => {
  let source;
  class FakeSource {
    constructor() { source = this; }
    addEventListener(name, handler) { this[name] = handler; }
    close() { this.closed = true; }
    send(update) { this.update({ data: JSON.stringify(update) }); }
  }
  const frames = [], statuses = [], epochs = [], timers = [];
  const stop = streamClient({ EventSourceImpl: FakeSource, onFrame: frame => frames.push(frame), onStatus: status => statuses.push(status), onEpoch: epoch => epochs.push(epoch),
    setTimer: handler => { timers.push(handler); return handler; }, clearTimer: () => {} });
  const update = (epoch, topic, id, payload) => ({ epoch, server_ms: 10000, frames: [{ topic, id, payload }] });
  source.onopen(); source.send(update('boot1', 'snapshot', 100, { gold: 1 }));
  source.onerror(); timers.shift()(); source.onopen(); source.send(update('boot1', 'snapshot', 100, { gold: 1 }));
  source.send(update('boot1', 'map', 99, map())); // topics have separate revisions
  source.send(update('boot1', 'snapshot', 101, { gold: 2 }));
  source.send(update('boot2', 'snapshot', 1, { gold: 0 }));
  source.send(update('boot2', 'native-command', 2, { command: 'quit' }));
  assert.deepEqual(frames.map(frame => [frame.topic, frame.id]), [['snapshot', 100], ['map', 99], ['snapshot', 101], ['snapshot', 1]]);
  assert.deepEqual(statuses, ['connected', 'reconnecting', 'connected']);
  assert.deepEqual(epochs, ['boot1', 'boot2'], 'reconnecting to the same process must retain its map, but a new process reloads packaged data');
  stop(); assert.equal(source.closed, true);
});

test('fatal disabled responses recreate one source with bounded retry and clean up on unmount', () => {
  const sources = [], timers = new Map(), delays = [];
  class DisabledSource {
    constructor() { sources.push(this); }
    addEventListener() {}
    close() { this.closed = true; }
  }
  const stop = streamClient({ EventSourceImpl: DisabledSource, onFrame: () => {}, onStatus: () => {},
    setTimer: (handler, delay) => { const key = {}; timers.set(key, handler); delays.push(delay); return key; },
    clearTimer: key => timers.delete(key) });
  for (let retry = 0; retry < 6; retry++) {
    const current = sources.at(-1); current.onerror(); current.onerror();
    assert.equal(current.closed, true); assert.equal(timers.size, 1);
    const [key, handler] = [...timers][0]; timers.delete(key); handler();
  }
  assert.deepEqual(delays, [1000, 2000, 4000, 5000, 5000, 5000]);
  sources.at(-1).onopen(); sources.at(-1).onerror();
  assert.equal(delays.at(-1), 1000, 'a successful enable resets recovery delay');
  stop(); assert.equal(timers.size, 0);
  assert.equal(sources.filter(source => !source.closed).length, 0);
});

test('expired notifications cannot replay when a source reconnects', () => {
  const entries = [{ stream_id: 1, until_ms: 9900 }, { stream_id: 2, start_ms: 10000, until_ms: 16000 },
    { stream_id: 3, start_ms: 16000, until_ms: 22000 }];
  assert.deepEqual(currentEntries(entries, 17000, 90000).map(entry => entry.stream_id), [3]);
  assert.equal(currentEntries(entries, 23000, 90000).length, 0);
});

test('OBS reads authoritative corrected counters rather than accumulating client-side credit', () => {
  const before = { xp: { earned: 9000, per_hour: 18000 } };
  const corrected = { xp: { earned: 1200, per_hour: 2400 } };
  assert.equal(reading('xp', before, 0)[1], '9,000');
  assert.equal(reading('xp', corrected, 0)[1], '1,200');
  assert.equal(reading('xph', corrected, 0)[1], '2,400');
});

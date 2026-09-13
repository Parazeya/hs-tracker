// The one copy of the settings that every tab of the dashboard edits.
//
// Each tab used to hold a copy of its own, load it on open and write it back on
// a timer, and each got the same two things wrong in its own way. A save took
// its snapshot when the edit was made and wrote it 150ms later, over the top of
// whatever the tray, a hotkey or the overlay's strip had changed in between.
// And a change arriving from outside replaced the whole copy, taking any edit
// not yet written with it. It was reported, reproduced as a lock toggle that
// came back on, and fixed on one tab — the other three kept it.
//
// So there is one copy, here, and one save, and every tab reads and writes the
// same object. Splitting a tab in two costs nothing now: there is no plumbing
// left in a tab to copy.
import { invoke, listen } from './bridge.js';

/// `settings` is null until the backend has answered. `error` is the last save
/// that failed, said out loud: installed where the folder will not take a
/// write, every save fails, and a page of controls that silently do nothing is
/// the worst way to find that out.
///
/// `changed` counts every settings broadcast from the backend, the echo of this
/// window's own saves included. A tab that keeps a little state of its own
/// derived from the settings — whether a picker is folded open — watches it to
/// know that the settings under that state have just been replaced.
export const store = $state({ settings: null, error: '', changed: 0 });

/// The settings as the backend last handed them over. Anything that now
/// differs from this is an edit made here and not yet written.
let base = null;
let timer = null;
let started = false;

const copy = (value) => JSON.parse(JSON.stringify(value));
const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);

/// Load once, and follow every change from then on. Safe to call from every
/// tab that mounts; only the first call does anything.
export function startSettings() {
  if (started) return;
  started = true;
  invoke('get_settings')
    .then((s) => {
      store.settings = s;
      base = copy(s);
    })
    .catch(() => {});
  listen('settings-changed', (e) => {
    store.changed += 1;
    // Nothing waiting to be written: take it whole.
    if (!timer || !store.settings || !base) {
      store.settings = e.payload;
      base = copy(e.payload);
      return;
    }
    // An edit here is waiting. Take every field except the ones edited here —
    // "edited here" being whatever differs from what the backend last sent.
    // That needs nothing from the controls themselves, which is the point:
    // there are well over a hundred of them, and any one that forgot to say
    // it had been touched would be the bug back again.
    for (const [key, value] of Object.entries(e.payload)) {
      if (same(store.settings[key], base[key])) store.settings[key] = value;
    }
  });
}

/// Write the settings, a moment after the last edit.
///
/// The snapshot is taken when the timer fires and not when the edit was made:
/// the listener above spends these 150ms merging changes from elsewhere into
/// the settings, and a snapshot taken before the wait would be written over
/// every one of them.
export function save() {
  clearTimeout(timer);
  timer = setTimeout(() => {
    timer = null;
    if (!store.settings) return;
    const snapshot = $state.snapshot(store.settings);
    base = copy(snapshot);
    store.error = '';
    invoke('save_settings', { settings: snapshot }).catch((e) => {
      store.error = String(e);
    });
  }, 150);
}

/// Write a pending edit now rather than in a moment. For whatever is about to
/// end this process — a restart of the backend — where an edit still on the
/// timer would die with it.
export async function flush() {
  if (!timer || !store.settings) return;
  clearTimeout(timer);
  timer = null;
  const snapshot = $state.snapshot(store.settings);
  base = copy(snapshot);
  await invoke('save_settings', { settings: snapshot }).catch((e) => {
    store.error = String(e);
  });
}

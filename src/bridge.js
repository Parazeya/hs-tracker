// One way in for everything the windows ask of the backend.
//
// The same components are drawn twice: in the app's own windows, where Tauri is
// there and every command works, and in a browser — OBS's Browser Source — where
// none of it exists. Rather than two versions of each panel, both talk to this,
// and it answers from whichever side it is on.
//
// The page on a stream reads and never commands: it shows the run, it does not
// reset it. Anything that would change something answers with nothing.

import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { listen as tauriListen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';

/// Tauri puts this on the window before any of our code runs; a browser has no
/// such thing, and that is the whole of the difference.
export const native = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

/// Which face the page is showing when it is not in a window of its own —
/// `?view=overlay` or `?view=dashboard`, as the addresses in Settings say.
export const view = native
  ? null
  : new URLSearchParams(location.search).get('view') || 'overlay';

const READS = {
  snapshot: '/api/snapshot',
  get_settings: '/api/settings',
  get_runs: '/api/runs',
};

export async function invoke(command, args) {
  if (native) return tauriInvoke(command, args);
  const path = READS[command];
  if (!path) return null;
  const answer = await fetch(path, { cache: 'no-store' });
  if (!answer.ok) throw new Error(`${command}: ${answer.status}`);
  return answer.json();
}

const listeners = new Map();
let events = null;

function open() {
  if (events) return;
  events = new EventSource('/api/events');
  // `flourish-play` is the same event the app's own window answers to, so the
  // announcement panel needs no idea which side it is drawn on
  for (const name of ['stats', 'flourish', 'drop']) {
    events.addEventListener(name, (e) => {
      let payload = null;
      try {
        payload = JSON.parse(e.data);
      } catch {
        return;
      }
      const to = { flourish: 'flourish-play', drop: 'drop-entry' }[name] ?? name;
      for (const fn of listeners.get(to) ?? []) fn({ payload });
    });
  }
  // EventSource reconnects on its own; nothing here has to
}

export async function listen(name, handler) {
  if (native) return tauriListen(name, handler);
  open();
  const list = listeners.get(name) ?? [];
  list.push(handler);
  listeners.set(name, list);
  return () => {
    const now = (listeners.get(name) ?? []).filter((f) => f !== handler);
    listeners.set(name, now);
  };
}

/// The window itself — minimise, drag, resize. A page in a browser has no window
/// to speak of, so it gets one that politely does nothing.
const NOTHING = {
  minimize() {},
  hide() {},
  setFocus() {},
  startDragging() {},
  startResizeDragging() {},
  label: view ?? 'browser',
};

export function appWindow() {
  // the import is harmless anywhere; it is the call that needs Tauri under it
  return native ? getCurrentWindow() : NOTHING;
}

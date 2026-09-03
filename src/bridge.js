// One way in for everything the windows ask of the backend.
//
// One implementation, not two. There were two sides to it: the app's own
// windows, where Tauri is present and every command works, and a page served to
// OBS as a Browser Source,
// where none of it exists. The served page is gone — it went with the little
// HTTP server that fed it — so what is left is the one door, and the guards
// which is why this no longer chooses between two.
//
// The guards stay. `native` is what tells a Tauri window from anything else,
// and the panels are drawn by a webview either way: a component that calls a
// command while it is being rendered somewhere without one should get nothing
// back, not an exception in a transparent window nobody can see fail.

import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { listen as tauriListen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';

/// Tauri puts this on the window before any of our code runs.
export const native = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

export async function invoke(command, args) {
  if (!native) return null;
  return tauriInvoke(command, args);
}

export async function listen(name, handler) {
  if (!native) return () => {};
  return tauriListen(name, handler);
}

/// What the windows remember between sessions, and what to do when they cannot.
///
/// `localStorage` is not guaranteed to work. A WebView2 profile that is damaged,
/// read-only or out of quota throws on the first touch of it, and this app read
/// it at module scope, before the interface was mounted: the throw took the
/// whole module with it. Every window here is transparent and has no frame, so
/// what was left on screen was an invisible rectangle with no close button and
/// no drag region — a window that answers no click and can only be ended in the
/// task manager. A remembered tab is not worth that.
export function recall(key) {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

export function remember(key, value) {
  try {
    localStorage.setItem(key, value);
  } catch {
    // A preference that cannot be written is a preference that lasts one
    // session. The window stays up, which is the part that matters.
  }
}

/// The window itself — minimise, drag, resize. Without Tauri under it there is
/// no window to speak of, so it gets one that politely does nothing.
const NOTHING = {
  minimize() {},
  hide() {},
  setFocus() {},
  startDragging() {},
  startResizeDragging() {},
  label: 'none',
};

export function appWindow() {
  // the import is harmless anywhere; it is the call that needs Tauri under it
  return native ? getCurrentWindow() : NOTHING;
}

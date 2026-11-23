// The panels also render in a regular browser during development. Guard native
// commands and events so the browser preview works without Tauri.

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
  toggleMaximize() {},
  isMaximized() {
    return Promise.resolve(false);
  },
  onResized() {
    return Promise.resolve(() => {});
  },
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

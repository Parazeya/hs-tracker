import { mount } from 'svelte';
import './theme.css';
import { wearSkin } from './skin.svelte.js';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import App from './App.svelte';
import Dashboard from './Dashboard.svelte';
import Ticker from './Ticker.svelte';
import Flourish from './Flourish.svelte';

// no default WebView2 context menu anywhere; the overlay draws its own
window.addEventListener('contextmenu', (e) => e.preventDefault());

// Which desktop this is, for the handful of rules that have to differ.
// WebKitGTK on a transparent X11 window composites each frame over the last
// instead of clearing, so anything drawn on transparency there smears; opaque
// paint replaces the pixel underneath and is the only thing that does not. The
// rules that pay for that are marked [data-os='linux'] and cost Windows
// nothing.
document.documentElement.dataset.os = /Linux|X11/.test(navigator.userAgent) ? 'linux' : 'other';

// A panel that throws while rendering goes blank and says nothing — which has
// already cost an evening once. Everything the web side throws is written to
// the app's log instead of the console nobody can see in a released build.
const told = new Set();
function tell(what) {
  // the same error can fire on every frame; one line per kind is plenty
  if (told.has(what) || told.size > 40) return;
  told.add(what);
  invoke('report', { level: 'error', message: what }).catch(() => {});
}
window.addEventListener('error', (e) => {
  const where = e.filename ? ` (${e.filename}:${e.lineno}:${e.colno})` : '';
  tell(`${e.message}${where}
${e.error?.stack ?? ''}`.trim());
});
window.addEventListener('unhandledrejection', (e) => {
  const reason = e.reason;
  tell(`unhandled rejection: ${reason?.stack ?? reason?.message ?? String(reason)}`);
});

// The skin is chosen once, before anything is drawn, so no window ever flashes
// in the wrong colours. Every window follows the same setting, and a change in
// Settings reaches the others through the event the backend already emits.
import { invoke, listen, native, view } from './bridge.js';

const wearTheme = (name) => {
  const root = document.documentElement;
  if (name && name !== 'default') root.setAttribute('data-theme', name);
  else root.removeAttribute('data-theme');
  localStorage.setItem('theme', name ?? 'default');
  // the sprites follow the palette; both halves of a skin move together
  wearSkin(name);
};
// The settings live in the backend, and asking for them is a round trip — long
// enough to draw one frame in the wrong colours. The last answer is kept here
// and worn immediately; the real one arrives a moment later and corrects it.
wearTheme(localStorage.getItem('theme'));
invoke('get_settings')
  .then((s) => wearTheme(s?.theme))
  .catch(() => {});
listen('settings-changed', (e) => wearTheme(e.payload?.theme));

// In one of the app's own windows the label says which face to draw. Served to
// a browser — OBS's Browser Source — there is no window to ask, so the address
// says instead: /?view=overlay or /?view=dashboard.
const label = native ? getCurrentWebviewWindow().label : view;
const roots = { dashboard: Dashboard, ticker: Ticker, flourish: Flourish };

const app = mount(roots[label] ?? App, {
  target: document.getElementById('app'),
});

// Tell the backend a page really did paint. Every window here is transparent,
// so a renderer that dies leaves an *invisible* window rather than a blank one
// and nothing else can tell the difference. Sent after a frame, not on mount:
// mounting only means the script ran.
if (native) {
  requestAnimationFrame(() => requestAnimationFrame(() => invoke('ui_ready').catch(() => {})));
}

export default app;

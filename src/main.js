import { mount } from 'svelte';
import './theme.css';
import './plain.css';
import { wearSkin } from './skin.svelte.js';
import { speak } from './say.svelte.js';
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
import { invoke, listen, native, recall, remember } from './bridge.js';

const wearTheme = (name) => {
  const root = document.documentElement;
  if (name && name !== 'default') root.setAttribute('data-theme', name);
  else root.removeAttribute('data-theme');
  // The palette rides on data-theme; the SHAPE rides on this. Only the plain
  // skin has one, because only it replaces the art with rules — a season is
  // the same nine-slice PNGs in other colours and needs nothing here.
  // The palette rides on data-theme and there are two plain palettes; the SHAPE
  // is one and the same, so both wear the same skin name.
  if (name === 'plain' || name === 'plainlight') root.setAttribute('data-skin', 'plain');
  else root.removeAttribute('data-skin');
  remember('theme', name ?? 'default');
  // the sprites follow the palette; both halves of a skin move together
  wearSkin(name);
};
// The settings live in the backend, and asking for them is a round trip — long
// enough to draw one frame in the wrong colours. The last answer is kept here
// and worn immediately; the real one arrives a moment later and corrects it.
wearTheme(recall('theme'));
// The names the game gives things follow the same path as the skin: remembered
// so the first paint is right, corrected when the backend answers, and moved
// when the setting changes in another window.
speak(recall('language') ?? 'en');
const wearLanguage = (code) => {
  remember('language', code ?? 'en');
  speak(code ?? 'en');
};
invoke('get_settings')
  .then((s) => {
    wearTheme(s?.theme);
    wearLanguage(s?.language);
  })
  .catch(() => {});
listen('settings-changed', (e) => {
  wearTheme(e.payload?.theme);
  wearLanguage(e.payload?.language);
});

// The window's own label says which face to draw. There is one other way in —
// a webview built without Tauri under it, which is what a test harness gets —
// and it draws the overlay, the same as an unlabelled window would.
const label = native ? getCurrentWebviewWindow().label : 'main';
const roots = { dashboard: Dashboard, ticker: Ticker, flourish: Flourish };

// Nothing drew, and every window here is transparent.
//
// A page that throws on its way up leaves an invisible rectangle: no panel, no
// close button, no drag region, nothing to tell it from a working window over
// something dark. Clicks land on it and nothing answers, so the only way out is
// the task manager.
//
// The backend cannot help — it sees a window it built and showed, and
// `ui_ready` never arriving reaches only the log — so an interface that cannot
// start says so in something opaque and puts a close button on it.
function lastResort(err) {
  const said = `${err?.stack || err?.message || err}`;
  try {
    invoke('report', { level: 'error', message: `the interface did not start: ${said}` });
  } catch {
    // the bridge itself is what failed; the panel below is all that is left
  }
  const root = document.getElementById('app') ?? document.body;
  root.innerHTML = '';
  const panel = document.createElement('div');
  panel.style.cssText =
    'font:13px/1.5 system-ui,sans-serif;box-sizing:border-box;height:100vh;padding:16px;' +
    'display:flex;flex-direction:column;gap:10px;background:#151016;color:#e8dfd4';
  const head = document.createElement('b');
  head.textContent = 'HS Tracker could not start its interface.';
  const why = document.createElement('pre');
  why.style.cssText = 'flex:1;margin:0;overflow:auto;white-space:pre-wrap;font-size:11px;opacity:.75';
  why.textContent = said;
  const shut = document.createElement('button');
  shut.textContent = 'Close';
  shut.style.cssText = 'align-self:flex-start;padding:6px 14px;cursor:pointer';
  // `destroy` answers with a promise, so a refusal arrives after this function
  // has returned and the `try` around it never sees one. The only way out of a
  // window with nothing drawn in it was a button that did nothing at all.
  shut.onclick = () => {
    let asked;
    try {
      asked = getCurrentWebviewWindow().destroy();
    } catch {
      window.close();
      return;
    }
    Promise.resolve(asked).catch(() => window.close());
  };
  panel.append(head, why, shut);
  root.append(panel);
  document.documentElement.style.background = '#151016';
}

let app;
try {
  app = mount(roots[label] ?? App, {
    target: document.getElementById('app'),
  });
} catch (e) {
  lastResort(e);
}

// Tell the backend a page really did paint. Every window here is transparent,
// so a renderer that dies leaves an *invisible* window rather than a blank one
// and nothing else can tell the difference. Sent after a frame, not on mount:
// mounting only means the script ran.
if (native) {
  requestAnimationFrame(() => requestAnimationFrame(() => invoke('ui_ready').catch(() => {})));
}

export default app;

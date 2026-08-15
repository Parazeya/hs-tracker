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

export default mount(roots[label] ?? App, {
  target: document.getElementById('app'),
});

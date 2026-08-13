import { mount } from 'svelte';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import App from './App.svelte';
import Dashboard from './Dashboard.svelte';
import Ticker from './Ticker.svelte';

// no default WebView2 context menu anywhere; the overlay draws its own
window.addEventListener('contextmenu', (e) => e.preventDefault());

const label = getCurrentWebviewWindow().label;
const roots = { dashboard: Dashboard, ticker: Ticker };

export default mount(roots[label] ?? App, {
  target: document.getElementById('app'),
});

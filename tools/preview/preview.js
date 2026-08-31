// Every window the app has, in a browser, without the app.
//
// A skin is CSS, and checking one used to mean building the Rust side, starting
// the app, and clicking through eight tabs on a live screen — for a change to a
// border radius. Worse, half the panels only draw when something has dropped:
// the loot pillar and the drop ticker are empty until the game says otherwise,
// so the two most visual things in the app were the two hardest to look at.
//
// This mocks the Tauri IPC with canned answers, plays the backend for the one
// event a panel is waiting on, and mounts whichever window is asked for.
//
//   npx vite
//   http://localhost:5176/tools/preview/?theme=modern
//   ...&panel=runs | filter | watchlist | codex | shop | settings | about
//   ...&panel=flourish | zone | ticker
//
// with no `panel`, the dashboard and the overlay side by side. `theme` takes
// any skin name — `default` and `modern` are what it is usually pointed at,
// one after the other.
//
// The data below is invented and only has to be shaped right; where a figure
// looks wrong in here, it is this file that is wrong and not the panel.

import { mockIPC, mockWindows } from '@tauri-apps/api/mocks';

const theme = new URLSearchParams(location.search).get('theme') ?? 'modern';

// A panel that throws renders nothing at all, and a headless screenshot of
// nothing looks exactly like a skin that painted everything black.
addEventListener('error', (e) => {
  const p = document.createElement('pre');
  p.style.cssText = 'color:#f87171;font:12px monospace;white-space:pre-wrap;padding:12px';
  p.textContent = `${e.message}\n${e.error?.stack ?? ''}`;
  document.body.prepend(p);
});

const SETTINGS = {
  theme,
  locked: false,
  opacity: 1,
  scale: 1,
  auto_show: true,
  autostart: false,
  ticker: true,
  debug_log: false,
  wide_capture: false,
  sound_on_ground: true,
  discord: false,
  compact: false,
  ghost: false,
  flourish: true,
  flourish_scale: 1,
  flourish_shade: 0.55,
  flourish_secs: 600,  // held up long enough for a still to catch it
  flourish_rarities: ['Satanic', 'Set', 'Heroic', 'Angelic', 'Unholy'],
  flourish_tier: 1,
  flourish_listed: false,
  flourish_zone: true,
  flourish_always: false,
  alerts: ['Satanic', 'Set', 'Heroic', 'Angelic', 'Unholy'],
  min_tier: 0,
  notable: [{ label: 'Angelic Key', names: ['Angelic Key'] }],
  filters: [{ id: 'f1', name: 'Filter 1', lists: [
    { id: 'l1', name: 'Chase', enabled: true, volume: 0.7, items: ['AK-47', 'Glacier Talons'], rules: [] },
    { id: 'l2', name: 'Runes', enabled: false, volume: 0.5, items: ['S rune'], rules: [{ rarity: 'Satanic', tier: 5 }] },
  ] }],
  filter: 'f1',
  use_filter: true,
  lists: [],
  relics: [],
  relic: { enabled: false, volume: 0.7 },
  zone_buffs: [],
  zone: { enabled: true, volume: 0.7 },
  mail: { enabled: true, volume: 0.7 },
  satanic: { enabled: true, volume: 0.7 },
  set: { enabled: true, volume: 0.7 },
  heroic: { enabled: true, volume: 0.7 },
  angelic: { enabled: true, volume: 0.7 },
  unholy: { enabled: true, volume: 0.7 },
  hidden: [],
  x11_backend: false,
};

const RARITIES = ['Satanic', 'Set', 'Heroic', 'Angelic', 'Unholy'];
const items = Object.fromEntries(
  RARITIES.map((r, i) => [r, { total: 12 - i * 2, mf: 3, per_hour: 20 - i * 3 }]),
);

const SNAPSHOT = {
  status: 'capturing|Ethernet|3|0|182004|0',
  session_secs: 4085,
  paused: false,
  gold: 3987,
  gold_per_hour: 20900,
  bank: 788800,
  xp: 5640000,
  xp_per_hour: 29590,
  kills: 1117,
  kills_per_hour: 5861,
  level: 100,
  hero_level: 112,
  char_name: 'Parahryushka',
  difficulty: 'Inferno IV',
  act: 7,
  magic_find: 1456,
  items,
  has_mail: true,
  save_age_secs: 540,
  bank_age_secs: 540,
  satanic_here: true,
  satanic_zone: { zone: 'Act_07_Subconscious_Mind_rm', buffs: [24, 3, 4], debuffs: [1, 2], resets_in: 806, at: '18:00' },
  tallies: [
    { group: 'chest', name: 'Common', total: 1 },
    { group: 'boss', name: 'Damien', total: 4 },
  ],
  drops: Array.from({ length: 9 }, (_, i) => ({
    at: `17:3${i}:09`,
    rarity: RARITIES[i % 5],
    name: ['Giant Thorned Lizard', 'Anubis Oculus', 'AK-47', "Warbosses' Trophy", "Dark Lord's Sigil"][i % 5],
    tier: 6 - (i % 4),
    mf: i % 2 === 0,
    zone: 'Act 7 · Zone 5',
  })),
  rates: Array.from({ length: 24 }, (_, i) => ({ gold: 90000 + i * 6000, xp: 120000 + i * 9000 })),
};

const RUNS = Array.from({ length: 6 }, (_, i) => ({
  id: `r${i}`,
  at: Date.now() - i * 3600_000,
  secs: 2400 + i * 300,
  gold: 300000 + i * 40000,
  xp: 9000000 + i * 500000,
  kills: 3400 + i * 120,
  items,
  tallies: SNAPSHOT.tallies,
  finds: [{ name: 'Glacier Talons', rarity: 'Satanic', tier: 6 }],
}));

const ANSWERS = {
  get_settings: SETTINGS,
  snapshot: SNAPSHOT,
  get_extra: { timeline: SNAPSHOT.drops, rates: SNAPSHOT.rates, tallies: SNAPSHOT.tallies },
  get_runs: RUNS,
  get_shopping: ['Angelic Key', 'Satanic Dice'],
  session_info: { overlay: true, smears: false, wayland: false },
  about: { version: '1.0.5', binary: 'C:/Program Files/HS Tracker/hs-tracker.exe', appimage: false, npcap: '1.79' },
  sound_status: {},
  sound_path: '',
  log_path: 'C:/Program Files/HS Tracker/hs-tracker.log',
};

mockWindows('dashboard', 'main', 'ticker', 'flourish');

// `listen` is itself a command, so the mock sees every subscription go past and
// can keep the callback id. `emit` then plays the backend for one event, which
// is the only way to see the loot pillar or the drop ticker: both are empty
// until something drops.
const listeners = new Map();
mockIPC((cmd, args) => {
  if (cmd === 'plugin:event|listen') {
    listeners.set(args.event, args.handler);
    return 1;
  }
  return ANSWERS[cmd] ?? null;
});
const emit = (event, payload) => {
  const id = listeners.get(event);
  if (id != null) window.__TAURI_INTERNALS__.runCallback(id, { event, id: 1, payload });
};

const { mount } = await import('svelte');
await import('../../src/theme.css');
await import('../../src/modern.css');
const { wearSkin } = await import('../../src/skin.svelte.js');

if (theme !== 'default') document.documentElement.setAttribute('data-theme', theme);
document.documentElement.dataset.os = 'other';
wearSkin(theme);

const PANELS = {
  runs: '../../src/Runs.svelte',
  filter: '../../src/SoundFilter.svelte',
  watchlist: '../../src/Watchlist.svelte',
  codex: '../../src/Codex.svelte',
  shop: '../../src/Shop.svelte',
  settings: '../../src/Settings.svelte',
  about: '../../src/About.svelte',
};

const root = document.getElementById('app');
root.style.cssText = 'display:flex;gap:24px;align-items:flex-start;padding:20px;background:#0a0a0c';

const box = (w, h) => {
  const d = document.createElement('div');
  d.style.cssText = `width:${w}px;${h ? `height:${h}px;` : ''}flex:none`;
  root.append(d);
  return d;
};

const which = new URLSearchParams(location.search).get('panel');

if (which === 'flourish' || which === 'zone') {
  const { default: Flourish } = await import('../../src/Flourish.svelte');
  const d = box(900, 460);
  // a ground to judge it against: it is drawn over the game, not over black
  d.style.background = 'linear-gradient(140deg,#243a2e,#3a2a24 60%,#1c2430)';
  mount(Flourish, { target: d });
  const drop =
    which === 'zone'
      ? { zone: 'Act_07_Subconscious_Mind_rm', buffs: [24, 3, 4], debuffs: [1, 2], kind: 'zone' }
      : { rarity: 'Satanic', name: 'The Absence of Constraint', tier: 6 };
  setTimeout(() => emit('flourish-play', drop), 60);
} else if (which === 'ticker') {
  const { default: Ticker } = await import('../../src/Ticker.svelte');
  mount(Ticker, { target: box(444) });
  setTimeout(() => {
    SNAPSHOT.drops.slice(0, 4).forEach((d, i) =>
      emit('drop-entry', { ...d, rarity: d.rarity, item_type: 0, item_id: 0 }),
    );
  }, 60);
} else if (PANELS[which]) {
  // A section draws itself inside the dashboard's pane, so it is given one:
  // the chrome around it is what half of these rules are about.
  const { default: Panel } = await import(/* @vite-ignore */ PANELS[which]);
  const shell = box(900, 660);
  shell.className = 'panel window';
  shell.style.boxSizing = 'border-box';
  const pane = document.createElement('div');
  pane.className = 'pane';
  pane.style.cssText = 'height:100%;box-sizing:border-box';
  shell.append(pane);
  mount(Panel, { target: pane });
} else {
  const { default: App } = await import('../../src/App.svelte');
  const { default: Dashboard } = await import('../../src/Dashboard.svelte');
  mount(Dashboard, { target: box(880, 660) });
  mount(App, { target: box(444) });
}

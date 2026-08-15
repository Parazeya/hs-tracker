<script>
  import { invoke } from './bridge.js';
  import { art } from './skin.svelte.js';
  import { listen } from './bridge.js';
  import { buffInfo, defaultBuffIcon, zoneName, icon } from './buffs.js';
  import { RARITIES, soundUrl, play } from './audio.js';

  let snap = $state(null);

  let cfg = $state(null);
  let locked = $derived(cfg?.locked ?? false);
  let drag = $derived(cfg?.locked ? null : '');
  const urls = {};
  const lastPlayed = {};

  async function initSounds() {
    cfg = await invoke('get_settings').catch(() => null);
    for (const rarity of RARITIES) urls[rarity] = await soundUrl(rarity);
  }

  // a list brings its own sound and its own volume; everything else is one of
  // the six built-in alerts. Lists live inside the active filter — the loose
  // `lists` field is pre-0.9.4 and is emptied by the migration on load.
  function channel(key) {
    if (!key.startsWith('list-')) return cfg?.[key];
    const active = (cfg?.filters ?? []).find((f) => f.id === cfg?.filter);
    return (active?.lists ?? []).find((l) => `list-${l.id}` === key);
  }

  async function playSound(key, rarity) {
    const c = channel(key);
    if (c && c.enabled === false) return;
    const now = Date.now();
    if (now - (lastPlayed[key] ?? 0) < 200) return;
    lastPlayed[key] = now;
    urls[key] ??= await soundUrl(key);
    // a list without a sound of its own borrows the one for its rarity
    const url = urls[key] ?? urls[String(rarity ?? '').toLowerCase()];
    play(url, c?.volume ?? 0.7);
  }

  // the backend pushes a snapshot when something changes; the clock is kept
  // running locally so the seconds never stutter between two pushes
  let clock = $state({ secs: 0, at: Date.now() });
  let tick = $state(Date.now());
  let sessionSecs = $derived(
    clock.secs + (snap?.paused ? 0 : Math.floor((tick - clock.at) / 1000)),
  );

  function received(s) {
    snap = s;
    clock = { secs: s.session_secs, at: Date.now() };
  }

  $effect(() => {
    initSounds();
    invoke('snapshot').then(received).catch(() => {});
    const unsubs = [
      listen('stats', (e) => received(e.payload)),
      // this window is the one that plays sounds, hidden or not — the backend
      // says when mail arrives rather than leaving it to be spotted in a
      // snapshot that only travels to windows on screen
      listen('mail', () => playSound('mail')),
      listen('item-drop', (e) => playSound(...(Array.isArray(e.payload) ? e.payload : [e.payload]))),
      listen('settings-changed', (e) => (cfg = e.payload)),
      listen('sounds-changed', async (e) => (urls[e.payload] = await soundUrl(e.payload))),
    ];
    const timer = setInterval(() => (tick = Date.now()), 1000);
    return () => {
      clearInterval(timer);
      unsubs.forEach((u) => u.then((f) => f()));
    };
  });

  // 1 234 567 -> 1.23kk; keeps the chips readable at any scale
  function fmt(n) {
    const v = n ?? 0;
    const abs = Math.abs(v);
    if (abs >= 1e9) return `${(v / 1e9).toFixed(2)}kkk`;
    if (abs >= 1e6) return `${(v / 1e6).toFixed(2)}kk`;
    if (abs >= 10_000) return `${(v / 1e3).toFixed(1)}k`;
    return v.toLocaleString('en-US');
  }

  function dur(secs) {
    const h = Math.floor(secs / 3600);
    const m = Math.floor((secs % 3600) / 60);
    const s = secs % 60;
    return `${h}:${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`;
  }

  const item = (name) => snap?.items?.[name] ?? { total: 0, mf: 0, per_hour: 0 };

  let buffs = $derived(
    Array.from({ length: 3 }, (_, i) => {
      const id = snap?.satanic_zone?.buffs?.[i];
      return id == null ? null : buffInfo(id);
    })
  );

  // The window is only ever as tall as the panel inside it. Measuring here and
  // telling the backend keeps the two in step whatever rows are switched on —
  // and means adding a row to this file needs nothing done anywhere else.
  let panelEl;
  $effect(() => {
    if (!panelEl) return;
    const report = () => {
      const height = panelEl.getBoundingClientRect().height;
      if (height > 0) invoke('fit_overlay', { height }).catch(() => {});
    };
    const observer = new ResizeObserver(report);
    observer.observe(panelEl);
    report();
    return () => observer.disconnect();
  });

  let status = $derived.by(() => {
    const s = snap?.status ?? '';
    if (s.startsWith('capturing')) {
      const [, iface, hosts, dropped] = s.split('|');
      const loss = Number(dropped) > 0 ? `, ${dropped} packets dropped` : '';
      return { cls: Number(dropped) > 0 ? 'warn' : 'ok', tip: `Capturing: ${iface} (${hosts} hosts${loss})` };
    }
    if (s === 'waiting-for-game') return { cls: 'warn', tip: 'Waiting for Hero Siege to start' };
    if (s === 'npcap-missing') return { cls: 'err', tip: 'Npcap is not installed — https://npcap.com' };
    // elsewhere libpcap is always there; what is missing is the right to use it
    if (s === 'no-capture') return { cls: 'err', tip: 'No capture device — the binary needs cap_net_raw' };
    return { cls: 'err', tip: 'No suitable network interface' };
  });

  const shown = (id) => !(cfg?.hidden ?? []).includes(id);

  // pinned over a running game: drop the frame and the button, leave the
  // numbers floating on top of the game
  let live = $derived((snap?.status ?? '').startsWith('capturing'));
  let ghost = $derived(locked && live);

  // anything that throws work away asks once; the second click does it
  let armed = $state(null);
  let armTimer;
  function danger(key, action) {
    clearTimeout(armTimer);
    if (armed === key) {
      armed = null;
      action();
      return;
    }
    armed = key;
    armTimer = setTimeout(() => (armed = null), 4000);
  }

  const reset = () => invoke('reset_stats');

  let menu = $state(null);

  let menuSize = $state({ w: 138, h: 96 });

  function onContext(e) {
    e.preventDefault();
    menu = { x: e.clientX, y: e.clientY };
  }

  // the overlay is only a couple of rows tall and clips its content, so the
  // menu is placed against the real window box and its measured size
  let menuPos = $derived.by(() => {
    if (!menu) return null;
    const pad = 4;
    return {
      x: Math.max(pad, Math.min(menu.x, window.innerWidth - menuSize.w - pad)),
      y: Math.max(pad, Math.min(menu.y, window.innerHeight - menuSize.h - pad)),
    };
  });

  const closeMenu = () => (menu = null);

  function menuAction(cmd) {
    closeMenu();
    invoke(cmd).catch(() => {});
  }

  async function toggleLock() {
    if (!cfg) cfg = await invoke('get_settings').catch(() => null);
    if (!cfg) return;
    cfg = { ...cfg, locked: !cfg.locked };
    invoke('save_settings', { settings: cfg }).catch(() => {});
  }
</script>

<!-- the window is nothing but the panel, so the menu belongs to the whole of it -->
<svelte:window onclick={closeMenu} onblur={closeMenu} oncontextmenu={onContext} />

<div
  bind:this={panelEl}
  class="panel"
  class:ghost
  class:held={snap?.paused}
  style:--frost="url({art('frozen')})"
  style:border-image-source="url({art('panel')})"
  style:opacity={cfg?.opacity ?? 1}
  data-tauri-drag-region={drag}
>
  <button
    class="lock"
    class:locked
    onclick={toggleLock}
    title={locked
      ? 'Locked — click to unlock'
      : 'Click to lock: the overlay becomes click-through except this button (Ctrl+Shift+L works too)'}
    aria-label="lock"
  >
    <img src={locked ? art('lock_gold') : art('lock_pale')} alt="" />
  </button>

  {#if shown('session')}
    <div class="row" data-tauri-drag-region={drag}>
      <div class="chip lg" style:border-image-source="url({art('chip_dark')})" title={status.tip}>
        <span class="dot {status.cls}"></span>
        <img src={snap?.paused ? art('frozen_icon') : icon('time')} alt="" class="ic" />
        <span class="val" class:frozen={snap?.paused}>{snap ? dur(sessionSecs) : '0:00:00'}</span>
      </div>
      <div class="chip md" style:border-image-source="url({art('chip_dark')})">
        <img src={icon(snap?.has_mail ? 'mail_1' : 'mail_0')} alt="" class="ic" />
        <span class="val">{snap?.has_mail ? 'Mail!' : 'No mail'}</span>
      </div>
      {#if shown('reset') && !ghost}
        <button
          class="btn md"
          style:--btn="url({art('button')})"
          style:--btn-hover="url({art('button_hover')})"
          style:--btn-down="url({art('button_down')})"
          onclick={() => danger('reset', reset)}>{armed === 'reset' ? 'Sure?' : 'Reset Stats'}</button
        >
      {:else}
        <div class="chip md" style:border-image-source="url({art('chip_dark')})">
          <span class="dot {status.cls}"></span>
          <span class="val">{fmt(snap?.kills.earned)} kills</span>
        </div>
      {/if}
    </div>
  {/if}

  {#if shown('gold')}
    <div class="row" data-tauri-drag-region={drag}>
      <div class="chip lg" style:border-image-source="url({art('chip_dark')})">
        <span class="coin" class:idle={!live} style:background-image="url({art('coin_strip')})"></span>
        <span class="val">{fmt(snap?.gold.total)}</span>
      </div>
      <div class="chip md" style:border-image-source="url({art('chip_dark')})">
        <span class="val">+{fmt(snap?.gold.earned)}</span>
      </div>
      <div class="chip md" style:border-image-source="url({art('chip_dark')})">
        <span class="val">{fmt(snap?.gold.per_hour)}/h</span>
      </div>
    </div>
  {/if}

  {#if shown('xp')}
    <div class="row" data-tauri-drag-region={drag}>
      <div class="chip lg" style:border-image-source="url({art('chip_dark')})">
        <img src={icon('xp')} alt="" class="ic" />
        <span class="val">{fmt(snap?.xp.total)}</span>
      </div>
      <div class="chip md" style:border-image-source="url({art('chip_dark')})">
        <span class="val">+{fmt(snap?.xp.earned)}</span>
      </div>
      <div class="chip md" style:border-image-source="url({art('chip_dark')})">
        <span class="val">{fmt(snap?.xp.per_hour)}/h</span>
      </div>
    </div>
  {/if}

  {#if shown('items')}
    <div class="row" data-tauri-drag-region={drag}>
      <div class="chip lg" style:border-image-source="url({art('chip_dark')})" title="Angelic | Unholy">
        <img src={icon('chest')} alt="" class="ic" />
        <span class="val">
          <span class="c-ang">{fmt(item('Angelic').total)}</span>
          <span class="c-blue">({fmt(item('Angelic').mf)})</span>
          | <span class="c-unh">{fmt(item('Unholy').total)}</span>
          <span class="c-blue">({fmt(item('Unholy').mf)})</span>
        </span>
      </div>
      <div class="chip md" style:border-image-source="url({art('chip_dark')})" title="Heroic | Set">
        <span class="val">
          <span class="c-her">{fmt(item('Heroic').total)}</span>
          <span class="c-blue">({fmt(item('Heroic').mf)})</span>
          | <span class="c-set">{fmt(item('Set').total)}</span>
          <span class="c-blue">({fmt(item('Set').mf)})</span>
        </span>
      </div>
      <div class="chip md" style:border-image-source="url({art('chip_dark')})">
        <span class="val">
          <span class="c-sat">{fmt(item('Satanic').total)}</span>
          <span class="c-blue">({fmt(item('Satanic').mf)})</span>
          | <span class="c-sat">{fmt(item('Satanic').per_hour)}/h</span>
        </span>
      </div>
    </div>
  {/if}

  {#if shown('vitals')}
    <div class="row" data-tauri-drag-region={drag}>
      <div
        class="chip lg"
        style:border-image-source="url({art('chip_dark')})"
        title="magic find, off the client's heartbeat rather than the character save"
      >
        <img src={icon('mf')} alt="" class="ic" />
        <span class="val c-blue">{snap?.mf ? `${fmt(snap.mf)}%` : '—'}</span>
      </div>
      <div class="chip md" style:border-image-source="url({art('chip_dark')})" title="character level">
        <span class="val">Lv {snap?.character?.level || '—'}</span>
      </div>
      <div class="chip md" style:border-image-source="url({art('chip_dark')})" title="hero level">
        <span class="val">HLv {snap?.character?.herolevel || '—'}</span>
      </div>
    </div>
  {/if}

  {#if shown('zone')}
    <div class="row" data-tauri-drag-region={drag}>
      <div class="chip lg buffs" style:border-image-source="url({art('chip_dark')})">
        {#each buffs as b}
          <img
            class="buff"
            src={b ? b.icon : defaultBuffIcon}
            alt=""
            title={b ? `${b.name} : ${b.desc}` : 'Satanic Zone'}
          />
        {/each}
      </div>
      <div class="zone" style:background-image="url({art('header')})" data-tauri-drag-region={drag}>
        <span class="zone-name" class:here={snap?.satanic_here}>
          {snap?.satanic_zone ? zoneName(snap.satanic_zone.zone) : '—'}
        </span>
      </div>
    </div>
  {/if}

  {#if menu}
    <div
      class="menu"
      style:border-image-source="url({art('chip_dark')})"
      style:left="{menuPos.x}px"
      style:top="{menuPos.y}px"
      bind:clientWidth={menuSize.w}
      bind:clientHeight={menuSize.h}
    >
      <button onclick={() => menuAction('full_mode')}>Dashboard</button>
      <button onclick={() => menuAction('hide_window')}>Hide to tray</button>
      <button
        onclick={() => danger('menu-reset', () => { closeMenu(); reset(); })}
        >{armed === 'menu-reset' ? 'Reset — sure?' : 'Reset stats'}</button
      >
      <button class="danger" onclick={() => menuAction('quit')}>Quit</button>
    </div>
  {/if}
</div>

<style>
  @font-face {
    font-family: 'CookieRun Bold';
    src: url('./assets/fonts/cookierunbold.ttf') format('truetype');
  }

  :global(html, body) {
    margin: 0;
    background: transparent;
    overflow: hidden;
    user-select: none;
    -webkit-user-select: none;
    cursor: default;
  }

  :global(img) {
    image-rendering: pixelated;
  }

  .panel {
    position: relative;
    box-sizing: border-box;
    width: 444px;
    border: 14px solid transparent;
    border-image-slice: 14 fill;
    border-image-width: 14px;
    border-image-repeat: stretch;
    image-rendering: pixelated;
    padding: 6px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-family: 'CookieRun Bold', sans-serif;
    font-size: 13px;
    color: var(--bone-6);
  }

  /* Held: the run's clock has stopped, by hand or because nothing has happened
     for a while. The panel wears the same ice the game lays over a frozen
     enemy, tiled and faint — enough to read as frozen at a glance, not enough
     to hide the numbers underneath. */
  .panel.held::after {
    content: '';
    position: absolute;
    inset: 0;
    background-image: var(--frost);
    background-size: 38px 50px;
    image-rendering: pixelated;
    opacity: 0.28;
    mix-blend-mode: screen;
    pointer-events: none;
  }
  .panel.held .val { color: #bfe4ff; }
  .val.frozen { color: #dff2ff; }

  /* the border box stays, only its art goes — layout must not shift */
  .panel.ghost {
    border-image-source: none !important;
    background: none;
  }

  .row {
    display: flex;
    gap: 8px;
    justify-content: space-between;
    align-items: center;
  }

  .chip {
    box-sizing: border-box;
    height: 27px;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0 8px;
    border: 6px solid transparent;
    border-image-slice: 6 fill;
    border-image-width: 6px;
    image-rendering: pixelated;
    white-space: nowrap;
  }

  /* two paired counters per chip need the room; keep the row at 388px */
  .chip.lg { width: 140px; }
  .chip.md { width: 124px; }

  .ic {
    height: 20px;
    width: auto;
    max-width: 24px;
    flex: none;
    filter: brightness(1.2) drop-shadow(0 1px 1px rgba(0, 0, 0, 0.8));
  }

  .coin {
    width: 18px;
    height: 17px;
    flex: none;
    background-repeat: no-repeat;
    image-rendering: pixelated;
    /* a transparent always-on-top window recomposites on every frame of this,
       so it runs at half speed and stops when no game is being captured */
    animation: coin-spin 2.2s steps(11) infinite;
  }
  .coin.idle {
    animation: none;
  }
  @keyframes coin-spin {
    to { background-position: -198px 0; }
  }

  .val { margin-left: auto; overflow: hidden; text-overflow: ellipsis; }

  .btn {
    box-sizing: border-box;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    line-height: 1;
    height: 28px;
    width: 106px;
    font: inherit;
    font-size: 12px;
    color: var(--bone-12);
    text-shadow: 0 1px 0 var(--ground-2);
    background: var(--btn) no-repeat;
    background-size: 100% 100%;
    image-rendering: pixelated;
    border: none;
    cursor: pointer;
    padding: 0 0 2px;
  }
  .btn:hover { background-image: var(--btn-hover); }
  .btn:active { background-image: var(--btn-down); }

  .lock {
    position: absolute;
    top: -9px;
    right: -6px;
    width: 21px;
    height: 30px;
    padding: 0;
    background: none;
    border: none;
    cursor: pointer;
    z-index: 1;
    opacity: 0;
    pointer-events: none;
    transition: opacity 0.15s;
  }
  .panel:hover .lock {
    opacity: 0.9;
    pointer-events: auto;
  }
  .panel:hover .lock:hover,
  .panel:hover .lock.locked { opacity: 1; }
  .lock img {
    width: 21px;
    height: 30px;
    display: block;
    filter: drop-shadow(0 1px 2px rgba(0, 0, 0, 0.9));
  }

  .dot { width: 7px; height: 7px; border-radius: 50%; flex: none; }
  .dot.ok { background: #4caf50; }
  .dot.warn { background: #e0b040; }
  .dot.err { background: #d04040; }

  .chip.buffs { gap: 10px; justify-content: center; }
  .buff { width: 21px; height: 21px; }

  .zone {
    box-sizing: border-box;
    width: 240px;
    height: 29px;
    display: flex;
    align-items: center;
    justify-content: center;
    background-size: 100% 100%;
    background-repeat: no-repeat;
    image-rendering: pixelated;
    padding: 0 24px;
  }
  .zone-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 12px;
  }

  .c-ang { color: #f6f794; }
  .c-her { color: #00ffae; }
  /* the game says outright when this is the room you are in */
  .zone-name.here { color: #ff6a6a; }

  .c-sat { color: #ca1717; }
  .c-blue { color: #5050ae; }
  .c-set { color: #40d040; }
  .c-unh { color: #e04a7a; }

  .menu {
    position: fixed;
    z-index: 10;
    box-sizing: border-box;
    width: 138px;
    border: 6px solid transparent;
    border-image-slice: 6 fill;
    border-image-width: 6px;
    image-rendering: pixelated;
    padding: 3px;
    display: flex;
    flex-direction: column;
  }
  .menu button {
    font: inherit;
    font-size: 12px;
    color: var(--bone-6);
    text-align: left;
    background: none;
    border: none;
    cursor: pointer;
    padding: 4px 8px;
  }
  .menu button:hover {
    background: rgba(150, 37, 56, 0.55);
    color: var(--bone-13);
  }
  .menu button.danger:hover {
    background: rgba(180, 30, 30, 0.7);
  }
</style>

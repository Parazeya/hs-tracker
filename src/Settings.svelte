<script>
  import { invoke } from './bridge.js';
  import { art } from './skin.svelte.js';
  import { listen } from './bridge.js';

  let settings = $state(null);

  // Where no overlay can exist, the settings that only steer it say so instead
  // of pretending to work. Nothing is drawn until the backend answers: guessing
  // would flash a row of controls that then vanish.
  let session = $state(null);
  let overlay = $derived(session?.overlay ?? false);
  $effect(() => {
    invoke('session_info')
      .then((s) => (session = s))
      .catch(() => (session = { overlay: true, wayland: false, through_x11: false, can_switch: false }));
  });

  let notice = $state('');
  async function restart(x11) {
    // a pending edit would die with this process
    if (saveTimer) {
      clearTimeout(saveTimer);
      saveTimer = null;
      await invoke('save_settings', { settings: $state.snapshot(settings) }).catch(() => {});
    }
    try {
      await invoke('restart_backend', { x11 });
    } catch (e) {
      notice = String(e);
    }
  }

  // Settings are shared: a hotkey, the tray or another section can change them
  // while this one is open. Without following along, the next save here would
  // write back the copy loaded on open and undo them.
  $effect(() => {
    invoke('get_settings').then((s) => (settings = s));
    const unsubs = [
      listen('settings-changed', (e) => {
        if (!saveTimer) settings = e.payload;
      }),
    ];
    return () => unsubs.forEach((u) => u.then((f) => f()));
  });

  let saveTimer = null;
  function save() {
    clearTimeout(saveTimer);
    const snapshot = $state.snapshot(settings);
    saveTimer = setTimeout(() => {
      saveTimer = null;
      invoke('save_settings', { settings: snapshot }).catch(() => {});
    }, 150);
  }

  /// Sliders fire `input` while the DOM settles, which would persist a value
  /// the user never chose — only write on a real change.
  function setNumber(key, value) {
    if (!settings || !Number.isFinite(value) || settings[key] === value) return;
    settings[key] = value;
    save();
  }

  const SECTIONS = [
    ['session', 'Session timer, mail & reset'],
    ['gold', 'Gold'],
    ['xp', 'Experience'],
    ['items', 'Item counters'],
    ['vitals', 'Magic find & levels'],
    ['zone', 'Satanic zone'],
    ['reset', 'Reset Stats button'],
  ];

  const TIERS = ['D', 'C', 'B', 'A', 'S', 'SS'];

  // the two that are stored as a fraction but shown as a percentage
  let scalePct = $state(100);
  let shadePct = $state(55);
  $effect(() => {
    if (!settings) return;
    scalePct = Math.round((settings.flourish_scale ?? 1) * 100);
    shadePct = Math.round((settings.flourish_shade ?? 0.55) * 100);
  });

  function toggleFlourish(name) {
    const on = new Set(settings.flourish_rarities ?? []);
    on.has(name) ? on.delete(name) : on.add(name);
    settings.flourish_rarities = [...on];
    save();
  }

  function toggleSection(id) {
    const hidden = new Set(settings.hidden ?? []);
    hidden.has(id) ? hidden.delete(id) : hidden.add(id);
    settings.hidden = [...hidden];
    save();
  }


</script>

<div class="panel">
  <div class="body">
  {#if settings && session}
    <div class="section" style:border-image-source="url({art('chip_dark')})">
      {#if overlay}
        <div class="line" data-tauri-drag-region>
          <span class="name">Opacity</span>
          <input
            type="range"
            min="30"
            max="100"
            value={Math.round((settings.opacity ?? 1) * 100)}
            oninput={(e) => setNumber('opacity', e.target.value / 100)}
          />
          <span class="pct">{Math.round((settings.opacity ?? 1) * 100)}%</span>
        </div>
        <div class="line" data-tauri-drag-region>
          <span class="name">Scale</span>
          <input
            type="range"
            min="60"
            max="150"
            value={Math.round((settings.scale ?? 1) * 100)}
            oninput={(e) => setNumber('scale', e.target.value / 100)}
          />
          <span class="pct">{Math.round((settings.scale ?? 1) * 100)}%</span>
        </div>
        <div class="line" data-tauri-drag-region>
          <button class="check" onclick={() => { settings.auto_show = !settings.auto_show; save(); }} aria-label="auto show">
            <img src={settings.auto_show ? art('check_on') : art('check_off')} alt="" />
          </button>
          <span class="opt">Show / hide the overlay with the game</span>
        </div>
      {/if}
      <div class="line" data-tauri-drag-region>
        <span class="name">Theme</span>
        <select
          class="picker"
          value={settings.theme ?? 'default'}
          onchange={(e) => { settings.theme = e.target.value; save(); }}
        >
          <option value="default">Hero Siege</option>
          <option value="ebontharn">Ebontharn</option>
        </select>
      </div>
      <div class="line" data-tauri-drag-region>
        <button class="check" onclick={() => { settings.autostart = !settings.autostart; save(); }} aria-label="autostart">
          <img src={settings.autostart ? art('check_on') : art('check_off')} alt="" />
        </button>
        <span class="opt">Start on login</span>
      </div>
      <div class="line" data-tauri-drag-region>
        <button class="check" onclick={() => { settings.auto_pause = !settings.auto_pause; save(); }} aria-label="auto pause">
          <img src={settings.auto_pause ? art('check_on') : art('check_off')} alt="" />
        </button>
        <span class="opt" title="After five quiet minutes the clock stops and the idle time is taken back out, so the per-hour figures describe the farming rather than the break">
          Pause the session when nothing happens
        </span>
      </div>
      <div class="line" data-tauri-drag-region>
        <button class="check" onclick={() => { settings.discord = !settings.discord; save(); }} aria-label="discord">
          <img src={settings.discord ? art('check_on') : art('check_off')} alt="" />
        </button>
        <span class="opt" title="Zone, difficulty, the drops so far and how long the run has been going">
          Show the run in Discord while the game is open
        </span>
      </div>
      {#if overlay}
        <div class="line" data-tauri-drag-region>
          <button class="check" onclick={() => { settings.flourish = !settings.flourish; save(); }} aria-label="flourish">
            <img src={settings.flourish ? art('check_on') : art('check_off')} alt="" />
          </button>
          <span class="opt" title="The game's own loot pillar, played over the screen where you put it">
            Announce the best drops with the game's loot pillar
          </span>
        </div>
        {#if settings.flourish}
          <div class="line" data-tauri-drag-region>
            <span class="name">Size</span>
            <input
              type="range" min="50" max="200"
              bind:value={scalePct}
              oninput={() => setNumber('flourish_scale', scalePct / 100)}
            />
            <span class="pct">{Math.round((settings.flourish_scale ?? 1) * 100)}%</span>
          </div>
          <div class="line" data-tauri-drag-region>
            <span class="name">On screen</span>
            <input
              type="range" min="2" max="12" step="0.5"
              bind:value={settings.flourish_secs}
              oninput={() => save()}
            />
            <span class="pct">{(settings.flourish_secs ?? 6).toFixed(1)}s</span>
          </div>
          <div class="line" data-tauri-drag-region>
            <span class="name">Shading</span>
            <input
              type="range" min="0" max="90"
              bind:value={shadePct}
              oninput={() => setNumber('flourish_shade', shadePct / 100)}
            />
            <span class="pct">{Math.round((settings.flourish_shade ?? 0.55) * 100)}%</span>
          </div>
          <div class="grid">
            {#each ['Satanic', 'Set', 'Heroic', 'Angelic', 'Unholy'] as name}
              <button class="secopt" onclick={() => toggleFlourish(name)}>
                <img src={(settings.flourish_rarities ?? []).includes(name) ? art('check_on') : art('check_off')} alt="" />
                <span>{name}</span>
              </button>
            {/each}
          </div>
          <div class="line" data-tauri-drag-region>
            <span class="name">Least grade</span>
            <input
              type="range" min="1" max="6"
              bind:value={settings.flourish_tier}
              oninput={() => save()}
            />
            <span class="pct">{TIERS[(settings.flourish_tier ?? 6) - 1]}</span>
          </div>
          <div class="line" data-tauri-drag-region>
            <button class="check" onclick={() => { settings.flourish_always = !settings.flourish_always; save(); }} aria-label="flourish always">
              <img src={settings.flourish_always ? art('check_on') : art('check_off')} alt="" />
            </button>
            <span class="opt" title="It draws nothing between drops, but OBS can only capture a window that is there">
              Keep its window on screen so OBS can capture it
            </span>
          </div>
          <div class="line">
            <button
              class="btn wide"
              style:--btn="url({art('button')})"
              style:--btn-hover="url({art('button_hover')})"
              style:--btn-down="url({art('button_down')})"
              onclick={() => invoke('place_flourish', { placing: true })}
            >
              Place it on the screen…
            </button>
          </div>
        {/if}
        <div class="line" data-tauri-drag-region>
          <button class="check" onclick={() => { settings.ticker = !settings.ticker; save(); }} aria-label="ticker">
            <img src={settings.ticker ? art('check_on') : art('check_off')} alt="" />
          </button>
          <span class="opt">Drop ticker under the overlay</span>
        </div>
      {/if}
      <div class="line" data-tauri-drag-region>
        <button
          class="check"
          onclick={() => { settings.sound_on_ground = !settings.sound_on_ground; save(); }}
          aria-label="sound on ground"
        >
          <img src={settings.sound_on_ground ? art('check_on') : art('check_off')} alt="" />
        </button>
        <span class="opt">Alert when the item drops (off = when picked up)</span>
      </div>
      <div class="line" data-tauri-drag-region>
        <button class="check" onclick={() => { settings.stream = !settings.stream; save(); }} aria-label="stream">
          <img src={settings.stream ? art('check_on') : art('check_off')} alt="" />
        </button>
        <span class="opt" title="Serves the overlay as a page on this machine so OBS can add it as a Browser Source. The addresses are in About.">
          Serve the overlay to OBS
        </span>
      </div>
      {#if settings.stream}
        <div class="line" data-tauri-drag-region>
          <span class="name">Port</span>
          <input
            class="port"
            type="number" min="1024" max="65535"
            value={settings.stream_port ?? 4600}
            onchange={(e) => setNumber('stream_port', Math.trunc(Number(e.target.value)))}
          />
          <span class="pct">127.0.0.1 only</span>
        </div>
      {/if}
      <div class="line" data-tauri-drag-region>
        <button class="check" onclick={() => { settings.debug_log = !settings.debug_log; save(); }} aria-label="debug">
          <img src={settings.debug_log ? art('check_on') : art('check_off')} alt="" />
        </button>
        <span class="opt">Log parsed packets to debug-capture.jsonl</span>
      </div>
      {#if overlay}
        <div class="hotkeys" data-tauri-drag-region>
          Ctrl+Shift+O — show/hide · Ctrl+Shift+L — lock · Ctrl+Shift+R — reset stats ·
          Ctrl+Shift+P — pause
        </div>
      {:else}
        <div class="hotkeys" data-tauri-drag-region>
          Wayland session — the dashboard runs alone. An application there cannot
          place a window above the game, read the pointer outside itself or take
          global hotkeys. Running through XWayland brings all three back, and the
          game does the same when it runs through Proton, so the two meet in one
          X server.
        </div>
        {#if session.can_switch}
          <div class="line">
            <button
              class="btn wide"
              style:--btn="url({art('button')})"
              style:--btn-hover="url({art('button_hover')})"
              style:--btn-down="url({art('button_down')})"
              onclick={() => restart(true)}
            >
              Enable the overlay — restart through XWayland
            </button>
          </div>
        {:else}
          <div class="hotkeys" data-tauri-drag-region>
            This session has no XWayland to switch to, so the overlay stays out
            of reach here.
          </div>
        {/if}
      {/if}
      {#if session.wayland && session.through_x11}
        <div class="line">
          <button
            class="btn wide"
            style:--btn="url({art('button')})"
            style:--btn-hover="url({art('button_hover')})"
            style:--btn-down="url({art('button_down')})"
            onclick={() => restart(false)}
            title="Native Wayland is sharper and scales better, but has no overlay"
          >
            Back to native Wayland
          </button>
        </div>
      {/if}
      {#if notice}<div class="notice">{notice}</div>{/if}
    </div>

    {#if overlay}
      <div class="section" style:border-image-source="url({art('chip_dark')})">
        <div class="sechead" data-tauri-drag-region>Overlay sections</div>
        <div class="grid">
          {#each SECTIONS as [id, label]}
            <button class="secopt" onclick={() => toggleSection(id)}>
              <img src={(settings.hidden ?? []).includes(id) ? art('check_off') : art('check_on')} alt="" />
              <span>{label}</span>
            </button>
          {/each}
        </div>
      </div>
    {/if}

  {/if}
  </div>
</div>

<style>
  @font-face {
    font-family: 'CookieRun Bold';
    src: url('./assets/fonts/cookierunbold.ttf') format('truetype');
  }

  :global(html, body) {
    margin: 0;
    height: 100%;
    background: transparent;
    overflow: hidden;
    user-select: none;
    -webkit-user-select: none;
    cursor: default;
  }

  :global(img) {
    image-rendering: pixelated;
  }

  :global(#app) { height: 100%; }

  .panel {
    position: relative;
    box-sizing: border-box;
    width: 100%;
    height: 100%;
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-family: 'CookieRun Bold', sans-serif;
    font-size: 13px;
    color: var(--bone-6);
  }

  /* the list grows as features are added, so it scrolls instead of clipping */
  .body {
    flex: 1 1 auto;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding-right: 2px;
  }
  .body::-webkit-scrollbar { width: 6px; }
  .body::-webkit-scrollbar-thumb { background: var(--dim-1); border-radius: 3px; }

  .section {
    box-sizing: border-box;
    flex: none;
    border: 6px solid transparent;
    border-image-slice: 6 fill;
    border-image-width: 6px;
    image-rendering: pixelated;
    padding: 2px 6px 4px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  /* the rows keep their shape on a wide window instead of stretching across it */
  .line {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 22px;
    max-width: 620px;
  }

  .check {
    width: 27px;
    height: 27px;
    padding: 0;
    background: none;
    border: none;
    cursor: pointer;
    flex: none;
  }
  .check:hover { filter: brightness(1.25); }
  .check img { width: 27px; height: 27px; display: block; }

  .name { width: 108px; flex: none; }
  .opt { font-size: 12px; }

  .sechead {
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--bone-4);
    padding: 2px 0 4px;
  }
  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 2px 10px;
  }
  .secopt {
    display: flex;
    align-items: center;
    gap: 6px;
    font: inherit;
    font-size: 11px;
    color: var(--bone-6);
    background: none;
    border: none;
    cursor: pointer;
    padding: 1px 0;
    text-align: left;
  }
  .secopt img { width: 19px; height: 19px; flex: none; }
  .secopt:hover { color: var(--bone-13); }

  .picker {
    flex: 1 1 auto;
    min-width: 0;
    box-sizing: border-box;
    appearance: none;
    -webkit-appearance: none;
    font: inherit;
    font-size: 11px;
    color: var(--bone-13);
    background-color: rgba(0, 0, 0, 0.35);
    background-image: linear-gradient(45deg, transparent 50%, var(--bone-6) 50%),
      linear-gradient(135deg, var(--bone-6) 50%, transparent 50%);
    background-position: calc(100% - 12px) 50%, calc(100% - 7px) 50%;
    background-size: 5px 5px, 5px 5px;
    background-repeat: no-repeat;
    border: 1px solid var(--ground-10);
    border-radius: 0;
    padding: 3px 22px 3px 6px;
    height: 24px;
    cursor: pointer;
  }
  .picker:hover { border-color: var(--edge-4); }
  .picker:focus,
  .picker:focus-visible {
    outline: none;
    border-color: var(--edge-4);
  }
  /* the popup list is the toolkit's own window; these are the only two
     properties it honours */
  .picker option {
    background: var(--ground-7);
    color: var(--bone-9);
  }

  .tabs {
    flex: none;
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }

  .port {
    width: 78px;
    box-sizing: border-box;
    font: inherit;
    font-size: 11px;
    color: var(--bone-13);
    background: rgba(0, 0, 0, 0.35);
    border: 1px solid var(--ground-10);
    padding: 3px 6px;
    height: 24px;
  }
  .port:focus { outline: none; border-color: var(--edge-4); }

  .hotkeys {
    font-size: 10px;
    line-height: 15px;
    color: var(--edge-8);
    text-align: center;
    padding-top: 2px;
    max-width: 620px;
  }
  /* A slider that runs the whole width of a wide window is harder to aim, not
     easier — it stops growing well before that. The rail and the handle are
     drawn by us: left to itself each engine has its own idea of a slider, and
     WebKitGTK's is a fat bar with a big white dot. */
  input[type='range'] {
    flex: 1 1 auto;
    max-width: 260px;
    height: 14px;
    appearance: none;
    -webkit-appearance: none;
    background: none;
    cursor: pointer;
  }
  input[type='range']::-webkit-slider-runnable-track {
    height: 4px;
    background: var(--ground-7);
    border: 1px solid var(--ground-11);
  }
  input[type='range']::-webkit-slider-thumb {
    -webkit-appearance: none;
    width: 11px;
    height: 11px;
    margin-top: -5px;
    background: var(--bone-6);
    border: 1px solid var(--ground-7);
  }
  input[type='range']:hover:not(:disabled)::-webkit-slider-thumb { background: var(--bone-13); }
  input[type='range']:disabled { opacity: 0.4; cursor: default; }

  .pct { width: 38px; text-align: right; flex: none; font-size: 12px; }

  .btn {
    box-sizing: border-box;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    line-height: 1;
    height: 28px;
    flex: none;
    font: inherit;
    font-size: 11px;
    color: var(--bone-13);
    text-shadow: 0 1px 0 var(--ground-1);
    border: 6px solid transparent;
    border-image-source: var(--btn);
    border-image-slice: 6 fill;
    border-image-width: 6px;
    image-rendering: pixelated;
    padding: 0 12px;
    cursor: pointer;
  }
  .btn:hover { border-image-source: var(--btn-hover); }
  .btn:active { border-image-source: var(--btn-down); }
  .btn.wide { width: 100%; max-width: 380px; }

  .notice {
    font-size: 10px;
    line-height: 15px;
    color: #e06a6a;
    padding: 2px 2px 0;
    max-width: 620px;
  }
</style>

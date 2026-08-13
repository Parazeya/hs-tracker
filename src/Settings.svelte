<script>
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';

  import chipBg from './assets/game/chip_dark.png';
  import checkOff from './assets/game/check_off.png';
  import checkOn from './assets/game/check_on.png';

  let settings = $state(null);

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
    ['zone', 'Satanic zone'],
    ['reset', 'Reset Stats button'],
  ];

  function toggleSection(id) {
    const hidden = new Set(settings.hidden ?? []);
    hidden.has(id) ? hidden.delete(id) : hidden.add(id);
    settings.hidden = [...hidden];
    save();
  }

</script>

<div class="panel">
  <div class="body">
  {#if settings}
    <div class="section" style:border-image-source="url({chipBg})">
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
          <img src={settings.auto_show ? checkOn : checkOff} alt="" />
        </button>
        <span class="opt">Show / hide the overlay with the game</span>
      </div>
      <div class="line" data-tauri-drag-region>
        <button class="check" onclick={() => { settings.autostart = !settings.autostart; save(); }} aria-label="autostart">
          <img src={settings.autostart ? checkOn : checkOff} alt="" />
        </button>
        <span class="opt">Start on login</span>
      </div>
      <div class="line" data-tauri-drag-region>
        <button class="check" onclick={() => { settings.ticker = !settings.ticker; save(); }} aria-label="ticker">
          <img src={settings.ticker ? checkOn : checkOff} alt="" />
        </button>
        <span class="opt">Drop ticker under the overlay</span>
      </div>
      <div class="line" data-tauri-drag-region>
        <button
          class="check"
          onclick={() => { settings.sound_on_ground = !settings.sound_on_ground; save(); }}
          aria-label="sound on ground"
        >
          <img src={settings.sound_on_ground ? checkOn : checkOff} alt="" />
        </button>
        <span class="opt">Alert when the item drops (off = when picked up)</span>
      </div>
      <div class="line" data-tauri-drag-region>
        <button class="check" onclick={() => { settings.debug_log = !settings.debug_log; save(); }} aria-label="debug">
          <img src={settings.debug_log ? checkOn : checkOff} alt="" />
        </button>
        <span class="opt">Log parsed packets to debug-capture.jsonl</span>
      </div>
      <div class="hotkeys" data-tauri-drag-region>
        Ctrl+Shift+O — show/hide · Ctrl+Shift+L — lock · Ctrl+Shift+R — reset stats
      </div>
    </div>

    <div class="section" style:border-image-source="url({chipBg})">
      <div class="sechead" data-tauri-drag-region>Overlay sections</div>
      <div class="grid">
        {#each SECTIONS as [id, label]}
          <button class="secopt" onclick={() => toggleSection(id)}>
            <img src={(settings.hidden ?? []).includes(id) ? checkOff : checkOn} alt="" />
            <span>{label}</span>
          </button>
        {/each}
      </div>
    </div>
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
    color: #c3af75;
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
  .body::-webkit-scrollbar-thumb { background: #4a3a3a; border-radius: 3px; }

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
    color: #9a8a68;
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
    color: #c3af75;
    background: none;
    border: none;
    cursor: pointer;
    padding: 1px 0;
    text-align: left;
  }
  .secopt img { width: 19px; height: 19px; flex: none; }
  .secopt:hover { color: #f0e0b0; }

  .hotkeys {
    font-size: 10px;
    color: #8a7a5a;
    text-align: center;
    padding-top: 2px;
  }
  /* a slider that runs the whole width of a wide window is harder to aim, not
     easier — it stops growing well before that */
  input[type='range'] {
    flex: 1 1 auto;
    max-width: 260px;
    accent-color: #c3af75;
    height: 14px;
  }

  .pct { width: 38px; text-align: right; flex: none; font-size: 12px; }
</style>

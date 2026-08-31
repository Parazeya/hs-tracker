<script>
  import { invoke, recall, remember } from './bridge.js';
  import { art, css } from './skin.svelte.js';
  import { listen } from './bridge.js';

  let settings = $state(null);

  // Where no overlay can exist, the settings that only steer it say so instead
  // of pretending to work. Nothing is drawn until the backend answers: guessing
  // would flash a row of controls that then vanish.
  // Most of these are set once and never touched again. Keeping them all on
  // screen made the page long enough that the four people actually look for —
  // theme, scale, autostart, the overlay — were lost among them. Nothing is
  // removed; the rest is one click away and stays open once opened.
  let advanced = $state(recall('settings-advanced') === '1');
  $effect(() => remember('settings-advanced', advanced ? '1' : '0'));

  let session = $state(null);
  let overlay = $derived(session?.overlay ?? false);
  // The frameless look is only worth offering where it can be judged, and only
  // worth warning about where it costs something — see App.svelte.
  const smears = typeof document !== 'undefined' && document.documentElement.dataset.os === 'linux';
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
    invoke('get_settings').then((s) => {
      settings = s;
      base = JSON.parse(JSON.stringify(s));
    });
    const unsubs = [
      listen('settings-changed', (e) => {
        // A change from the tray, a hotkey or another panel, arriving while
        // this one has an unsaved edit of its own.
        //
        // Taking it whole would undo the edit. Throwing it away — which is what
        // this did — loses the other change for good, because nothing sends it
        // again; the window was only 150ms wide, but a tray toggle lands inside
        // it easily enough and then reads as a switch that did not stick.
        //
        // So every field is taken except the ones edited here, and "edited
        // here" is whatever now differs from the copy the backend last handed
        // over. That needs nothing from the controls themselves, which is the
        // point: there are forty of them and any one that forgot to say would
        // be the bug back again.
        if (!saveTimer || !settings || !base) {
          settings = e.payload;
          base = JSON.parse(JSON.stringify(e.payload));
          return;
        }
        const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);
        for (const [k, v] of Object.entries(e.payload)) {
          if (same(settings[k], base[k])) settings[k] = v;
        }
      }),
    ];
    return () => unsubs.forEach((u) => u.then((f) => f()));
  });

  // Import replaces everything, so it asks once — the second click does it.
  let armedBundle = $state(false);
  let armTimer;
  async function bundle(command) {
    if (command === 'import_settings' && !armedBundle) {
      armedBundle = true;
      notice = 'Import replaces every setting — click again to confirm';
      clearTimeout(armTimer);
      armTimer = setTimeout(() => { armedBundle = false; notice = ''; }, 5000);
      return;
    }
    armedBundle = false;
    try {
      const file = await invoke(command);
      notice = file ? `${command === 'export_settings' ? 'Saved to' : 'Loaded'} ${file}` : '';
    } catch (e) {
      notice = String(e);
    }
  }

  let saveTimer = null;
  /// The settings as the backend last handed them over. Anything that differs
  /// from this is an edit made here and not yet written.
  let base = null;
  function save() {
    clearTimeout(saveTimer);
    const snapshot = $state.snapshot(settings);
    saveTimer = setTimeout(() => {
      saveTimer = null;
      base = JSON.parse(JSON.stringify(snapshot));
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
  {#if settings && session}
    <div class="section" style:border-image-source={css('chip_dark')}>
      {#if overlay && advanced}
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
        <div class="line" data-tauri-drag-region>
          <button class="check" onclick={() => { settings.ghost = !settings.ghost; save(); }} aria-label="ghost">
            <img src={settings.ghost ? art('check_on') : art('check_off')} alt="" />
          </button>
          <span
            class="opt"
            title={smears
              ? 'The locked overlay drops its frame and the numbers float over the game. This desktop does not clear the overlay window between frames, so they can leave the last few of their frames behind — the frame is what hides that.'
              : 'The locked overlay drops its frame, leaving the numbers over the game'}
          >
            Enable transparent overlay while locked{smears ? ' (can create artifacts)' : ''}
          </span>
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
          <option value="modern">Modern</option>
        </select>
      </div>
      <div class="line" data-tauri-drag-region>
        <button class="check" onclick={() => { settings.autostart = !settings.autostart; save(); }} aria-label="autostart">
          <img src={settings.autostart ? art('check_on') : art('check_off')} alt="" />
        </button>
        <span class="opt">Start on login</span>
      </div>
      <div class="line" data-tauri-drag-region>
        <button class="check" onclick={() => { settings.discord = !settings.discord; save(); }} aria-label="discord">
          <img src={settings.discord ? art('check_on') : art('check_off')} alt="" />
        </button>
        <span class="opt" title="Zone, difficulty, the drops so far and how long the run has been going">
          Show the run in Discord while the game is open
        </span>
      </div>
      <!-- The announcement moved to the Alerts page: what is worth telling
           you about and how you are told are one decision, and asking them on
           two different tabs is what made the announcement look inert. -->
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
      <!-- The OBS browser sources are gone: one route into OBS, and it is the
           one that needs no address, no port and no local server — capture the
           announcement window, which stays on screen for exactly that. -->
      <!-- Everything in one file: switches, filters, lists and the sound
           files themselves, which live outside settings.json and would
           otherwise arrive as silence on the other machine. -->
      <div class="line">
        <button
          class="btn"
          style:--btn={css('button')}
          style:--btn-hover={css('button_hover')}
          style:--btn-down={css('button_down')}
          onclick={() => bundle('export_settings')}
          title="Save every setting, filter and sound to one file"
        >Export all settings…</button>
        <button
          class="btn"
          style:--btn={css('button')}
          style:--btn-hover={css('button_hover')}
          style:--btn-down={css('button_down')}
          onclick={() => bundle('import_settings')}
          title="Replace every setting with the ones in a file"
        >Import…</button>
      </div>

      <button class="more" onclick={() => (advanced = !advanced)}>
        {advanced ? '▾' : '▸'} More settings
      </button>

      {#if advanced && overlay}
        <div class="line" data-tauri-drag-region>
          <button class="check" onclick={() => { settings.ticker = !settings.ticker; save(); }} aria-label="ticker">
            <img src={settings.ticker ? art('check_on') : art('check_off')} alt="" />
          </button>
          <span class="opt">Drop ticker under the overlay</span>
        </div>
      {/if}
      {#if advanced}
      <div class="line" data-tauri-drag-region>
        <button
          class="check"
          onclick={() => { settings.wide_capture = !settings.wide_capture; save(); }}
          aria-label="read every connection"
        >
          <img src={settings.wide_capture ? art('check_on') : art('check_off')} alt="" />
        </button>
        <span class="opt">Read every connection, not just the game's</span>
      </div>
      <div class="hint" data-tauri-drag-region>
        Normally only the connections Windows says the game holds are read. A route
        optimiser such as ExitLag redirects the game's packets underneath that, so those
        connections are not the ones on the wire and nothing gets counted. This reads
        everything instead — more work for the machine, and the only way to see the game
        through one.
      </div>
      <div class="line" data-tauri-drag-region>
        <button class="check" onclick={() => { settings.debug_log = !settings.debug_log; save(); }} aria-label="debug">
          <img src={settings.debug_log ? art('check_on') : art('check_off')} alt="" />
        </button>
        <span class="opt">Log parsed packets to debug-capture.jsonl</span>
      </div>
      {/if}
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
              style:--btn={css('button')}
              style:--btn-hover={css('button_hover')}
              style:--btn-down={css('button_down')}
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
            style:--btn={css('button')}
            style:--btn-hover={css('button_hover')}
            style:--btn-down={css('button_down')}
            onclick={() => restart(false)}
            title="Native Wayland is sharper and scales better, but has no overlay"
          >
            Back to native Wayland
          </button>
        </div>
      {/if}
      {#if notice}<div class="notice">{notice}</div>{/if}
    </div>

    {#if overlay && advanced}
      <div class="section" style:border-image-source={css('chip_dark')}>
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

  /* the seam between what people set and what they set once */
  .more {
    align-self: flex-start;
    margin: 4px 0 2px;
    padding: 2px 0;
    font: inherit;
    font-size: 11px;
    letter-spacing: 0.3px;
    color: var(--edge-2b);
    background: none;
    border: none;
    cursor: pointer;
  }
  /* Lighter on hover, not darker. `--edge-1` is a shadow colour — over this
     panel it took the label almost to the background, so the one control that
     opens the rest of the page disappeared under the cursor. */
  .more:hover { color: var(--bone-13); }

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



  /* An address is read character by character and typed into another program,
     so it is set in the system's monospace rather than the game's face — which
     has no glyph of its own for a good half of ASCII. */

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

  /* Why a switch is there, for the one switch whose reason is not obvious from
     its label. Same size as `.notice` and not its colour: this is an
     explanation, not a warning. */
  .hint {
    font-size: 10px;
    line-height: 15px;
    color: var(--bone-6);
    padding: 0 2px 6px 30px;
    max-width: 620px;
  }

  .notice {
    font-size: 10px;
    line-height: 15px;
    color: #e06a6a;
    padding: 2px 2px 0;
    max-width: 620px;
  }
</style>

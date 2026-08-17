<script>
  import { invoke } from './bridge.js';
  import { art } from './skin.svelte.js';
  import { listen } from './bridge.js';
  import { RARITIES, soundUrl, play } from './audio.js';

  const LABELS = {
    satanic: ['Satanic drop', 'c-sat'],
    set: ['Set drop', 'c-set'],
    heroic: ['Heroic drop', 'c-her'],
    angelic: ['Angelic drop', 'c-ang'],
    unholy: ['Unholy drop', 'c-unh'],
    mail: ['Mail reminder', 'c-gold'],
  };

  let settings = $state(null);
  let custom = $state({});
  let saveTimer;

  async function refreshStatus() {
    const next = {};
    for (const r of RARITIES) next[r] = await invoke('sound_status', { rarity: r }).catch(() => null);
    custom = next;
  }

  $effect(() => {
    invoke('get_settings').then((s) => (settings = s));
    refreshStatus();
    const unsubs = [
      listen('settings-changed', (e) => (settings = e.payload)),
      listen('sounds-changed', refreshStatus),
    ];
    return () => unsubs.forEach((u) => u.then((f) => f()));
  });

  function save() {
    clearTimeout(saveTimer);
    const snapshot = $state.snapshot(settings);
    saveTimer = setTimeout(() => invoke('save_settings', { settings: snapshot }).catch(() => {}), 150);
  }

  function toggle(rarity) {
    settings[rarity].enabled = !settings[rarity].enabled;
    save();
  }

  async function test(rarity) {
    play(await soundUrl(rarity), settings?.[rarity]?.volume ?? 0.7);
  }

  async function pickFile(rarity) {
    try {
      await invoke('pick_sound', { rarity });
      refreshStatus();
    } catch {}
  }

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

  const resetSound = (rarity) => invoke('clear_sound', { rarity }).catch(() => {});
</script>

<div class="panel">
  <div class="body">
  {#if settings}
    {#each RARITIES as rarity}
      <div class="section" style:border-image-source="url({art('chip_dark')})">
        <div class="line" data-tauri-drag-region>
          <button class="check" onclick={() => toggle(rarity)} aria-label="toggle">
            <img src={settings[rarity].enabled ? art('check_on') : art('check_off')} alt="" />
          </button>
          <span class="name {LABELS[rarity][1]}">{LABELS[rarity][0]}</span>
          <input
            type="range"
            min="0"
            max="100"
            value={Math.round(settings[rarity].volume * 100)}
            oninput={(e) => {
              const v = e.target.value / 100;
              if (settings[rarity].volume === v) return;
              settings[rarity].volume = v;
              save();
            }}
            disabled={!settings[rarity].enabled}
          />
          <span class="pct">{Math.round(settings[rarity].volume * 100)}%</span>
        </div>
        <div class="line sub" data-tauri-drag-region>
          <span class="src" title={custom[rarity] ? `sounds/${custom[rarity]}` : 'built-in sound'}>
            {custom[rarity] ?? 'built-in'}
          </span>
          <button
            class="btn sm"
            style:--btn="url({art('button')})"
            style:--btn-hover="url({art('button_hover')})"
            style:--btn-down="url({art('button_down')})"
            onclick={() => test(rarity)}>Test</button
          >
          <button
            class="btn sm"
            style:--btn="url({art('button')})"
            style:--btn-hover="url({art('button_hover')})"
            style:--btn-down="url({art('button_down')})"
            onclick={() => pickFile(rarity)}>Browse…</button
          >
          <button
            class="btn sm"
            style:--btn="url({art('button')})"
            style:--btn-hover="url({art('button_hover')})"
            style:--btn-down="url({art('button_down')})"
            disabled={!custom[rarity]}
            onclick={() => danger(rarity, () => resetSound(rarity))}
            >{armed === rarity ? 'Sure?' : 'Default'}</button
          >
        </div>
      </div>
    {/each}
    <div class="note">
      Each alert has its own file. <b>Browse…</b> copies yours into the
      <code>sounds</code> folder next to the settings, where it replaces the
      bundled one; <b>Default</b> puts the bundled one back. Lists in the sound
      filter bring their own sounds and outrank these.
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

  /* the rows keep their shape on a wide window instead of stretching the
     slider one way and the buttons the other */
  .line {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 22px;
    max-width: 620px;
  }

  .line.sub {
    padding-left: 35px;
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

  /* the closing paragraph explains where the files live; it should read as a
     footnote, not as another setting */
  .note {
    flex: none;
    font-size: 11px;
    line-height: 16px;
    color: var(--edge-8);
    padding: 0 2px 2px;
  }

  /* A slider that runs the whole width of a wide window is harder to aim, not
     easier — it stops growing well before that. The rail and the handle are
     drawn by us so the control looks the same on every engine. */
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

  .src {
    flex: 1;
    font-size: 11px;
    color: var(--edge-8);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .btn {
    box-sizing: border-box;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    line-height: 1;
    height: 26px;
    width: 66px;
    flex: none;
    font: inherit;
    font-size: 11px;
    color: var(--bone-12);
    text-shadow: 0 1px 0 var(--ground-2);
    background: var(--btn) no-repeat;
    background-size: 100% 100%;
    image-rendering: pixelated;
    border: none;
    cursor: pointer;
    padding: 0 0 2px;
  }
  .btn:hover:not(:disabled) { background-image: var(--btn-hover); }
  .btn:active:not(:disabled) { background-image: var(--btn-down); }
  .btn:disabled { opacity: 0.4; cursor: default; }

  .c-ang { color: #f6f794; }
  .c-her { color: #00ffae; }
  .c-sat { color: var(--rar-satanic); }
  .c-set { color: #40d040; }
  .c-unh { color: #e04a7a; }
  .c-gold { color: var(--gold-2); }
</style>

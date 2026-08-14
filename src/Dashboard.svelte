<script>
  import { invoke } from '@tauri-apps/api/core';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import Stats from './Stats.svelte';
  import Runs from './Runs.svelte';
  import Shop from './Shop.svelte';
  import SoundFilter from './SoundFilter.svelte';
  import Sounds from './Sounds.svelte';
  import Settings from './Settings.svelte';

  import panelBg from './assets/game/panel.png';
  import chipBg from './assets/game/chip_dark.png';
  import btnBg from './assets/game/button.png';
  import btnHoverBg from './assets/game/button_hover.png';
  import btnDownBg from './assets/game/button_down.png';
  import closeImg from './assets/game/close.png';
  import closeHoverImg from './assets/game/close_hover.png';
  import headerBg from './assets/game/header.png';

  const DIRECTIONS = {
    n: 'North',
    s: 'South',
    e: 'East',
    w: 'West',
    ne: 'NorthEast',
    nw: 'NorthWest',
    se: 'SouthEast',
    sw: 'SouthWest',
  };

  const SECTIONS = [
    { id: 'stats', label: 'Statistics', component: Stats },
    { id: 'runs', label: 'Runs', component: Runs },
    { id: 'filter', label: 'Sound Filter', component: SoundFilter },
    { id: 'sounds', label: 'Sounds', component: Sounds },
    { id: 'shop', label: 'Shopping List', component: Shop },
    { id: 'settings', label: 'Settings', component: Settings },
  ];

  // the section survives a hide/show, which is what makes the sidebar feel
  // like one window rather than four
  let section = $state(localStorage.getItem('section') ?? 'stats');

  // the backend pushes the heavy statistics payload only while it is the
  // section on screen, so it has to be told which one that is
  $effect(() => {
    localStorage.setItem('section', section);
    invoke('viewing', { section }).catch(() => {});
  });

  // a Wayland session cannot host the overlay, so the way into it is not shown
  let overlay = $state(true);
  $effect(() => {
    invoke('session_info')
      .then((s) => (overlay = s.overlay))
      .catch(() => {});
  });

  let Current = $derived((SECTIONS.find((s) => s.id === section) ?? SECTIONS[0]).component);
</script>

<div
  class="panel"
  style:border-image-source="url({panelBg})"
  style:--btn="url({btnBg})"
  style:--btn-hover="url({btnHoverBg})"
  style:--btn-down="url({btnDownBg})"
  data-tauri-drag-region
>
  <button
    class="min"
    onclick={() => getCurrentWindow().minimize()}
    title="Minimize to the taskbar"
    aria-label="minimize"
  >
    <span></span>
  </button>

  <button class="close" onclick={() => invoke('hide_dashboard')} title="Close to tray" aria-label="close">
    <img src={closeImg} alt="" class="close-normal" />
    <img src={closeHoverImg} alt="" class="close-hover" />
  </button>

  <div class="title" style:background-image="url({headerBg})" data-tauri-drag-region>
    <span>HS Tracker</span>
  </div>

  <div class="body">
    <nav class="nav" data-tauri-drag-region>
      {#each SECTIONS as s}
        <button class="tab" class:on={s.id === section} onclick={() => (section = s.id)}>{s.label}</button>
      {/each}

      <div class="spacer"></div>

      {#if overlay}
        <button
          class="btn"
          onclick={() => invoke('compact_mode')}
          title="Shrink to the overlay that sits on top of the game"
        >
          Compact mode
        </button>
      {/if}
    </nav>

    <div class="pane" style:border-image-source="url({chipBg})">
      <Current />
    </div>
  </div>

  {#each ['n', 's', 'e', 'w', 'ne', 'nw', 'se', 'sw'] as edge}
    <div
      class="grip {edge}"
      role="presentation"
      onmousedown={(e) => e.button === 0 && getCurrentWindow().startResizeDragging(DIRECTIONS[edge])}
    ></div>
  {/each}
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

  :global(#app) { height: 100%; }
  :global(img) { image-rendering: pixelated; }

  .panel {
    position: relative;
    box-sizing: border-box;
    width: 100%;
    height: 100%;
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
    font-size: 12px;
    color: #c3af75;
  }

  .title {
    height: 29px;
    flex: none;
    display: flex;
    align-items: center;
    justify-content: center;
    background-size: 100% 100%;
    background-repeat: no-repeat;
    font-size: 13px;
  }
  /* the drag region is the element under the cursor, and the caption is an
     element of its own — without this the window refuses to move by its name */
  .title span { pointer-events: none; }

  .close {
    position: absolute;
    top: 2px;
    right: 2px;
    width: 22px;
    height: 22px;
    padding: 0;
    border: none;
    background: none;
    cursor: pointer;
    z-index: 5;
  }
  .close img { width: 100%; height: 100%; }
  .close .close-hover { display: none; }
  .close:hover .close-normal { display: none; }
  .close:hover .close-hover { display: block; }

  /* The game's art has no minimise glyph, so the close plate is rebuilt in
     CSS — same square, same frame, a bar instead of the cross — and it lights
     up the way the cross does. */
  .min {
    position: absolute;
    top: 2px;
    right: 26px;
    width: 22px;
    height: 22px;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    box-sizing: border-box;
    border: 1px solid #a99873;
    background: #180d10;
    cursor: pointer;
    z-index: 5;
  }
  .min span {
    display: block;
    width: 12px;
    height: 3px;
    background: #b2262c;
  }
  .min:hover { border-color: #e0cc90; }
  .min:hover span { background: #e2453f; }

  .body {
    flex: 1 1 auto;
    min-height: 0;
    display: flex;
    gap: 6px;
  }

  .nav {
    flex: none;
    width: 116px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  /* The sidebar keeps the panel's own darkness — the grey chip art belongs to
     rows of data, not to navigation. The section you are in wears the game's
     button plate, and the same 6px transparent border on every state keeps the
     tabs from jumping when it changes. */
  .tab {
    box-sizing: border-box;
    display: flex;
    align-items: center;
    min-height: 30px;
    font: inherit;
    font-size: 12px;
    color: #9a8a68;
    text-align: left;
    border: 6px solid transparent;
    background: linear-gradient(180deg, #2c1a1d, #1b1013);
    image-rendering: pixelated;
    padding: 0 3px;
    cursor: pointer;
    text-shadow: 0 1px 0 #140a0a;
  }
  .tab:hover {
    color: #e2cf98;
    background: linear-gradient(180deg, #3b2126, #24151a);
  }
  .tab.on {
    color: #f4e6bb;
    background: none;
    border-image-source: var(--btn);
    border-image-slice: 6 fill;
    border-image-width: 6px;
  }
  .tab.on:hover { border-image-source: var(--btn-hover); }
  .tab.on:active { border-image-source: var(--btn-down); }

  .spacer { flex: 1 1 auto; }

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
    color: #f0e0b0;
    text-shadow: 0 1px 0 #140a0a;
    border: 6px solid transparent;
    border-image-source: var(--btn);
    border-image-slice: 6 fill;
    border-image-width: 6px;
    image-rendering: pixelated;
    cursor: pointer;
  }
  .btn:hover { border-image-source: var(--btn-hover); }
  .btn:active { border-image-source: var(--btn-down); }

  /* the frame is drawn by us, so the resize edges are ours to provide too */
  .grip { position: absolute; z-index: 6; }
  .grip.n, .grip.s { left: 8px; right: 8px; height: 6px; cursor: ns-resize; }
  .grip.e, .grip.w { top: 8px; bottom: 8px; width: 6px; cursor: ew-resize; }
  .grip.n { top: 0; }
  .grip.s { bottom: 0; }
  .grip.w { left: 0; }
  .grip.e { right: 0; }
  .grip.ne, .grip.nw, .grip.se, .grip.sw { width: 10px; height: 10px; }
  .grip.nw { top: 0; left: 0; cursor: nwse-resize; }
  .grip.se { bottom: 0; right: 0; cursor: nwse-resize; }
  .grip.ne { top: 0; right: 0; cursor: nesw-resize; }
  .grip.sw { bottom: 0; left: 0; cursor: nesw-resize; }

  .pane {
    flex: 1 1 auto;
    min-width: 0;
    box-sizing: border-box;
    border: 6px solid transparent;
    border-image-slice: 6 fill;
    border-image-width: 6px;
    image-rendering: pixelated;
    padding: 6px;
    overflow: hidden;
  }
</style>

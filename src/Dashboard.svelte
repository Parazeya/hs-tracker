<script>
  import { invoke } from '@tauri-apps/api/core';
  import { art } from './skin.svelte.js';
  import { listen } from '@tauri-apps/api/event';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import Stats from './Stats.svelte';
  import Runs from './Runs.svelte';
  import Shop from './Shop.svelte';
  import SoundFilter from './SoundFilter.svelte';
  import Sounds from './Sounds.svelte';
  import Settings from './Settings.svelte';

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

  // Why the numbers are not moving, said out loud. The overlay has always had a
  // coloured dot with a tooltip for this; on a Wayland session there is no
  // overlay, so a player watching zeros had nothing to read at all — twice now
  // that has cost a round of questions to work out what the app already knew.
  let snap = $state(null);
  $effect(() => {
    invoke('snapshot').then((s) => (snap = s)).catch(() => {});
    const unsub = listen('stats', (e) => (snap = e.payload));
    return () => unsub.then((f) => f());
  });

  let trouble = $derived.by(() => {
    if (!snap) return null;
    const status = snap.status ?? '';
    if (status === 'npcap-missing')
      return {
        bad: true,
        title: 'Npcap is not installed',
        detail:
          'It is the driver that lets the app read the game’s traffic. Without it nothing can be counted. Get it from npcap.com — its defaults are right.',
      };
    if (status === 'no-capture')
      return {
        bad: true,
        title: 'Not allowed to read network traffic',
        detail:
          'The binary needs the capture right. A packaged install grants it; an AppImage cannot, so it has to be given by hand:',
        fix: 'sudo setcap cap_net_raw,cap_net_admin=eip <the hs-tracker binary>',
      };
    if (status === 'no-interface')
      return { bad: true, title: 'No network interface to listen on', detail: 'No adapter could be opened for capture.' };
    if (status === 'waiting-for-game')
      return { bad: false, title: 'Waiting for Hero Siege', detail: 'Counting starts a moment after the game is running.' };
    if (status.startsWith('capturing')) {
      const [, iface, hosts] = status.split('|');
      // the game is up and adapters are open, but nothing of the game's own has
      // been seen — the usual causes are a sandbox around the game or a tunnel
      // its traffic takes that we are not on
      if (Number(hosts) === 0)
        return {
          bad: true,
          title: 'Listening, but the game’s traffic is not reaching us',
          detail:
            `Capturing on ${iface}. The game is running, yet none of its connections can be seen. A Flatpak or Snap install of Steam hides the game from us; a VPN or a second network adapter can carry its traffic somewhere we are not listening.`,
        };
      // hosts found, but the game has never once reported the character
      if (snap.save_age_secs == null && snap.bank_age_secs == null && snap.session_secs > 240)
        return {
          bad: false,
          title: 'Connected, still nothing from the game',
          detail:
            'Its traffic is being read, but no character save has arrived yet. Gold, experience and kills travel only when the game saves; if this stays after a few minutes of fighting, the packet log in Settings is worth switching on.',
        };
    }
    return null;
  });
</script>

<div
  class="panel"
  class:scenic={art('backdrop')}
  style:--backdrop="url({art('backdrop')})"
  style:border-image-source="url({art('panel')})"
  style:--btn="url({art('button')})"
  style:--btn-hover="url({art('button_hover')})"
  style:--btn-down="url({art('button_down')})"
  data-tauri-drag-region
>
  <button
    class="min"
    onclick={() => getCurrentWindow().minimize()}
    title="Minimize to the taskbar"
    aria-label="minimize"
  >
    <img src={art('minimize')} alt="" class="min-normal" />
    <img src={art('minimize_hover')} alt="" class="min-hover" />
  </button>

  <button class="close" onclick={() => invoke('hide_dashboard')} title="Close to tray" aria-label="close">
    <img src={art('close')} alt="" class="close-normal" />
    <img src={art('close_hover')} alt="" class="close-hover" />
  </button>

  <div class="title" style:background-image="url({art('header')})" data-tauri-drag-region>
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

    <div class="pane" style:border-image-source="url({art('chip_dark')})">
      {#if trouble}
        <div class="trouble" class:bad={trouble.bad}>
          <div class="tt">{trouble.title}</div>
          <div class="td">{trouble.detail}</div>
          {#if trouble.fix}<code class="tf">{trouble.fix}</code>{/if}
        </div>
      {/if}
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

  /* A season may bring its own sky. It sits behind everything, dimmed hard —
     the panel is a place to read numbers first and a view second. */
  .panel.scenic::before {
    content: '';
    position: absolute;
    inset: 14px;
    background-image: var(--backdrop);
    background-size: cover;
    background-position: center;
    opacity: 0.22;
    pointer-events: none;
  }
  /* Only the two blocks that sit in the flow need lifting above the sky. The
     close and minimize buttons and the resize grips are positioned already, and
     positioning them again as `relative` drops them back into the flow — which
     is exactly what sent them to the corner the first time. */
  .panel.scenic > .title,
  .panel.scenic > .body { position: relative; }

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
    color: var(--bone-6);
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
  /* The pair is the game's own close button and one built from it, so both
     wear the same frame and a reskin moves them together. It carried a
     hand-drawn border before, which under a season's colours was the one thing
     on the window that did not belong to it. */
  .min {
    position: absolute;
    top: 2px;
    right: 26px;
    width: 22px;
    height: 22px;
    padding: 0;
    border: none;
    background: none;
    cursor: pointer;
    z-index: 5;
  }
  .min img { width: 100%; height: 100%; display: block; }
  .min .min-hover { display: none; }
  .min:hover .min-normal { display: none; }
  .min:hover .min-hover { display: block; }

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
    color: var(--bone-4);
    text-align: left;
    border: 6px solid transparent;
    background: linear-gradient(180deg, var(--ground-8), var(--ground-4));
    image-rendering: pixelated;
    padding: 0 3px;
    cursor: pointer;
    text-shadow: 0 1px 0 var(--ground-1);
  }
  .tab:hover {
    color: var(--bone-10);
    background: linear-gradient(180deg, var(--ground-9), var(--ground-6));
  }
  .tab.on {
    color: var(--bone-15);
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
    color: var(--bone-13);
    text-shadow: 0 1px 0 var(--ground-1);
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
    display: flex;
    flex-direction: column;
  }

  /* Above whatever section is open, because it explains all of them at once.
     Amber for something to wait out, crimson for something to go and fix. */
  .trouble {
    flex: none;
    margin-bottom: 6px;
    padding: 6px 10px;
    border-left: 3px solid #8a7a4a;
    background: rgba(120, 96, 40, 0.16);
    font-family: 'CookieRun Bold', sans-serif;
  }
  .trouble.bad {
    border-left-color: #ca1717;
    background: rgba(150, 37, 56, 0.18);
  }
  .trouble .tt { font-size: 13px; color: var(--gold-2); }
  .trouble.bad .tt { color: #ff7a7a; }
  .trouble .td { font-size: 11px; color: var(--bone-7); line-height: 1.45; margin-top: 2px; }
  .trouble .tf {
    display: block;
    margin-top: 4px;
    padding: 3px 6px;
    font-family: ui-monospace, Consolas, monospace;
    font-size: 11px;
    color: var(--bone-11);
    background: rgba(0, 0, 0, 0.35);
    user-select: text;
    overflow-x: auto;
  }
</style>

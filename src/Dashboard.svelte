<script>
  import { locale, say, t } from './say.svelte.js';
  import { appWindow, invoke, recall, remember } from './bridge.js';
  import { art, css } from './skin.svelte.js';
  import { listen } from './bridge.js';
  import { dismissUpdate, installUpdate, lookForUpdate, updater } from './update.svelte.js';
  import Stats from './Stats.svelte';
  import Runs from './Runs.svelte';
  import Shop from './Shop.svelte';
  import SoundFilter from './SoundFilter.svelte';
  import Watchlist from './Watchlist.svelte';
  import Settings from './Settings.svelte';
  import About from './About.svelte';
  import Codex from './Codex.svelte';

  // Steam in a sandbox is a Linux problem and naming it on Windows sends a
  // player looking for something that cannot be there.
  const onLinux =
    typeof document !== 'undefined' && document.documentElement.dataset.os === 'linux';

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
    { id: 'filter', label: 'Alerts', component: SoundFilter },
    { id: 'watchlist', label: 'Watchlist', component: Watchlist },
    { id: 'codex', label: 'Items', component: Codex },
    { id: 'shop', label: 'Shopping List', component: Shop },
    { id: 'settings', label: 'Settings', component: Settings },
    { id: 'about', label: 'About', component: About },
  ];

  // the section survives a hide/show, which is what makes the sidebar feel
  // like one window rather than four
  //
  // Checked against the list it is restored into: a section that was removed by
  // an update is still sitting in the browser's storage on the machines that
  // had it open, and a name nothing answers to was carried on being reported to
  // the backend. It gates the heavy statistics payload, so Statistics drew but
  // its timeline and its graph never moved and no tab looked selected — which
  // reads exactly like the window being broken, until the next click on a tab
  // quietly repairs it.
  const remembered = recall('section');
  let section = $state(
    SECTIONS.some((s) => s.id === remembered) ? remembered : 'stats'
  );

  // the backend pushes the heavy statistics payload only while it is the
  // section on screen, so it has to be told which one that is
  $effect(() => {
    remember('section', section);
    invoke('viewing', { section }).catch(() => {});
  });

  // Once, and a moment late. Asked during mount it competes with the first
  // paint for the same thread, and this is the window the player is waiting to
  // see; asked here it costs a network round trip nobody is watching. The
  // dashboard is the only window that asks — the overlay, the ticker and the
  // pillar are the same process and would each ask again for nothing.
  $effect(() => {
    const at = setTimeout(lookForUpdate, 2500);
    return () => clearTimeout(at);
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
  // the path to paste into setcap: guessing it is the user's job otherwise
  let binary = $state('');
  // ...except from an AppImage, where that line does not help and does harm
  let appimage = $state(false);
  $effect(() => {
    invoke('about')
      .then((a) => {
        binary = a?.binary ?? '';
        appimage = a?.appimage ?? false;
      })
      .catch(() => {});
  });
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
        title: t('Npcap is not installed'),
        detail:
          t('It is the driver that lets the app read the game’s traffic. Without it nothing can be counted. Get it from npcap.com — its defaults are right.'),
      };
    if (status === 'no-access')
      return {
        bad: true,
        title: t('Npcap is installed, but this app may not use it'),
        detail:
          t('Npcap has an option called “Restrict Npcap driver’s access to Administrators only”. With it on, only an elevated program can read traffic. Either run HS Tracker as administrator, or reinstall Npcap from npcap.com with that box unticked.'),
      };
    if (status === 'no-capture' && appimage)
      return {
        bad: true,
        title: t('An AppImage cannot be given the capture right'),
        detail:
          t('There is nothing to grant it to. Linux drops capabilities at every exec, so a right given to the .AppImage file is gone before the app starts; given to the binary inside instead, the loader stops trusting the library path the bundle needs and the app will not start at all. Install the .deb or the .rpm — either grants the right during installation. Running this AppImage with sudo also works, but then settings and runs are written to root’s home rather than yours.'),
      };
    if (status === 'no-capture')
      return {
        bad: true,
        title: t('Not allowed to read network traffic'),
        detail:
          t('The binary needs the capture right. A packaged install is meant to grant it — if this is a packaged install, the grant did not take. It can be given by hand:'),
        fix: `sudo setcap cap_net_raw=ep ${binary || '<the hs-tracker binary>'}`,
      };
    if (status === 'no-interface')
      return { bad: true, title: t('No network interface to listen on'), detail: t('No adapter could be opened for capture.') };
    if (status === 'waiting-for-game')
      return { bad: false, title: t('Waiting for Hero Siege'), detail: t('Counting starts a moment after the game is running.') };
    if (status.startsWith('capturing')) {
      const [, iface, hosts, , packets, deaf, local] = status.split('|');

      // The game is up, the adapters are open, and the game holds no connection
      // to anywhere but this machine.
      //
      // Local Mode is exactly this and is the first thing to say: it plays
      // without a server, everything it would otherwise send stays inside the
      // process, and there is nothing on the wire for any capture to read. Not
      // a fault, and no setting would change it — which is why this line is
      // not red. The old wording called it a fault and named a Flatpak first,
      // and a player in a local game was told to go hunting for a VPN.
      if (Number(hosts) === 0 && snap.session_secs > 60)
        return {
          bad: false,
          title: t('Nothing to count in a local game'),
          detail:
            Number(local) > 0
              ? t(
                  'The game is connected to this machine and to nowhere else, which is what Local Mode looks like from here. Gold, experience and finds are read out of what the game tells its server, and a local game tells one nothing. Join an online server and counting starts on its own.',
                )
              : say(
                  'Capturing on {iface}. The game is running and holds no connection at all — Local Mode looks exactly like this, and there is nothing on the wire to read. If you are playing online, a VPN or a second network adapter can carry its traffic somewhere we are not listening.',
                  { iface },
                ) +
                (onLinux
                  ? ' ' + t('A Flatpak or Snap install of Steam hides the game from us as well.')
                  : ''),
        };
      // Ninety seconds with the game up and not one message decoded. The
      // backend decides when that is true, because it is the only side that
      // knows when each capture started; `deaf` is 1 while only the game's own
      // connections are being read and 2 when everything already is.
      // Everything that arrived was encrypted. Not a fault, and no setting
      // would change it: the game talks to the account service over TLS until
      // it joins a server.
      if (deaf === '3')
        return {
          bad: false,
          title: t('Waiting for the game to join a server'),
          detail:
            say(
              'Its traffic is being read on {iface}, and all of it so far is encrypted — which is what a character screen or a menu looks like from here. Counting starts when the game connects to a game server.',
              { iface },
            ),
        };
      if (deaf === '1')
        return {
          bad: true,
          title: t('Nothing has been counted for 90 seconds'),
          detail:
            say(
              "Capturing on {iface}: {n} packets read, none of them a game message. If you use a route optimiser — ExitLag and its kind — it redirects the game's packets, so the connections Windows reports are not the ones on the wire. Reading every connection is the way past that.",
              { iface, n: Number(packets).toLocaleString(locale()) },
            ),
          act: { label: t('Read every connection'), on: true },
        };
      if (deaf === '2')
        return {
          bad: true,
          title: t('Nothing has been counted for 90 seconds'),
          detail:
            say(
              "Every connection on this machine is already being read — {n} packets on {iface} — and none of it decoded as Hero Siege. Whatever carries the game's traffic here is not something this app can read from outside. The packet log in Settings and hs-tracker.log are what to send.",
              { n: Number(packets).toLocaleString(locale()), iface },
            ),
        };
      // The game's connections are known and not one frame of them is
      // arriving. This is the state that gets reported as "nothing works and
      // there is no error": the line above is green, every number is zero, and
      // until now nothing distinguished it from a quiet minute.
      if (Number(hosts) > 0 && Number(packets) === 0 && snap.session_secs > 90)
        return {
          bad: true,
          title: t('The game’s connections are known, but nothing is arriving'),
          detail:
            say(
              'Capturing on {iface}, and not one packet has come past the filter. The game’s traffic is taking a route this adapter cannot see — a VPN or split tunnel is the usual reason, and a second network adapter the next. Turning the VPN off for a minute is the quickest way to tell.',
              { iface },
            ),
        };
      // hosts found, frames arriving, but the game has never once reported the
      // character
      if (snap.save_age_secs == null && snap.bank_age_secs == null && snap.session_secs > 240)
        return {
          bad: false,
          title: t('Connected, still nothing from the game'),
          detail:
            say(
              'Its traffic is being read{sofar}, but no character save has arrived yet. Gold, experience and kills travel only when the game saves; if this stays after a few minutes of fighting, the packet log in Settings is worth switching on.',
              {
                sofar:
                  Number(packets) > 0
                    ? say(' — {n} packets so far', { n: Number(packets).toLocaleString(locale()) })
                    : '',
              },
            ),
        };
    }
    return null;
  });
</script>

<div
  class="panel window"
  class:scenic={art('backdrop')}
  style:--backdrop={css('backdrop')}
  style:border-image-source={css('panel')}
  style:--btn={css('button')}
  style:--btn-hover={css('button_hover')}
  style:--btn-down={css('button_down')}
  data-tauri-drag-region
>
  <button
    class="min"
    onclick={() => appWindow().minimize()}
    title={t("Minimize to the taskbar")}
    aria-label={t("minimize")}
  >
    <img src={art('minimize')} alt="" class="min-normal" />
    <img src={art('minimize_hover')} alt="" class="min-hover" />
  </button>

  <button class="close" onclick={() => invoke('hide_dashboard')} title={t("Close to tray")} aria-label={t("close")}>
    <img src={art('close')} alt="" class="close-normal" />
    <img src={art('close_hover')} alt="" class="close-hover" />
  </button>

  <div class="title" style:background-image={css('header')} data-tauri-drag-region>
    <span>HS Tracker</span>
  </div>

  <div class="body">
    <nav class="nav" data-tauri-drag-region>
      {#each SECTIONS as s}
        <button class="tab" class:on={s.id === section} onclick={() => (section = s.id)}>{t(s.label)}</button>
      {/each}

      <div class="spacer"></div>

      {#if overlay}
        <button
          class="btn"
          onclick={() => invoke('compact_mode')}
          title={t("Shrink to the overlay that sits on top of the game")}
        > {t("Compact mode")} </button>
      {/if}
    </nav>

    <div class="pane" style:border-image-source={css('chip_dark')}>
      <!-- A newer version, and the two answers to it. It wears `.trouble`
           without `.bad` because it is the same shape of news as "waiting for
           the game" — something to know about, not something broken — and that
           way a season and the flat skin both already know how to draw it. -->
      {#if updater.found}
        <div class="trouble newer">
          <div class="tt">Version {updater.found.version} is out</div>
          {#if updater.found.notes}<div class="td notes">{updater.found.notes}</div>{/if}
          <div class="nbtns">
            <button class="tact" onclick={installUpdate} disabled={updater.stage === 'downloading'}>
              {updater.stage === 'downloading'
                ? `Downloading… ${Math.round(updater.progress * 100)}%`
                : 'Install and restart'}
            </button>
            <button class="tact" onclick={dismissUpdate} disabled={updater.stage === 'downloading'}>
              Not now
            </button>
          </div>
          {#if updater.failure}
            <div class="td nfail">
              {updater.failure} — the release page in About has the installer.
            </div>
          {/if}
        </div>
      {/if}
      {#if trouble}
        <div class="trouble" class:bad={trouble.bad}>
          <div class="tt">{trouble.title}</div>
          <div class="td">{trouble.detail}</div>
          {#if trouble.fix}<code class="tf">{trouble.fix}</code>{/if}
          {#if trouble.act}
            <button
              class="tact"
              onclick={() => invoke('set_wide_capture', { on: trouble.act.on }).catch(() => {})}
            >
              {trouble.act.label}
            </button>
          {/if}
        </div>
      {/if}
      <!-- One section that throws must not take the window with it.
           Without this the sidebar, Compact mode and the close button go down
           with whatever panel failed, and the window is transparent, so what is
           left on screen is nothing at all. -->
      <div class="content">
        <svelte:boundary onerror={(e) => invoke('report', { level: 'error', message: `${section}: ${e?.stack ?? e}` }).catch(() => {})}>
          <Current />
          {#snippet failed(error, reset)}
            <div class="broke">
              <div class="tt">{t("This panel stopped working.")}</div>
              <div class="td">{error?.message ?? error}</div>
              <button onclick={reset}>{t("Try again")}</button>
            </div>
          {/snippet}
        </svelte:boundary>
      </div>
    </div>
  </div>

  {#each ['n', 's', 'e', 'w', 'ne', 'nw', 'se', 'sw'] as edge}
    <div
      class="grip {edge}"
      role="presentation"
      onmousedown={(e) => e.button === 0 && appWindow().startResizeDragging(DIRECTIONS[edge])}
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
    font-family: var(--face);
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

  /* The section had no size of its own, so it took the height of its content
     and the pane clipped whatever did not fit — with no way to scroll to it.
     Only visible while the "waiting for the game" banner is up, because that is
     the one thing that ever pushed a page past the bottom. It takes what the
     banner leaves and scrolls inside itself; min-height is what lets a flex
     child be shorter than its content instead of overflowing. */
  .content {
    flex: 1 1 auto;
    min-height: 0;
    overflow: auto;
  }

  /* Above whatever section is open, because it explains all of them at once.
     Amber for something to wait out, crimson for something to go and fix. */
  /* A panel that threw. It wears the same clothes as `.trouble` because it is
     the same kind of news, and it keeps the window's own chrome alive around
     it — which is the whole point of the boundary it renders inside. */
  .broke {
    margin: 10px;
    padding: 8px 10px;
    border-left: 3px solid #ca1717;
    background: rgba(var(--pick-rgb), 0.18);
    font-family: var(--face);
  }
  .broke .tt { font-size: 13px; color: #ff7a7a; }
  .broke .td {
    font-size: 11px;
    color: var(--bone-7);
    line-height: 1.45;
    margin: 2px 0 8px;
    font-family: ui-monospace, Consolas, monospace;
  }
  .broke button {
    font: inherit;
    font-size: 11px;
    padding: 4px 12px;
    cursor: pointer;
    color: var(--bone-11);
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid rgba(255, 255, 255, 0.18);
  }

  .trouble {
    flex: none;
    margin-bottom: 6px;
    padding: 6px 10px;
    border-left: 3px solid #8a7a4a;
    background: rgba(120, 96, 40, 0.16);
    font-family: var(--face);
  }
  .trouble.bad {
    border-left-color: #ca1717;
    background: rgba(var(--pick-rgb), 0.18);
  }
  .trouble .tt { font-size: 13px; color: var(--gold-2); }
  .trouble.bad .tt { color: #ff7a7a; }
  .trouble .td { font-size: 11px; color: var(--bone-7); line-height: 1.45; margin-top: 2px; }
  /* The banner is where the reader already is when the app is not working;
     making them find the same switch in Settings is one step too many. */
  .trouble .tact {
    font: inherit;
    font-size: 11px;
    margin-top: 6px;
    padding: 4px 12px;
    cursor: pointer;
    color: var(--bone-11);
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid rgba(255, 255, 255, 0.18);
  }
  .trouble .tact:hover { background: rgba(255, 255, 255, 0.12); }
  /* A download in flight: both buttons are out until it lands, because the
     second click on "Install" would start a second one and "Not now" no
     longer answers anything. */
  .trouble .tact:disabled {
    opacity: 0.55;
    cursor: default;
    background: rgba(255, 255, 255, 0.06);
  }

  /* The release notes, which are a changelog section and can run to a screen
     of bullets. Given a ceiling and its own scroll: the banner sits above the
     page the reader came for, and it is an offer, not the news. */
  .trouble .notes {
    max-height: 84px;
    overflow-y: auto;
    white-space: pre-wrap;
    padding-right: 6px;
  }
  .nbtns { display: flex; gap: 6px; }
  .trouble .nfail { color: #ff7a7a; }

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

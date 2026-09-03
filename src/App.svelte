<script>
  import { say, t } from './say.svelte.js';
  import { invoke } from './bridge.js';
  import { art, css } from './skin.svelte.js';
  import { listen, native } from './bridge.js';
  import { buffInfo, defaultBuffIcon, zoneAct, zoneName, icon } from './buffs.js';
  import { RARITIES, soundUrl, play } from './audio.js';
  import { fmt } from './format.js';

  let snap = $state(null);

  let cfg = $state(null);
  let locked = $derived(cfg?.locked ?? false);
  /// Whether the cursor is over the strip down the right-hand edge, as the
  /// backend sees it.
  ///
  /// The only source: a locked overlay is sent no mouse events, so :hover can
  /// arrive and never leave, and an unlocked one has no way of knowing what
  /// happened while it was locked. One report, both states.
  let nearStrip = $state(false);
  let drag = $derived(cfg?.locked ? null : '');
  const urls = {};
  const lastPlayed = {};

  async function initSounds() {
    cfg = await invoke('get_settings').catch(() => null);
    for (const rarity of RARITIES) urls[rarity] = await soundUrl(rarity);
  }

  // a list brings its own sound and its own volume; everything else is one of
  // the built-in alerts. Lists live inside the active filter — the loose
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

  // Whether the mail is news rather than a standing fact. Paired with has_mail
  // in the markup rather than cleared by an effect, so collecting the mail stops
  // the blink without this ever having to watch the snapshot.
  let mailFresh = $state(false);
  let mailTimer;

  // The satanic zone has just moved. Held for a few seconds and then dropped:
  // the chip has to be noticed from the corner of an eye during a fight, and
  // anything still moving after that is something the eye learns to ignore.
  let zoneMoved = $state(false);
  let zoneTimer;
  const ZONE_ALERT_MS = 4000;

  $effect(() => {
    initSounds();
    invoke('snapshot').then(received).catch(() => {});
    const unsubs = [
      listen('stats', (e) => received(e.payload)),
      // this window is the one that plays sounds, hidden or not — the backend
      // says when mail arrives rather than leaving it to be spotted in a
      // snapshot that only travels to windows on screen
      listen('mail', () => {
        playSound('mail');
        // The chime is easy to miss with the game's own sound up, and the chip
        // said "Mail!" in the same bone as every other figure on the panel. It
        // blinks while the news is new and then settles to a gold that stays
        // until the mail is collected — mail waits as long as you leave it, and
        // a blink that never stops is a blink you learn to ignore.
        mailFresh = true;
        clearTimeout(mailTimer);
        mailTimer = setTimeout(() => (mailFresh = false), 20000);
      }),
      listen('item-drop', (e) => playSound(...(Array.isArray(e.payload) ? e.payload : [e.payload]))),
      // The rotation, from the backend, which fires only for a real one — not
      // for the zone this app has just learned about, and not for one whose
      // buffs the player did not ask about. That filtering lives there rather
      // than here because the pillar asks the same question, and one rule
      // written in two languages comes to disagree with itself.
      //
      // Chime and pulse share a switch, being two halves of one alert. The
      // pillar has its own: being shown and being told are different wants.
      listen('zone-changed', () => {
        if (cfg?.zone?.enabled === false) return;
        playSound('zone');
        zoneMoved = true;
        clearTimeout(zoneTimer);
        zoneTimer = setTimeout(() => (zoneMoved = false), ZONE_ALERT_MS);
      }),
      listen('settings-changed', (e) => (cfg = e.payload)),
      listen('strip-hover', (e) => {
        nearStrip = !!e.payload;
        // Walking away disarms Reset. On an icon the armed state is a blink
        // rather than a sentence, and a blink the cursor has left is a control
        // still holding a click nobody remembers giving it.
        if (!nearStrip) disarm();
      }),
      listen('sounds-changed', async (e) => (urls[e.payload] = await soundUrl(e.payload))),
    ];
    const timer = setInterval(() => (tick = Date.now()), 1000);
    return () => {
      clearInterval(timer);
      clearTimeout(mailTimer);
      clearTimeout(zoneTimer);
      unsubs.forEach((u) => u.then((f) => f()));
    };
  });


  function dur(secs) {
    const h = Math.floor(secs / 3600);
    const m = Math.floor((secs % 3600) / 60);
    const s = secs % 60;
    return `${h}:${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`;
  }

  const item = (name) => snap?.items?.[name] ?? { total: 0, mf: 0, per_hour: 0 };

  /// Bosses put down this session.
  ///
  /// The backend counts each one by name — Statistics lists them that way, and
  /// so does a finished run — but a chip 124px wide has room for a figure, not
  /// a table. Grouped exactly as `stats::tallies` groups them, so this figure
  /// and that list can never disagree.
  const tallied = (group) =>
    (snap?.tallies ?? []).reduce((n, t) => (t.group === group ? n + t.total : n), 0);

  let buffs = $derived(
    Array.from({ length: 3 }, (_, i) => {
      const id = snap?.satanic_zone?.buffs?.[i];
      return id == null ? null : buffInfo(id);
    })
  );

  // The game says outright when the character is standing in the satanic zone,
  // and that is what colours the name here. The flag rides the game's own state
  // packet, which since the August 2026 patch arrives rarely, so it is held
  // against the act — which every save states. Walking into another act clears
  // the colour without waiting for a heartbeat.
  let satanicHere = $derived(
    Boolean(snap?.satanic_here && snap?.act && zoneAct(snap?.satanic_zone?.zone) === snap.act)
  );

  // The window is only ever as tall as the panel inside it. Measuring here and
  // telling the backend keeps the two in step whatever rows are switched on, so
  // adding a row needs nothing done anywhere else.
  //
  // `$state`, not a plain `let`: `bind:this` writes it and an effect reads it,
  // and a plain binding would work only by the order the two happen to run in —
  // behind an `{#if}` the overlay would silently stop resizing.
  let panelEl = $state(null);
  $effect(() => {
    if (!panelEl) return;
    const report = () => {
      const { width, height } = panelEl.getBoundingClientRect();
      // The width goes with it. Held as a constant on both sides, a machine
      // whose text comes out wider spills the chips — fixed widths, no wrapping
      // — over the row instead of the panel giving way. See "Squished Panel".
      if (height > 0) invoke('fit_overlay', { height, width }).catch(() => {});
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
      const loss = Number(dropped) > 0 ? say(', {n} packets dropped', { n: dropped }) : '';
      return {
        cls: Number(dropped) > 0 ? 'warn' : 'ok',
        tip: say('Capturing: {iface} ({hosts} hosts{loss})', { iface, hosts, loss }),
      };
    }
    if (s === 'waiting-for-game') return { cls: 'warn', tip: t('Waiting for Hero Siege to start') };
    if (s === 'npcap-missing') return { cls: 'err', tip: t('Npcap is not installed — https://npcap.com') };
    if (s === 'no-access')
      return { cls: 'err', tip: t('Npcap will not let this app read traffic — run as administrator, or reinstall it without “Restrict to Administrators”') };
    // elsewhere libpcap is always there; what is missing is the right to use it
    if (s === 'no-capture') return { cls: 'err', tip: t('No capture device — the binary needs cap_net_raw') };
    return { cls: 'err', tip: t('No suitable network interface') };
  });

  const shown = (id) => !(cfg?.hidden ?? []).includes(id);

  // pinned over a running game: drop the frame, leave the numbers floating on
  // top of the game
  let live = $derived((snap?.status ?? '').startsWith('capturing'));
  // Ghost mode drops the frame so the numbers float over the game. It needs the
  // window to clear itself between frames, which this desktop does not — see
  // main.js — so the setting defaults off there and says why in Settings.
  let ghost = $derived(locked && live && (cfg?.ghost ?? true));

  // Anything that throws work away asks once; the second click does it — but
  // not the second half of a double-click. This is an ARPG: the left button
  // moves and attacks, players click-spam, and two clicks 80ms apart are what
  // the input device produces by itself. The arming blink is a 0.6s square wave
  // and had not completed a cycle before the confirming click landed.
  let armed = $state(null);
  let armedAt = 0;
  let armTimer;
  const ARM_SETTLE_MS = 350;
  function danger(key, action) {
    if (armed === key) {
      if (Date.now() - armedAt < ARM_SETTLE_MS) return;
      clearTimeout(armTimer);
      armed = null;
      action();
      return;
    }
    clearTimeout(armTimer);
    armed = key;
    armedAt = Date.now();
    armTimer = setTimeout(() => (armed = null), 4000);
  }

  /// Nothing stays armed across a change of mind. The lock does not go through
  /// `danger`, so without this, arming Reset and then locking leaves the
  /// question standing behind a strip the poller never reports leaving.
  const disarm = () => {
    clearTimeout(armTimer);
    armed = null;
  };

  const reset = () => invoke('reset_stats');

  // The strip, under the lock. These were the right-click menu, which was a menu
  // nobody could find: nothing on the panel ever hinted it was there.
  const ENTRIES = [
    // The titles are the English, not a translation: this array is built once
    // when the script runs — before the language file has been fetched — so a
    // t() in here would freeze at English and never move again. It is called
    // where the title is printed instead.
    { key: 'dashboard', icon: 'dashboard', title: 'Dashboard', run: () => invoke('full_mode') },
    { key: 'hide', icon: 'minimize', title: 'Hide to tray', run: () => invoke('hide_window') },
    { key: 'reset', icon: 'reset', title: 'Reset stats', danger: true, run: reset },
    { key: 'quit', icon: 'close', title: 'Quit', danger: true, run: () => invoke('quit') },
  ];




  async function toggleLock() {
    disarm();
    if (!cfg) cfg = await invoke('get_settings').catch(() => null);
    if (!cfg) return;
    cfg = { ...cfg, locked: !cfg.locked };
    invoke('save_settings', { settings: cfg }).catch(() => {});
  }
</script>

<div
  bind:this={panelEl}
  class="panel overlay"
  class:ghost
  class:held={snap?.paused}
  style:--frost={css('frozen')}
  style:border-image-source={css('panel')}
  style:opacity={cfg?.opacity ?? 1}
  data-tauri-drag-region={drag}
>
  {#if shown('session')}
    <div class="row" data-tauri-drag-region={drag}>
      <div class="chip lg" style:border-image-source={css('chip_dark')} title={status.tip}>
        <span class="dot {status.cls}"></span>
        <img src={snap?.paused ? art('frozen_icon') : icon('time')} alt="" class="ic" />
        <span class="val" class:frozen={snap?.paused}>{snap ? dur(sessionSecs) : '0:00:00'}</span>
      </div>
      <div
        class="chip md mail"
        class:has={snap?.has_mail}
        class:fresh={mailFresh && snap?.has_mail}
        style:border-image-source={css('chip_dark')}
        title={snap?.has_mail ? t('there is mail waiting') : t('no mail')}
      >
        <img src={icon(snap?.has_mail ? 'mail_1' : 'mail_0')} alt="" class="ic" />
        <span class="val">{snap?.has_mail ? t('Mail!') : t('No mail')}</span>
      </div>
      <!-- The third cell of the session row, which stood empty from the day the
           magic-find figure came out of it: every row on this panel is
           140 + 124 + 124 and the comment on `.chip` keeps those boundaries
           down the whole panel, so a row of two ended in a gap.

           The skull is the game's own: `Mapscreen_Skull_spr` is what its map
           screen puts over a boss dungeon, so it already reads as "boss" to
           anyone who has opened that screen — and hs-map marks the same
           dungeons with the same sprite. Exported by tools/export_ui.py rather
           than dropped in by hand, so a season that redraws it can regenerate
           it. The chests are counted too and are on the Statistics tab; one
           number is what fits here and the bosses are the one worth watching. -->
      <div
        class="chip md"
        style:border-image-source={css('chip_dark')}
        title={t("bosses put down this session")}
      >
        <img src={icon('boss')} alt="" class="ic" />
        <span class="val">{fmt(tallied('boss'))}</span>
      </div>
    </div>
  {/if}

  {#if shown('items')}
    <div class="row" data-tauri-drag-region={drag}>
      <div class="chip lg" style:border-image-source={css('chip_dark')} title={t("Angelic | Unholy")}>
        <img src={icon('chest')} alt="" class="ic" />
        <span class="val">
          <span class="c-ang">{fmt(item('Angelic').total)}</span>
          | <span class="c-unh">{fmt(item('Unholy').total)}</span>
        </span>
      </div>
      <div class="chip md" style:border-image-source={css('chip_dark')} title={t("Satanic | per hour")}>
        <span class="val">
          <span class="c-sat">{fmt(item('Satanic').total)}</span>
          | <span class="c-sat">{fmt(item('Satanic').per_hour)}{t('/h')}</span>
        </span>
      </div>
      <div class="chip md" style:border-image-source={css('chip_dark')} title={t("Heroic | Set")}>
        <span class="val">
          <span class="c-her">{fmt(item('Heroic').total)}</span>
          | <span class="c-set">{fmt(item('Set').total)}</span>
        </span>
      </div>
    </div>
  {/if}

  {#if shown('gold')}
    <div class="row" data-tauri-drag-region={drag}>
      <div class="chip lg" style:border-image-source={css('chip_dark')} title={t("gold earned this session")}>
        <span class="coin" class:idle={!live} style:background-image={css('coin_strip')}></span>
        <span class="val">+{fmt(snap?.gold?.earned)}</span>
      </div>
      <div class="chip md" style:border-image-source={css('chip_dark')} title={t("gold per hour")}>
        <span class="val">{fmt(snap?.gold?.per_hour)}{t('/h')}</span>
      </div>
      <!-- Kills is a statistic, not a stand-in for the button. Rendering it
           only when the Reset button is off or the overlay is ghosted makes the
           panel's one combat figure something you see by accident. -->
      <div class="chip md" style:border-image-source={css('chip_dark')}>
        <span class="dot {status.cls}"></span>
        <span class="val">{fmt(snap?.kills?.earned)} {t('kills')}</span>
      </div>
    </div>
  {/if}

  {#if shown('xp')}
    <div class="row" data-tauri-drag-region={drag}>
      <div class="chip lg" style:border-image-source={css('chip_dark')} title={t("experience earned this session")}>
        <img src={icon('xp')} alt="" class="ic" />
        <span class="val">+{fmt(snap?.xp?.earned)}</span>
      </div>
      <div class="chip md" style:border-image-source={css('chip_dark')} title={t("experience per hour")}>
        <span class="val">{fmt(snap?.xp?.per_hour)}/h</span>
      </div>
      <!-- The cell that ends the row. The Reset button was the only chip that
           came and went — ghost mode draws none — so the panel finished on a gap
           exactly when it was pinned over the game.

           SS is the top tier, and the number a run is judged by whatever colour
           the drops came out in. The backend has counted every tier all along;
           this is the one worth a chip. The label is written out rather than
           left to a tooltip: this panel is what a capture card records, and
           nobody hovers a video. -->
      <div
        class="chip md"
        style:border-image-source={css('chip_dark')}
        title={t("SS drops this session — the top tier, counted whatever the rarity")}
      >
        <span class="grade">SS</span>
        <span class="val">{fmt(snap?.ss)}</span>
      </div>
    </div>
  {/if}

  {#if shown('zone')}
    <div class="row" data-tauri-drag-region={drag}>
      <div class="chip lg buffs" style:border-image-source={css('chip_dark')}>
        {#each buffs as b}
          <img
            class="buff"
            src={b ? b.icon : defaultBuffIcon}
            alt=""
            title={b ? `${b.name} : ${b.desc}` : t('Satanic Zone')}
          />
        {/each}
      </div>
      <div
        class="zone"
        class:moved={zoneMoved}
        style:background-image={css('header')}
        data-tauri-drag-region={drag}
      >
        <span class="zone-name" class:here={satanicHere}>
          {snap?.satanic_zone ? zoneName(snap.satanic_zone.zone) : '—'}
        </span>
      </div>
    </div>
  {/if}

</div>

<!--
  The controls, in the twenty pixels between the last chip and the window's edge.
  The chip grid ends at x=424 and the window is 444 wide; everything in that band
  is the panel's border art and its padding, so a column here covers no figure.

  A sibling of the panel rather than a child, and fixed rather than absolute, for
  three reasons that each stand on their own: an absolutely positioned child is
  laid out against the panel's PADDING box, so its numbers and the backend's rect
  would be measured from different origins; a child inherits the panel's opacity
  slider, and a control faded to 0.3 is a control you cannot aim at; and a child
  of .panel.ghost loses the frame art with it.
-->
{#if native}
  <div class="strip" class:near={nearStrip} class:free={!locked}>
    <button
      class="cell lock"
      style:--i={css(locked ? 'lock_gold' : 'lock_pale')}
      onclick={toggleLock}
      title={locked
        ? t('Locked — click to unlock (Ctrl+Shift+L)')
        : t('Click to lock: the overlay becomes click-through except this strip (Ctrl+Shift+L)')}
      aria-label={locked ? t('unlock the overlay') : t('lock the overlay')}
    >
    </button>
    {#each ENTRIES as e}
      <button
        class="cell"
        class:armed={armed === e.key}
        style:--i={css(e.icon)}
        style:--i-hover={css(`${e.icon}_hover`)}
        onclick={() => (e.danger ? danger(e.key, e.run) : e.run())}
        title={armed === e.key ? `${t(e.title)}${t(' — click again')}` : t(e.title)}
        aria-label={t(e.title)}
      >
      </button>
    {/each}
  </div>
{/if}

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
    /* A floor, not a size. The chips below set the rhythm; if a machine draws
       them wider than this, the panel widens and the window follows it. */
    min-width: 444px;
    width: max-content;
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
    /* Packed from the left, not spread. 140 + 8 + 124 + 8 + 124 is 404, which is
       the content box exactly, so a full row looks the same either way — but a
       row that is one cell short does not. `space-between` sent the survivors to
       the two edges and moved every column boundary with them; ghost mode drops
       the Reset button, and the session row has stood two chips wide since the
       magic-find figure came out, so short rows are ordinary here rather than
       exotic. The gap goes at the end, where it reads as the panel running out
       rather than as a hole. */
    justify-content: flex-start;
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

  /* Every row is 140 + 124 + 124 with two 8px gaps: 388px, and the same column
     boundaries down the whole panel. The loot row was briefly 124 × 124 × 124
     after its bracketed figures came out — the same total, so nothing
     overflowed, but its first boundary sat 16px left of every other row's and
     the panel read as crooked. The widths are the grid, not the content. */
  .chip.lg { min-width: 140px; }
  .chip.md { min-width: 124px; }

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

  /* The grade, set the way the game sets it: full size and gold, with the count
     beside it in ordinary bone — the game's own tooltip reads "Tier SS, Requires
     Level 100" with exactly that split. Written out rather than left to a
     tooltip because this panel is what a capture card records, and nobody
     hovers a video. Gold measures 10:1 on the plate. */
  .grade { color: var(--gold-2); letter-spacing: 1px; }

  /* Mail. Blinking while the news is new, gold for as long as it waits. The
     blink is a square wave rather than a fade because everything else in this
     skin is pixel art, and a filter carries the plate and the glyphs together. */
  .chip.mail.has .val { color: var(--gold-2); }
  .chip.mail.fresh { animation: mail-blink 0.8s infinite; }
  @keyframes mail-blink {
    0%, 49% { filter: none; }
    50%, 100% { filter: brightness(1.6); }
  }
  @media (prefers-reduced-motion: reduce) {
    .chip.mail.fresh { animation: none; filter: brightness(1.35); }
  }

  /* The strip stands BESIDE the panel, not on it. The window is 472 wide where
     the panel is 444, and this column occupies the 28 past the panel's right
     edge — a webview cannot paint outside its own window, so "beside the panel"
     had to become "a wider window".

     28 is the height of a chip, so the column keeps the panel's rhythm and reads
     as part of it rather than as something parked next to it. Twice the plates'
     native 21 would have been exactly crisp and was far too big. Five cells with
     1px between them and 3 more under the lock is 147 tall; STRIP_H in lib.rs is
     the same arithmetic and gives the window its floor. */
  .strip {
    position: fixed;
    top: 0;
    right: 0;
    width: 28px;
    display: flex;
    flex-direction: column;
    gap: 1px;
    z-index: 2;
    opacity: 0;
    pointer-events: none;
    /* Only on the way out. `pointer-events` cannot be animated, so it snaps to
       `auto` the instant the strip is revealed while the opacity is still
       climbing: for 120ms five buttons were live and half invisible, which is
       how a click lands on something the eye has not seen yet. */
    transition: opacity 0.12s;
  }
  .strip.free,
  .strip.near { transition: none; }
  /* Unlocked, the strip simply is there — dimmed, always clickable. It can
     afford to be, because it covers nothing. Locked, it waits for the backend to
     say the cursor has arrived: a click-through window is sent no mouse events,
     so :hover would never fire and could never end. */
  .strip.free { opacity: 0.55; pointer-events: auto; }
  .strip.near { opacity: 1; pointer-events: auto; }

  .cell {
    width: 28px;
    height: 28px;
    flex: none;
    padding: 0;
    border: none;
    cursor: pointer;
    background: var(--i) no-repeat center / 100% 100%;
    image-rendering: pixelated;
  }
  .cell:hover { background-image: var(--i-hover, var(--i)); }
  /* The lock is a toggle rather than an action, and its sprite is 33x48 rather
     than square: `contain` keeps it upright in the same 28 box instead of
     stretching it, and the wider gap under it is what separates a toggle from
     the four things that do something. */
  .cell.lock { background-size: contain; margin-bottom: 3px; }

  /* Armed: the second click does it. A square wave rather than a fade, for the
     same reason the mail chip blinks that way. */
  .cell.armed {
    background-image: var(--i-hover, var(--i));
    animation: cell-armed 0.6s infinite;
  }
  @keyframes cell-armed {
    0%, 49% { filter: none; }
    50%, 100% { filter: brightness(1.5) saturate(1.4); }
  }
  @media (prefers-reduced-motion: reduce) {
    .cell.armed { animation: none; filter: brightness(1.35); }
  }

  /* See main.js. The panel's art is pixel art with soft edges, so the text
     and its shadow sit partly on transparency — which on this desktop never
     clears, and every changed value leaves its last few frames behind as a
     dark smear. A solid colour behind the art replaces those pixels instead
     of blending with them. Ghost mode is the one place this must not apply:
     dropping the frame is the whole point of it, and painting a slab behind
     the numbers instead is not a frameless overlay, it is a darker one. Its
     smearing is what the switch in Settings is for. */
  :global(html[data-os='linux']) .panel:not(.ghost) { background-color: var(--ground-3); }
  :global(html[data-os='linux']) .panel:not(.ghost) .chip { background-color: var(--ground-5); }

  .dot { width: 7px; height: 7px; border-radius: 50%; flex: none; }
  .dot.ok { background: #4caf50; }
  .dot.warn { background: #e0b040; }
  .dot.err { background: #d04040; }

  .chip.buffs { gap: 10px; justify-content: center; }
  .buff { width: 21px; height: 21px; }

  .zone {
    box-sizing: border-box;
    /* the frame the rotation sweep is painted in — on the plate always, not
       only while it is sweeping, so the class that starts it changes nothing
       but what is drawn */
    position: relative;
    overflow: hidden;
    /* 140 + 240 is 380 in a 404 box, so this plate needs the 24px slack pushed
       in front of it to keep its right edge on the same boundary as the chips
       above. `space-between` on the row would do it, but the short rows above
       cannot afford that. */
    margin-left: auto;
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
  /* The zone has just moved. A sweep of light across the plate and a pulse on
     the name: drawn in the plate's own ::after, which is out of the flow and
     takes no clicks, so the row cannot reflow and the panel cannot start
     catching a click meant for the game underneath. It runs for as long as
     App.svelte holds the class and then the chip is an ordinary chip again. */
  .zone.moved::after {
    content: '';
    position: absolute;
    inset: 0;
    pointer-events: none;
    background: linear-gradient(
      100deg,
      transparent 30%,
      rgba(255, 255, 255, 0.35) 50%,
      transparent 70%
    );
    animation: zone-sweep 1.1s ease-out infinite;
  }
  /* The rotation, which is news for four seconds. Deliberately not
     `--rar-satanic`: that colour means "the room you are in is the satanic
     one", and wearing it here said so for four seconds every rotation — and
     outranked the real thing while it did. */
  .zone.moved .zone-name {
    color: #ffb08a;
    animation: zone-pulse 0.55s infinite;
  }
  @keyframes zone-sweep {
    from { transform: translateX(-100%); }
    to { transform: translateX(100%); }
  }
  /* A square wave, like the mail chip and the armed buttons: this skin is pixel
     art, and a filter carries the plate and the glyphs together. */
  @keyframes zone-pulse {
    0%, 49% { filter: none; }
    50%, 100% { filter: brightness(1.7); }
  }
  @media (prefers-reduced-motion: reduce) {
    .zone.moved::after { animation: none; opacity: 0.25; }
    .zone.moved .zone-name { animation: none; filter: brightness(1.35); }
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
  .zone-name.here { color: var(--rar-satanic); }

  .c-sat { color: var(--rar-satanic); }
  .c-set { color: #40d040; }
  .c-unh { color: #e04a7a; }

</style>

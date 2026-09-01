<script>
  import { invoke } from './bridge.js';
  import { art } from './skin.svelte.js';
  import { listen } from './bridge.js';
  import { rarityByName, tierLabel } from './items.js';
  import { itemName, nameOf, t, typeLabel } from './say.svelte.js';

  const TTL_MS = 8000;
  const FADE_MS = 600;
  const MAX_VISIBLE = 5;

  let entries = $state([]);
  // Off until the settings say otherwise. Starting on meant a drop that
  // arrived in the moment before they loaded was shown by a ticker the player
  // had switched off.
  let enabled = $state(false);
  let nowTick = $state(Date.now());
  // Under the overlay the rows stack from the top of this window, which puts
  // the first of them against the panel. Over the overlay that same stacking
  // leaves the window's whole height between the row and the panel, so the
  // list reads as having jumped to the top of the screen. The backend knows
  // which way it hung the window and says so.
  let above = $state(false);
  let nextKey = 0;

  function label(d) {
    if (d.name) return nameOf(d.name, d.item_type, d.item_id, d.weapon_type);
    const known = itemName(d.item_type, d.item_id, d.weapon_type);
    if (known) return known;
    if (d.item_id > 0) return `${typeLabel(d.item_type, d.weapon_type)} #${d.item_id}`;
    return typeLabel(d.item_type, d.weapon_type);
  }

  function rarity(d) {
    if (d.rarity) return d.rarity;
    return rarityByName(d.name || label(d)) ?? 'Drop';
  }

  // the list is empty most of the time; a timer running then would re-render
  // the window five times a second for nothing
  let sweep = null;
  function stopSweep() {
    clearInterval(sweep);
    sweep = null;
    invoke('ticker_busy', { active: false }).catch(() => {});
  }
  function startSweep() {
    if (sweep) return;
    invoke('ticker_busy', { active: true }).catch(() => {});
    sweep = setInterval(() => {
      nowTick = Date.now();
      entries = entries.filter((it) => it.until > nowTick);
      if (!entries.length) stopSweep();
    }, 200);
  }

  $effect(() => {
    invoke('get_settings').then((s) => (enabled = s?.ticker ?? true));
    const unsubs = [
      listen('settings-changed', (e) => (enabled = e.payload?.ticker ?? true)),
      listen('ticker-above', (e) => (above = !!e.payload)),
      listen('drop-entry', (e) => {
        if (!enabled) return;
        const d = e.payload;
        entries = [{ ...d, key: nextKey++, until: Date.now() + TTL_MS }, ...entries].slice(0, MAX_VISIBLE);
        nowTick = Date.now();
        startSweep();
      }),
    ];
    return () => {
      stopSweep();
      unsubs.forEach((u) => u.then((f) => f()));
    };
  });

  const rarityCls = {
    Satanic: 'c-sat',
    Heroic: 'c-her',
    Angelic: 'c-ang',
    Unholy: 'c-unh',
    Mythic: 'c-myt',
    Set: 'c-set',
  };
</script>

<div class="stack" class:above>
  {#each entries as it (it.key)}
    <div class="entry" class:fading={it.until - nowTick < FADE_MS} style:border-image-source="url({art('chip_dark')})">
      <span class="rar {rarityCls[rarity(it)] ?? ''}">{t(rarity(it))}</span>
      <span class="name {rarityCls[rarity(it)] ?? ''}">{label(it)}</span>
      {#if it.tier > 0}<span class="dim">{tierLabel(it.tier)}</span>{/if}
      {#if it.mf}<span class="c-blue">{t('MF')}</span>{/if}
      {#if it.announced}<span class="dim">{t("server")}</span>{/if}
    </div>
  {/each}
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
  }

  .stack {
    display: flex;
    flex-direction: column;
    gap: 4px;
    /* so the rows can be held against whichever edge the overlay is on */
    height: 100vh;
    justify-content: flex-start;
    /* The panel's own inset, not the gap between its chips. Both windows are
       444 CSS px on the same x, so the numbers line up directly: the overlay's
       chip columns run 20…424 (14px border-image + 6px padding), while 8px here
       put these plates at 8…436 — 12px proud on each side, always. It showed
       worst in ghost mode, where the frame art is gone and those chips ARE the
       overlay's edge. See .panel in App.svelte. */
    padding: 0 20px;
    font-family: var(--face);
    font-size: 12px;
    color: var(--bone-6);
  }

  /* hanging over the overlay: the rows sit at the foot of the window, which is
     the edge the panel is on */
  .stack.above { justify-content: flex-end; }

  .entry {
    box-sizing: border-box;
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 26px;
    padding: 0 4px;
    border: 6px solid transparent;
    border-image-slice: 6 fill;
    border-image-width: 6px;
    image-rendering: pixelated;
    white-space: nowrap;
    animation: slide-in 0.18s ease-out;
    transition: opacity 0.5s;
  }
  .entry.fading { opacity: 0; }

  @keyframes slide-in {
    from {
      transform: translateY(-6px);
      opacity: 0;
    }
    to {
      transform: translateY(0);
      opacity: 1;
    }
  }

  .rar { font-size: 10px; text-transform: uppercase; letter-spacing: 0.5px; flex: none; }
  .name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; }
  .dim { color: var(--edge-8); font-size: 10px; flex: none; }

  .c-ang { color: #f6f794; }
  .c-her { color: #00ffae; }
  .c-sat { color: var(--rar-satanic); }
  .c-blue { color: var(--mf); }
  .c-myt { color: #c060e0; }
  .c-unh { color: #e04a7a; }
  .c-set { color: #40d040; }
  .c-ble { color: var(--bone-14); }
</style>

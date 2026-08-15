<script>
  import { invoke } from './bridge.js';
  import { art } from './skin.svelte.js';
  import { listen } from './bridge.js';
  import { itemName, rarityByName, tierLabel, typeLabel } from './items.js';

  const TTL_MS = 8000;
  const FADE_MS = 600;
  const MAX_VISIBLE = 5;

  let entries = $state([]);
  let enabled = $state(true);
  let nowTick = $state(Date.now());
  let nextKey = 0;

  function label(d) {
    if (d.name) return d.name;
    const known = itemName(d.item_type, d.item_id, d.weapon_type);
    if (known) return known;
    if (d.item_id > 0) return `${typeLabel(d.item_type, d.weapon_type)} #${d.item_id}`;
    return typeLabel(d.item_type, d.weapon_type);
  }

  function rarity(d) {
    if (d.rarity) return d.rarity;
    return rarityByName(label(d)) ?? 'Drop';
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

<div class="stack">
  {#each entries as it (it.key)}
    <div class="entry" class:fading={it.until - nowTick < FADE_MS} style:border-image-source="url({art('chip_dark')})">
      <span class="rar {rarityCls[rarity(it)] ?? ''}">{rarity(it)}</span>
      <span class="name {rarityCls[rarity(it)] ?? ''}">{label(it)}</span>
      {#if it.tier > 0}<span class="dim">{tierLabel(it.tier)}</span>{/if}
      {#if it.mf}<span class="c-blue">MF</span>{/if}
      {#if it.announced}<span class="dim">server</span>{/if}
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
    padding: 0 8px;
    font-family: 'CookieRun Bold', sans-serif;
    font-size: 12px;
    color: var(--bone-6);
  }

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
  .c-sat { color: #ca1717; }
  .c-blue { color: #5050ae; }
  .c-myt { color: #c060e0; }
  .c-unh { color: #e04a7a; }
  .c-set { color: #40d040; }
  .c-ble { color: var(--bone-14); }
</style>

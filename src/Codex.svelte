<script>
  // Every named item the datamined tables know, with what it takes to get one.
  //
  // Two numbers matter and they are not the same number: the general chance,
  // which applies wherever you are, and the chance in the places the item is
  // tied to — the one the game prints in green. An item with both is worth
  // farming somewhere in particular; an item with only the first is not.
  import {
    DROP_CHASE,
    DROP_PLACES,
    DROP_RATE,
    DROP_ZONES,
    ITEMS,
    RARITY_BY_NAME,
    TIER_BY_NAME,
    TYPE_NAMES,
    tierLabel,
    typeLabel,
  } from './items.js';
  import { art } from './skin.svelte.js';

  /// Built once: the tables are keyed by lowercase name, and one item can wear
  /// several ids, so the first sighting of a name wins.
  const CATALOGUE = (() => {
    const seen = new Map();
    for (const [key, name] of Object.entries(ITEMS)) {
      if (seen.has(name)) continue;
      const [type, , weapon] = key.split(':').map(Number);
      const k = name.toLowerCase();
      seen.set(name, {
        name,
        key: k,
        type,
        weapon,
        kind: typeLabel(type, weapon),
        group: TYPE_NAMES[type] ?? `Type ${type}`,
        rarity: RARITY_BY_NAME[k] ?? '',
        tier: TIER_BY_NAME[k] ?? 0,
        rate: DROP_RATE[k] ?? 0,
        chase: DROP_CHASE[k] ?? 0,
        places: DROP_PLACES[k] ?? [],
        zones: DROP_ZONES[k] ?? [],
      });
    }
    return [...seen.values()];
  })();

  // taken from the data, like the kinds below it: the tables carry five
  // rarities, and offering ones they do not would only ever find nothing
  const RARITIES = [...new Set(CATALOGUE.map((i) => i.rarity).filter(Boolean))].sort();
  const GROUPS = [...new Set(CATALOGUE.map((i) => i.group))].sort();
  const RARITY_CLASS = {
    Satanic: 'c-sat', Set: 'c-set', Heroic: 'c-her', Angelic: 'c-ang', Unholy: 'c-unh',
  };
  /// how many rows are drawn at once; the rest wait behind a narrower search
  const SHOWN = 150;

  let query = $state('');
  let rarity = $state('');
  let group = $state('');
  let sort = $state('rarest');

  const odds = (rate) =>
    !rate
      ? '—'
      : rate >= 1e6
        ? `1/${(rate / 1e6).toFixed(rate >= 1e7 ? 0 : 1)}M`
        : rate >= 1e3
          ? `1/${(rate / 1e3).toFixed(rate >= 1e4 ? 0 : 1)}k`
          : `1/${rate}`;

  let found = $derived.by(() => {
    const q = query.trim().toLowerCase();
    let list = CATALOGUE.filter(
      (i) =>
        (!q || i.key.includes(q)) &&
        (!rarity || i.rarity === rarity) &&
        (!group || i.group === group),
    );
    if (sort === 'name') {
      list = list.toSorted((a, b) => a.name.localeCompare(b.name));
    } else {
      // rarest first, and anything the tables cannot price goes last
      list = list.toSorted((a, b) => (b.rate || -1) - (a.rate || -1));
    }
    return list;
  });
</script>

<div class="panel">
  <div class="body">
    <div class="tools" data-tauri-drag-region>
      <input class="find" placeholder="Search by name" bind:value={query} />
      <select class="picker" bind:value={rarity}>
        <option value="">Any rarity</option>
        {#each RARITIES as r}<option value={r}>{r}</option>{/each}
      </select>
      <select class="picker" bind:value={group}>
        <option value="">Any kind</option>
        {#each GROUPS as g}<option value={g}>{g}</option>{/each}
      </select>
      <select class="picker narrow" bind:value={sort}>
        <option value="rarest">Rarest first</option>
        <option value="name">By name</option>
      </select>
    </div>

    <div class="box" style:border-image-source="url({art('chip_dark')})">
      <div class="head">
        <span class="hname">Item</span>
        <span class="hkind">Kind</span>
        <span class="hgrade">Grade</span>
        <span class="hrate">Anywhere</span>
        <span class="hchase">Where it is tied</span>
      </div>

      <div class="rows">
        {#each found.slice(0, SHOWN) as it (it.name)}
          <div class="row">
            <span class="name {RARITY_CLASS[it.rarity] ?? ''}" title={it.rarity || 'unlisted'}>{it.name}</span>
            <span class="kind dim">{it.kind}</span>
            <span class="grade dim">{tierLabel(it.tier) || '—'}</span>
            <span class="rate">{odds(it.rate)}</span>
            <span class="chase">
              {#if it.chase}
                <b>{odds(it.chase)}</b>
                <span class="where">{it.places.join(' · ') || it.zones.join(', ')}</span>
              {:else if it.places.length}
                <span class="where">{it.places.join(' · ')}</span>
              {:else}
                <span class="dim">anywhere</span>
              {/if}
            </span>
          </div>
        {:else}
          <div class="empty dim">nothing matches that</div>
        {/each}
      </div>

      <div class="foot dim">
        {#if found.length > SHOWN}
          showing {SHOWN} of {found.length} — narrow the search to see the rest
        {:else}
          {found.length} of {CATALOGUE.length} items
        {/if}
      </div>
    </div>
  </div>
</div>

<style>
  @font-face {
    font-family: 'CookieRun Bold';
    src: url('./assets/fonts/cookierunbold.ttf') format('truetype');
  }

  .panel { height: 100%; }
  .body {
    height: 100%;
    box-sizing: border-box;
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-family: 'CookieRun Bold', sans-serif;
    font-size: 12px;
    color: var(--bone-6);
  }

  .tools { display: flex; gap: 6px; align-items: center; }
  .find {
    flex: 1 1 auto;
    min-width: 0;
    box-sizing: border-box;
    font: inherit;
    font-size: 12px;
    color: var(--bone-13);
    background: rgba(0, 0, 0, 0.35);
    border: 1px solid var(--ground-10);
    padding: 4px 8px;
    height: 24px;
  }
  .find:focus { outline: none; border-color: var(--edge-4); }
  .find::placeholder { color: var(--bone-3); }

  .picker {
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
  .picker:focus { outline: none; border-color: var(--edge-4); }
  .picker.narrow { width: 110px; }
  .picker option { background: var(--ground-7); color: var(--bone-9); }

  .box {
    flex: 1 1 auto;
    min-height: 0;
    box-sizing: border-box;
    border: 6px solid transparent;
    border-image-slice: 6 fill;
    border-image-width: 6px;
    image-rendering: pixelated;
    padding: 6px 8px;
    display: flex;
    flex-direction: column;
  }

  .head, .row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 74px 44px 62px minmax(0, 1.2fr);
    gap: 6px;
    align-items: baseline;
  }
  .head {
    font-size: 10px;
    color: var(--bone-3);
    text-transform: uppercase;
    letter-spacing: 0.06em;
    padding-bottom: 4px;
    border-bottom: 1px solid var(--ground-10);
  }
  .hgrade, .hrate { text-align: right; }

  .rows { flex: 1 1 auto; min-height: 0; overflow-y: auto; padding-top: 2px; }
  .rows::-webkit-scrollbar { width: 6px; }
  .rows::-webkit-scrollbar-thumb { background: var(--dim-1); border-radius: 3px; }

  .row { padding: 2px 0; }
  .row:hover { background: rgba(255, 255, 255, 0.04); }
  .name, .kind, .where { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .grade, .rate { text-align: right; }
  .rate { color: var(--bone-9); }
  .chase { display: flex; gap: 6px; align-items: baseline; min-width: 0; }
  .chase b { color: var(--gold-2); font-weight: normal; }
  .where { color: var(--bone-3); font-size: 11px; }

  .empty { padding: 12px 0; text-align: center; }
  .foot { padding-top: 4px; font-size: 11px; }
  .dim { color: var(--bone-3); }

  .c-sat { color: var(--rar-satanic); }
  .c-set { color: #40d040; }
  .c-her { color: #00ffae; }
  .c-ang { color: #f6f794; }
  .c-unh { color: #e04a7a; }
</style>

<script>
  import { onMount } from 'svelte';
  import { BY_ID, DROP_CHASE, DROP_PLACES, DROP_RATE, ITEMS, RARITY_BY_NAME, TIER_BY_NAME, TYPE_NAMES, tierLabel } from './items.js';
  import { itemName, language, locale, placeLabel, say, t, typeLabel } from './say.svelte.js';
  import { art } from './skin.svelte.js';
  import { invoke, native, recall, remember } from './bridge.js';

  const searchKey = (name) => name.normalize('NFKC').toLowerCase().replace(/[‘’]/g, "'");
  const shown = (it) => itemName(it.type, it.id, it.weapon);
  const kindOf = (it) => typeLabel(it.type, it.weapon);

  const CATALOGUE = (() => {
    const counts = new Map();
    for (const name of Object.values(ITEMS)) {
      const key = name.toLowerCase();
      counts.set(key, (counts.get(key) ?? 0) + 1);
    }
    return Object.entries(ITEMS).map(([identity, name]) => {
      const [type, id, weapon] = identity.split(':').map(Number);
      const key = name.toLowerCase();
      const sharedName = counts.get(key) > 1;
      const exact = BY_ID[identity];
      return {
        identity,
        name,
        search: searchKey(name),
        type,
        id,
        weapon,
        group: TYPE_NAMES[type] ?? `Type ${type}`,
        rarity: exact?.[1] ?? (!sharedName ? RARITY_BY_NAME[key] : '') ?? '',
        tier: exact?.[2] ?? (!sharedName ? TIER_BY_NAME[key] : 0) ?? 0,
        rate: exact?.[3] ?? (!sharedName ? DROP_RATE[key] : 0) ?? 0,
        chase: (!sharedName && DROP_CHASE[key]) || 0,
        locations: (sharedName ? [] : DROP_PLACES[key] ?? []).map((name) => [name, '', false]),
      };
    });
  })();

  let sources = $state.raw({});
  let roomLabels = $state.raw({});
  let loaded = $state(false);
  onMount(() => {
    import('virtual:tracker-item-sources')
      .then(({ default: index, locations }) => { sources = index; roomLabels = locations; })
      .catch(() => {})
      .finally(() => (loaded = true));
    return () => clearTimeout(saidTimer);
  });

  let catalogue = $derived(CATALOGUE.map((item) => {
    const source = sources[item.identity];
    return source ? { ...item, ...source, locations: source.locations ?? item.locations } : item;
  }));
  let rarities = $derived([...new Set(catalogue.map((i) => i.rarity).filter((name) => RARITY_CLASS[name]))].sort());
  // Paired with the numeric type, because the word for a kind is the game's
  // and the English name is only the key the filter matches on.
  const GROUPS = [...new Map(CATALOGUE.map((i) => [i.group, i.type])).entries()];
  let groupChoices = $derived(
    GROUPS.map(([name, type]) => ({ name, label: typeLabel(type) })).sort((a, b) =>
      a.label.localeCompare(b.label, locale()),
    ),
  );
  const RARITY_CLASS = {
    Satanic: 'c-sat', Set: 'c-set', Heroic: 'c-her', Angelic: 'c-ang', Unholy: 'c-unh',
  };
  const PAGE_SIZE = 150;

  let query = $state('');
  let rarity = $state('');
  let group = $state('');
  let sort = $state('rarest');
  let limit = $state(PAGE_SIZE);
  // Cards first: the question this page is opened with is where to farm an
  // item, and in the table that answer is the fifth column of a row. The choice
  // outlives the window the way the dashboard's section does.
  let view = $state(recall('codex-view') === 'table' ? 'table' : 'cards');
  $effect(() => {
    remember('codex-view', view);
  });

  const odds = (rate) =>
    !rate
      ? '—'
      : rate >= 1e6
        ? `1/${(rate / 1e6).toFixed(rate >= 1e7 ? 0 : 1)}M`
        : rate >= 1e3
          ? `1/${(rate / 1e3).toFixed(rate >= 1e4 ? 0 : 1)}k`
          : `1/${rate}`;

  let found = $derived.by(() => {
    const q = searchKey(query.trim());
    const list = catalogue.filter(
      (i) =>
        (!q || i.search.includes(q) || searchKey(shown(i)).includes(q)) &&
        (!rarity || i.rarity === rarity) &&
        (!group || i.group === group),
    );
    const collator = new Intl.Collator(locale(), { sensitivity: 'base' });
    const byName = (a, b) => collator.compare(shown(a), shown(b));
    return list.sort(sort === 'name' ? byName : (a, b) => b.rate - a.rate || byName(a, b));
  });
  $effect(() => {
    query;
    rarity;
    group;
    sort;
    limit = PAGE_SIZE;
  });

  // ── the shopping list, from here ──────────────────────────────────────────
  //
  // Reading the catalogue and wanting one of these on the list is the same
  // moment, and until now it meant changing tab and typing the name out.
  //
  // The name that goes on the list is the one on the screen. The list is a
  // note the player reads back and types into the game's own search, and the
  // game is in the language they picked too.
  let menu = $state(null);
  let menuButton;
  let said = $state('');
  let saidTimer;

  const MENU_W = 210;
  const MENU_H = 103;
  const MAP_ROOT = 'https://parazeya.github.io/hs-map/';
  const mapUrl = (page, params) => `${MAP_ROOT}${page}?${new URLSearchParams(params)}`;
  const codexUrl = (it) => mapUrl('codex.html', { item: it.key, ...(it.record && { record: it.record }) });

  function report(message) {
    said = message;
    clearTimeout(saidTimer);
    saidTimer = setTimeout(() => (said = ''), 2500);
  }

  async function openLink(url) {
    try {
      if (native) await invoke('open_url', { url });
      else window.open(url, '_blank', 'noopener,noreferrer');
    } catch (e) {
      report(String(e));
    }
  }

  function openMenu(e, it, anchored = false) {
    e.preventDefault();
    const anchor = anchored ? e.currentTarget.getBoundingClientRect() : null;
    menuButton = anchored ? e.currentTarget : null;
    menu = {
      item: it,
      x: Math.max(0, Math.min(anchor?.left ?? e.clientX, window.innerWidth - MENU_W - 8)),
      y: Math.max(0, Math.min(anchor?.bottom ?? e.clientY, window.innerHeight - MENU_H - 8)),
    };
    if (anchored) requestAnimationFrame(() => document.getElementById('codex-item-actions')?.querySelector('button')?.focus());
  }

  function closeMenu() {
    menu = null;
    menuButton?.focus();
    menuButton = null;
  }

  async function copyItemLink() {
    const url = codexUrl(menu.item);
    closeMenu();
    try {
      if (native) await invoke('copy_text', { text: url });
      else await navigator.clipboard.writeText(url);
      report(t('copied'));
    } catch (e) {
      report(String(e));
    }
  }

  /// Add what the menu is holding, and say which of the two things happened.
  ///
  /// The list is read back rather than kept here: the Shopping List tab writes
  /// it too, and whichever of them last wrote is the one that counts.
  async function addToShopping() {
    const name = shown(menu.item);
    closeMenu();
    if (!name) return;
    try {
      const list = (await invoke('get_shopping')) ?? [];
      const already = list.some((row) => row.trim().toLowerCase() === name.toLowerCase());
      if (!already) {
        await invoke('set_shopping', { items: [...list, name] });
      }
      report(already ? say('{name} is already on the shopping list', { name })
                     : say('{name} added to the shopping list', { name }));
    } catch (e) {
      report(String(e));
    }
  }
</script>

{#snippet itemLocations(it)}
  {#each it.locations as [name, source, reference, kind = 'source'] (`${kind}:${source}:${name}`)}
    {@const place = (kind === 'room' && roomLabels[source]?.[language()]) || placeLabel(name)}
    {@const label = reference ? `${place} · ${t('Reference source')}` : place}
    {#if source}
      <button class="place jump" onclick={() => openLink(mapUrl('', { [kind]: source }))} title={t('Show on map')}>
        {label}
      </button>
    {:else}
      <span class="place">{label}</span>
    {/if}
  {:else}
    {#if loaded}<span class="dim">{t('Drop location unknown')}</span>{/if}
  {/each}
{/snippet}

{#snippet itemActions(it, compact = false)}
  <div class="actions" class:compact>
    {#if it.key}
      <button class="item-action" onclick={() => openLink(codexUrl(it))} title={t('Open in HS Map')} aria-label={t('Open in HS Map')}>
        {compact ? '↗' : t('Open in HS Map')}
      </button>
    {/if}
    {#if it.find}
      <button class="item-action" onclick={() => openLink(mapUrl('', { item: it.key, find: it.name }))} title={t('Show on map')} aria-label={t('Show on map')}>
        {compact ? '◎' : t('Show on map')}
      </button>
    {/if}
    <button class="item-action" onclick={(e) => openMenu(e, it, true)} title={t('Item actions')} aria-label={t('Item actions')} aria-expanded={menu?.item.identity === it.identity}>
      ⋯
    </button>
  </div>
{/snippet}

<svelte:window onkeydown={(e) => e.key === 'Escape' && closeMenu()} onblur={closeMenu} />

<div class="panel">
  <div class="body">
    <div class="tools" data-tauri-drag-region>
      <input class="find" aria-label={t('Search by name')} placeholder={t('Search by name')} bind:value={query} />
      <select class="picker" bind:value={rarity}>
        <option value="">{t("Any rarity")}</option>
        {#each rarities as r}<option value={r}>{t(r)}</option>{/each}
      </select>
      <select class="picker" bind:value={group}>
        <option value="">{t("Any kind")}</option>
        {#each groupChoices as g}<option value={g.name}>{g.label}</option>{/each}
      </select>
      <select class="picker narrow" bind:value={sort}>
        <option value="rarest">{t("Rarest first")}</option>
        <option value="name">{t("By name")}</option>
      </select>
      <div class="views">
        <button class="pick" class:on={view === 'cards'} onclick={() => (view = 'cards')}>{t("Cards")}</button>
        <button class="pick" class:on={view === 'table'} onclick={() => (view = 'table')}>{t("Table")}</button>
      </div>
    </div>

    <div class="box" style:border-image-source="url({art('chip_dark')})">
      {#if view === 'table'}
        <div class="head">
          <span class="hname">{t("Item")}</span>
          <span class="hkind">{t("Type")}</span>
          <span class="hgrade">{t("Tier")}</span>
          <span class="hrate">{t("Chance")}</span>
          <span class="hchase">{t("Drop location")}</span>
          <span></span>
        </div>

        <div class="rows" role="list">
          {#each found.slice(0, limit) as it (it.identity)}
            <div
              class="row"
              role="listitem"
              oncontextmenu={(e) => openMenu(e, it)}
            >
              <span class="name {RARITY_CLASS[it.rarity] ?? ''}" title={it.rarity ? t(it.rarity) : t('unlisted')}>{shown(it)}</span>
              <span class="kind dim">{kindOf(it)}</span>
              <span class="grade dim">{tierLabel(it.tier) || '—'}</span>
              <span class="rate">{odds(it.rate)}</span>
              <span class="chase">
                {#if it.chase}<b>{odds(it.chase)}</b>{/if}
                {@render itemLocations(it)}
              </span>
              {@render itemActions(it, true)}
            </div>
          {:else}
            <div class="empty dim">{t("nothing matches that")}</div>
          {/each}
        </div>
      {:else}
        <div class="grid" role="list">
          {#each found.slice(0, limit) as it (it.identity)}
            <div
              class="card {RARITY_CLASS[it.rarity] ?? ''}"
              role="listitem"
              oncontextmenu={(e) => openMenu(e, it)}
            >
              <div class="cname" title={it.rarity ? t(it.rarity) : t('unlisted')}>{shown(it)}</div>
              <div class="cline dim">{kindOf(it)}{it.tier ? ` · ${tierLabel(it.tier)}` : ''}</div>
              <div class="odds">
                <span class="dim">{t('Chance')}</span>
                <span class="rate">{odds(it.rate)}</span>
                {#if it.chase}
                  <span class="dim">{t("tied")}</span>
                  <b class="tied">{odds(it.chase)}</b>
                {/if}
              </div>
              <div class="places">
                {@render itemLocations(it)}
              </div>
              {@render itemActions(it)}
            </div>
          {:else}
            <div class="empty dim">{t("nothing matches that")}</div>
          {/each}
        </div>
      {/if}

      {#if said}
        <div class="said" role="status">{said}</div>
      {/if}

      <div class="foot dim">
        {say('{n} of {total} items', { n: Math.min(found.length, limit), total: found.length })}
        {#if found.length > limit}
          <button class="item-action" onclick={() => (limit += PAGE_SIZE)}>{t('more')}</button>
        {/if}
      </div>
    </div>
  </div>
</div>

{#if menu}
  <!-- The backdrop is what closes it: a menu that outlives the click that
       opened it is a menu that follows the reader down the page. -->
  <div
    class="scrim"
    role="presentation"
    onclick={closeMenu}
    oncontextmenu={(e) => {
      e.preventDefault();
      closeMenu();
    }}
  ></div>
  <div id="codex-item-actions" class="menu" style:left="{menu.x}px" style:top="{menu.y}px" style:border-image-source="url({art('chip_dark')})">
    <div class="mname">{shown(menu.item)}</div>
    {#if menu.item.key}<button class="mitem" onclick={copyItemLink}>{t('Copy item link')}</button>{/if}
    <button class="mitem" onclick={addToShopping}>{t('Add to the shopping list')}</button>
  </div>
{/if}

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
    font-family: var(--face);
    font-size: 12px;
    color: var(--bone-6);
  }

  /* and when even the floor does not fit, the row wraps rather than overflowing */
  .tools {
    flex-wrap: wrap; display: flex; gap: 6px; align-items: center; }
  .find {
    flex: 1 1 auto;
    /* A floor, because everything beside it refuses to shrink: the three
       selects size to their widest option and the view buttons are fixed, so
       at the 620px minimum the search box — the thing the page is for — was
       the only thing left to take the space out of. */
    min-width: 9rem;
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

  .views { display: flex; flex: none; }
  .pick {
    box-sizing: border-box;
    font: inherit;
    font-size: 11px;
    height: 24px;
    color: var(--bone-3);
    background: rgba(0, 0, 0, 0.35);
    border: 1px solid var(--ground-10);
    padding: 0 8px;
    cursor: pointer;
  }
  /* the two share an edge, and the chosen one is drawn over it */
  .pick + .pick { margin-left: -1px; }
  .pick:hover { color: var(--bone-9); }
  .pick.on {
    position: relative;
    color: var(--bone-13);
    border-color: var(--edge-4);
    background: rgba(var(--pick-rgb), 0.45);
  }

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
    grid-template-columns: minmax(0, 1fr) minmax(36px, 74px) 25px 48px minmax(0, 1.4fr) 63px;
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
  .name, .kind { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .grade, .rate { text-align: right; }
  .rate { color: var(--bone-9); }
  .chase { display: flex; flex-wrap: wrap; gap: 2px 6px; align-items: baseline; min-width: 0; font-size: 10px; }
  .chase b { color: var(--gold-2); font-weight: normal; }

  /* Cards reflow into whatever width the window is dragged to; the minimum is
     narrow enough that two still fit side by side at the smallest window the
     app allows, so a card never has to scroll sideways to be read. */
  .grid {
    flex: 1 1 auto;
    min-height: 0;
    overflow-y: auto;
    overflow-x: hidden;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(158px, 1fr));
    gap: 5px;
    padding: 3px 2px 2px 0;
    align-content: start;
  }
  .grid .empty { grid-column: 1 / -1; }

  .card {
    box-sizing: border-box;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
    /* the accent is the rarity colour the name already wears, so the eye can
       sort a screenful of cards without reading one of them */
    border-left: 3px solid currentColor;
    background: linear-gradient(180deg, var(--ground-8), var(--ground-4));
    padding: 4px 6px 5px;
  }
  .card:hover, .card:focus-visible { background: linear-gradient(180deg, var(--ground-9), var(--ground-6)); }
  .card:focus-visible { outline: 1px solid var(--edge-4); outline-offset: -1px; }

  .cname { font-size: 12px; line-height: 1.2; overflow-wrap: anywhere; }
  .cline { font-size: 10px; }
  .odds {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    align-items: baseline;
    font-size: 10px;
    padding-top: 1px;
  }
  .odds .rate, .odds .tied { text-align: right; }
  .odds .rate { color: var(--bone-9); font-size: 11px; }
  .odds .tied { color: var(--gold-2); font-size: 13px; font-weight: normal; }

  .places { display: flex; flex-wrap: wrap; gap: 2px; padding-top: 2px; }
  .place {
    color: var(--bone-7);
    background: rgba(0, 0, 0, 0.3);
    border: 1px solid var(--ground-10);
    padding: 0 3px;
    font: inherit;
    font-size: 10px;
    text-align: left;
    overflow-wrap: anywhere;
  }
  .jump { cursor: pointer; }
  .jump::after { content: ' ↗'; }
  .jump:hover, .item-action:hover { border-color: var(--edge-4); color: var(--bone-13); }
  .actions { display: flex; flex-wrap: wrap; gap: 3px; margin-top: auto; padding-top: 3px; }
  .actions.compact { justify-content: flex-end; flex-wrap: nowrap; padding: 0; }
  .item-action {
    min-height: 18px;
    padding: 1px 4px;
    background: rgba(0, 0, 0, 0.3);
    border: 1px solid var(--ground-10);
    color: var(--bone-9);
    font: inherit;
    font-size: 10px;
    cursor: pointer;
  }

  .empty { padding: 12px 0; text-align: center; }
  /* Above everything, and fixed to the viewport because the coordinates are
     the cursor's. */
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 40;
  }
  .menu {
    position: fixed;
    z-index: 41;
    /* Under the art rather than instead of it: the game skin's nine-slice is
       drawn with `fill` and covers this, and the plain skin has no art to
       cover anything. A popup that is not opaque is unreadable over whatever
       it was opened on. */
    background: var(--ground-1);
    width: 210px;
    box-sizing: border-box;
    border: 6px solid transparent;
    border-image-slice: 6 fill;
    border-image-width: 6px;
    image-rendering: pixelated;
    padding: 6px 4px;
    font-family: var(--face);
  }
  .mname {
    font-size: 11px;
    color: var(--bone-3);
    padding: 2px 8px 4px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .mitem {
    display: block;
    width: 100%;
    font: inherit;
    font-size: 12px;
    text-align: left;
    color: var(--bone-12);
    background: none;
    border: none;
    padding: 5px 8px;
    cursor: pointer;
  }
  .mitem:hover {
    background: rgba(255, 255, 255, 0.07);
    color: var(--gold-2);
  }

  .said {
    font-size: 11px;
    color: var(--gold-2);
    padding: 4px 2px 0;
  }

  .foot { display: flex; gap: 8px; align-items: center; padding-top: 4px; font-size: 11px; }
  .dim { color: var(--bone-3); }

  .c-sat { color: var(--rar-satanic); }
  .c-set { color: #40d040; }
  .c-her { color: #00ffae; }
  .c-ang { color: #f6f794; }
  .c-unh { color: #e04a7a; }
</style>

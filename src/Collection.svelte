<script>
  import { DROP_CHASE, DROP_PLACES, DROP_RATE, DROP_ZONES, ITEMS, RARITY_BY_NAME, TIER_BY_NAME, tierLabel } from './items.js';
  import { publicSeason } from './format.js';
  import { itemName, locale, nameOf, placeLabel, say, t, typeLabel } from './say.svelte.js';
  import { invoke, listen } from './bridge.js';
  import { art } from './skin.svelte.js';

  const UNIQUE_RARITIES = ['Satanic', 'Set', 'Heroic', 'Angelic', 'Unholy'];
  // These are the game's equipment slots, including charms and flasks.
  const GEAR = new Set([0, 1, 2, 3, 4, 5, 6, 7, 8, 10, 18]);
  const CATALOGUE = (() => {
    const seen = new Set();
    return Object.entries(ITEMS).flatMap(([key, name]) => {
      const [type, id, weapon] = key.split(':').map(Number);
      const lower = name.toLowerCase();
      const rarity = RARITY_BY_NAME[lower] ?? '';
      if (!GEAR.has(type) || !UNIQUE_RARITIES.includes(rarity) || seen.has(lower)) return [];
      seen.add(lower);
      return [{ key: lower, name, type, id, weapon, rarity, tier: TIER_BY_NAME[lower] ?? 0,
        rate: DROP_RATE[lower] ?? 0, chase: DROP_CHASE[lower] ?? 0,
        places: (DROP_PLACES[lower] ?? []).filter(Boolean), zones: DROP_ZONES[lower] ?? [] }];
    });
  })();
  const CATALOGUE_BY_NAME = new Map(CATALOGUE.map((it) => [it.key, it]));
  const RARITY_TOTALS = Object.fromEntries(UNIQUE_RARITIES.map((r) => [r, CATALOGUE.filter((it) => it.rarity === r).length]));
  const PAGE_SIZE = 100;
  const COLORS = { Satanic: 'c-sat', Set: 'c-set', Heroic: 'c-her', Angelic: 'c-ang', Unholy: 'c-unh' };

  let data = $state({ current: null, characters: {}, items: {} });
  let expanded = $state('');
  let query = $state('');
  let rarity = $state('');
  let filter = $state('all');
  let page = $state(0);
  let saving = $state('');
  let error = $state('');
  let selected = $state(null);
  let pendingEdit = $state(null);
  let importOpen = $state(false);
  let importUrl = $state('');
  let importTarget = $state('');
  let importPreview = $state(null);
  let importError = $state('');
  let importResult = $state(null);
  let modalTrigger;
  let generation = 0;

  function openDetails(event, item) {
    modalTrigger = event.currentTarget;
    selected = item;
  }

  function closeDetails() { selected = null; }

  function focusDetails(node) {
    node.focus();
    return { destroy() { if (modalTrigger?.isConnected) modalTrigger.focus(); } };
  }

  function askEdit(event, action, key = null, name = '') {
    modalTrigger = event.currentTarget;
    selected = null;
    pendingEdit = { action, key, name };
  }

  function closeEdit() { if (!saving) pendingEdit = null; }

  function onEscape(event) {
    if (event.key !== 'Escape') return;
    if (pendingEdit) closeEdit();
    else if (importOpen) closeImport();
    else closeDetails();
  }

  function openImport(event) {
    modalTrigger = event.currentTarget;
    importTarget ||= data.current?.key ?? roster[0]?.[0] ?? '';
    importError = '';
    importResult = null;
    importOpen = true;
  }

  function closeImport() {
    if (saving) return;
    importOpen = false;
    importPreview = null;
    importError = '';
  }

  async function previewImport(source) {
    if (saving) return;
    saving = 'import-preview';
    importError = '';
    importPreview = null;
    try {
      importPreview = source === 'file'
        ? await invoke('preview_checklist_file')
        : await invoke('preview_checklist_url', { url: importUrl.trim() });
    } catch (e) { importError = String(e); }
    saving = '';
  }

  let importMatch = $derived.by(() => {
    const matched = new Map();
    const skipped = [];
    for (const row of importPreview?.checked ?? []) {
      const item = row.canonical && CATALOGUE_BY_NAME.get(row.canonical.toLowerCase());
      if (item) matched.set(item.key, { item, row });
      else skipped.push(row.name);
    }
    return { matched: [...matched.values()], skipped };
  });
  let importNewForCharacter = $derived(importMatch.matched.filter(({ item }) => data.characters[importTarget]?.items?.[item.key] === undefined).length);
  let importNewForCollection = $derived(importMatch.matched.filter(({ item }) => owned[item.key] === undefined).length);

  async function commitImport() {
    if (saving || !importTarget || !importMatch.matched.length) return;
    saving = 'import-commit';
    importError = '';
    try {
      importResult = await invoke('import_checklist_items', { key: importTarget, rows: importMatch.matched.map(({ row }) => row) });
      importOpen = false;
      importPreview = null;
      await load();
    } catch (e) { importError = String(e); }
    saving = '';
  }

  function editTitle(edit) {
    switch (edit.action) {
      case 'clear_history': return t('Clear collection history?');
      case 'clear_character': return say('Clear finds for {name}?', { name: edit.name });
      case 'remove_character': return say('Remove {name} from the collection?', { name: edit.name });
      default: return t('Remove all saved characters?');
    }
  }

  function editDescription(edit) {
    switch (edit.action) {
      case 'clear_history': return t('All recorded finds will be deleted. Collection mode stays on for enabled characters.');
      case 'clear_character': return t('Only this character’s finds will be deleted. Finds saved by other characters remain.');
      case 'remove_character': return t('The character and all their finds will be deleted. They can be added again later.');
      default: return t('All saved characters and their finds will be deleted. The current character can be added again.');
    }
  }

  async function confirmEdit() {
    const edit = pendingEdit;
    if (!edit || saving) return;
    saving = 'edit';
    error = '';
    try {
      await invoke('edit_collection', { action: edit.action, key: edit.key });
      importResult = null;
      if (edit.action === 'remove_all_characters' || (edit.action === 'remove_character' && expanded === edit.key)) expanded = '';
      pendingEdit = null;
      await load();
    } catch (e) {
      error = String(e);
      pendingEdit = null;
    }
    saving = '';
  }

  const odds = (rate) => rate ? `1 / ${Number(rate).toLocaleString(locale())}` : '—';
  // Bosses and chests can have one listed chance: the catalogue repeats it as
  // both the general and tied rate. Show the actual source just once.
  const onePlaceRate = (it) => it.chase > 0 && it.chase === it.rate
    && (it.places.length > 0 || it.zones.length > 0);
  const showGeneralRate = (it) => (!it.chase || it.rate > 0) && !onePlaceRate(it);
  const showPlaceRate = (it) => it.chase > 0 && (it.chase !== it.rate || onePlaceRate(it));

  async function load() {
    const mine = ++generation;
    try {
      const next = await invoke('get_collection');
      if (!next || mine !== generation) return;
      const changed = data.current?.key !== next.current?.key;
      data = next;
      if (changed && next.current) expanded = next.current.key;
      error = '';
    } catch (e) { error = String(e); }
  }

  $effect(() => {
    load();
    const unsubs = [
      listen('collection-changed', load),
      listen('stats', (e) => {
        const c = e.payload?.character;
        if (c && (c.name !== data.current?.name || c.season !== data.current?.season || c.hardcore !== data.current?.hardcore)) load();
      }),
    ];
    return () => unsubs.forEach((u) => u.then((f) => f()));
  });

  let roster = $derived.by(() => {
    const rows = Object.entries(data.characters);
    const current = data.current;
    if (current && !data.characters[current.key]) {
      rows.push([current.key, { name: current.name, season: current.season, hardcore: current.hardcore, enabled: false, items: {} }]);
    }
    return rows.sort(([ka, a], [kb, b]) =>
      ka === current?.key ? -1 : kb === current?.key ? 1 : a.name.localeCompare(b.name, locale()));
  });
  const characterFinds = (c) => Object.entries(c.items ?? {})
    .filter(([name]) => CATALOGUE_BY_NAME.has(name))
    .sort((a, b) => b[1] - a[1]);
  let owned = $derived(data.items ?? {});
  let storedCount = $derived(Object.keys(data.items ?? {}).length);
  let savedCharacterCount = $derived(Object.keys(data.characters ?? {}).length);
  let rarityCounts = $derived.by(() => {
    const counts = Object.fromEntries(UNIQUE_RARITIES.map((r) => [r, 0]));
    for (const it of CATALOGUE) {
      if (owned[it.key] !== undefined && counts[it.rarity] !== undefined) counts[it.rarity]++;
    }
    return counts;
  });
  let uniqueCount = $derived(UNIQUE_RARITIES.reduce((sum, r) => sum + rarityCounts[r], 0));
  let found = $derived.by(() => {
    const q = query.trim().toLocaleLowerCase(locale());
    return CATALOGUE.filter((it) => {
      const has = owned[it.key] !== undefined;
      return (!q || it.name.toLowerCase().includes(q) || String(itemName(it.type, it.id, it.weapon)).toLocaleLowerCase(locale()).includes(q))
        && (!rarity || it.rarity === rarity)
        && (filter === 'all' || (filter === 'owned') === has);
    }).sort((a, b) => a.name.localeCompare(b.name));
  });
  let pages = $derived(Math.max(1, Math.ceil(found.length / PAGE_SIZE)));
  $effect(() => { query; rarity; filter; page = 0; });

  async function toggle(key, enabled) {
    if (!key || saving) return;
    saving = key;
    error = '';
    try {
      await invoke('set_collection_enabled', { key, enabled: !enabled });
      await load();
    } catch (e) { error = String(e); }
    saving = '';
  }

  const mode = (c) => `${c.season > 0 ? `${t('Season')} ${publicSeason(c.season)}` : t('Non-season')} · ${c.hardcore ? t('Hardcore') : t('Softcore')}`;
</script>

<svelte:window onkeydown={onEscape} />

<div class="panel">
  <div class="top">
    <div class="heading">{t('Collection')}</div>
    <div class="subtitle">{t('One collection shared by the characters you turn on.')} {t('Unique gear and charms only.')}</div>
  </div>

  <div class="rarity-summary" aria-label={t('Unique items by rarity')}>
    <span class="unique-total">{t('Unique items')}: {uniqueCount} / {CATALOGUE.length}</span>
    {#each UNIQUE_RARITIES as r}
      <button class="pick rarity-count" class:on={rarity === r} aria-pressed={rarity === r} onclick={() => rarity = rarity === r ? '' : r} title={t(r)}>
        <span class={rarity === r ? '' : COLORS[r]}>{t(r)}</span><b>{rarityCounts[r]} / {RARITY_TOTALS[r]}</b>
      </button>
    {/each}
  </div>

  {#if error}<div class="error">{error}</div>{/if}
  {#if !data.current && !roster.length}
    <div class="note">{t('The character appears after the first game save. Then turn collection mode on to record new finds.')}</div>
  {/if}

  <div class="characters" style:border-image-source="url({art('chip_dark')})">
    <div class="roster-title sechead">{t('Characters in this collection')} <span>{roster.filter(([, c]) => c.enabled).length} / {roster.length}</span></div>
    {#each roster as [key, c] (key)}
      <div class="character-row">
        <button class="pick expand" class:expanded={expanded === key} aria-expanded={expanded === key} aria-label={t('Show character finds')} title={t('Show character finds')} onclick={() => expanded = expanded === key ? '' : key}>
          <svg viewBox="0 0 16 16" aria-hidden="true"><path d="m6 3.5 4 4.5-4 4.5" /></svg>
        </button>
        <span class="character-name">{c.name}{key === data.current?.key ? ` · ${t('current')}` : ''}{!data.characters[key] ? ` · ${t('not added')}` : ''}</span>
        <span class="character-mode">{mode(c)}</span>
        <span class="character-count">{characterFinds(c).length}</span>
        <button class="btn switch" class:on={c.enabled} aria-pressed={c.enabled} disabled={!!saving} onclick={() => toggle(key, c.enabled)}>{c.enabled ? t('On') : t('Off')}</button>
      </div>
      {#if expanded === key}
        <div class="character-finds">
          {#each characterFinds(c) as [name] (name)}
            <button class="find-chip {COLORS[CATALOGUE_BY_NAME.get(name).rarity]}" onclick={(event) => openDetails(event, CATALOGUE_BY_NAME.get(name))}>{nameOf(name)}</button>
          {:else}
            <span class="dim">{t('No finds recorded for this character yet.')}</span>
          {/each}
        </div>
        {#if data.characters[key]}
          <div class="character-actions">
            <button class="pick danger" disabled={!!saving || !Object.keys(c.items ?? {}).length} onclick={(event) => askEdit(event, 'clear_character', key, c.name)}>{t('Clear finds')}</button>
            <button class="pick danger" disabled={!!saving} onclick={(event) => askEdit(event, 'remove_character', key, c.name)}>{t('Remove character')}</button>
          </div>
        {/if}
      {/if}
    {/each}
  </div>

  <div class="collection-actions">
    <button class="pick" disabled={!!saving} onclick={openImport}>{t('Import Check List')}</button>
    <button class="pick danger" disabled={!!saving || !storedCount} onclick={(event) => askEdit(event, 'clear_history')}>{t('Clear history')}</button>
    <button class="pick danger" disabled={!!saving || !savedCharacterCount} onclick={(event) => askEdit(event, 'remove_all_characters')}>{t('Remove all characters')}</button>
  </div>
  {#if importResult}
    <div class="note">{say('Imported {added} items for this character; {shared} new to the collection.', { added: importResult.added_to_character, shared: importResult.new_to_collection })}</div>
  {/if}

  <div class="tools">
    <input class="find" type="search" placeholder={t('Search by name')} bind:value={query} />
    <select class="picker" bind:value={rarity}>
      <option value="">{t('Any rarity')}</option>
      {#each UNIQUE_RARITIES as r}<option value={r}>{t(r)}</option>{/each}
    </select>
    <div class="filters">
      <button class="pick" class:on={filter === 'all'} onclick={() => filter = 'all'}>{t('All')}</button>
      <button class="pick" class:on={filter === 'owned'} onclick={() => filter = 'owned'}>{t('Found')}</button>
      <button class="pick" class:on={filter === 'missing'} onclick={() => filter = 'missing'}>{t('Missing')}</button>
    </div>
  </div>

  <div class="box" style:border-image-source="url({art('chip_dark')})">
    <div class="rows" role="list">
      {#each found.slice(page * PAGE_SIZE, (page + 1) * PAGE_SIZE) as it (it.key)}
        <div role="listitem">
          <button class="row" class:has={owned[it.key] !== undefined} onclick={(event) => openDetails(event, it)} title={t('Drop location')}>
            <span class="mark">{owned[it.key] !== undefined ? '✓' : '·'}</span>
            <span class="name {COLORS[it.rarity] ?? ''}">{itemName(it.type, it.id, it.weapon) ?? it.name}</span>
            <span class="kind">{typeLabel(it.type, it.weapon)}</span>
            <span class="grade">{tierLabel(it.tier) || '—'}</span>
            <span class="status">{owned[it.key] !== undefined ? t('Found') : t('Missing')}</span>
            <span class="open" aria-hidden="true"><svg viewBox="0 0 16 16"><path d="m6 3.5 4 4.5-4 4.5" /></svg></span>
          </button>
        </div>
      {:else}
        <div class="empty">{t('nothing matches that')}</div>
      {/each}
    </div>
    <div class="foot">
      <span>{found.length} / {CATALOGUE.length}</span>
      <div class="paging">
        <button class="pick" disabled={page === 0} onclick={() => page--}>‹</button>
        <span>{page + 1} / {pages}</span>
        <button class="pick" disabled={page + 1 >= pages} onclick={() => page++}>›</button>
      </div>
    </div>
  </div>

  {#if selected}
    <div class="detail-scrim" role="presentation" onclick={closeDetails}></div>
    <div class="detail-card menu" role="dialog" aria-modal="true" aria-label={itemName(selected.type, selected.id, selected.weapon) ?? selected.name} tabindex="-1" use:focusDetails style:border-image-source="url({art('chip_dark')})">
      <button class="pick detail-close" onclick={closeDetails} aria-label={t('close')} title={t('close')}>
        <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M4 4 12 12M12 4 4 12" /></svg>
      </button>
      <div class="detail-name {COLORS[selected.rarity]}">{itemName(selected.type, selected.id, selected.weapon) ?? selected.name}</div>
      <div class="detail-kind">{typeLabel(selected.type, selected.weapon)} · {t(selected.rarity)}{selected.tier ? ` · ${tierLabel(selected.tier)}` : ''}</div>
      <div class="detail-odds" class:solo={!(showGeneralRate(selected) && showPlaceRate(selected))}>
        {#if showGeneralRate(selected)}
          <div class="detail-odd"><span>{t('Chance')} · {t('anywhere')}</span><strong>{odds(selected.rate)}</strong></div>
        {/if}
        {#if showPlaceRate(selected)}
          <div class="detail-odd"><span>{t('Chance')} · {t('Drop location')}</span><strong class="boosted">{odds(selected.chase)}</strong></div>
        {/if}
      </div>
      {#if !selected.rate && !selected.chase}
        <div class="detail-hint">{t('the game states no drop chance for this — it comes from a boss, a chest or a tower rather than falling in the world')}</div>
      {/if}
      <div class="detail-label">{t('Drop location')}</div>
      <div class="detail-places">
        {#each selected.places.length ? selected.places : selected.zones as place}
          <span class="place">{placeLabel(place)}</span>
        {:else}
          <span class="dim">{t('drops anywhere')}</span>
        {/each}
      </div>
    </div>
  {/if}
  {#if pendingEdit}
    <div class="detail-scrim" role="presentation" onclick={closeEdit}></div>
    <div class="detail-card confirm-card menu" role="alertdialog" aria-modal="true" aria-label={editTitle(pendingEdit)} tabindex="-1" use:focusDetails style:border-image-source="url({art('chip_dark')})">
      <div class="confirm-title">{editTitle(pendingEdit)}</div>
      <div class="confirm-description">{editDescription(pendingEdit)}</div>
      <div class="confirm-actions">
        <button class="pick" disabled={!!saving} onclick={closeEdit}>{t('Cancel')}</button>
        <button class="pick danger confirm-delete" disabled={!!saving} onclick={confirmEdit}>{pendingEdit.action.startsWith('remove') ? t('Remove') : t('Clear')}</button>
      </div>
    </div>
  {/if}
  {#if importOpen}
    <div class="detail-scrim" role="presentation" onclick={closeImport}></div>
    <div class="detail-card import-card menu" role="dialog" aria-modal="true" aria-label={t('Import Check List')} tabindex="-1" use:focusDetails style:border-image-source="url({art('chip_dark')})">
      <button class="pick detail-close" onclick={closeImport} aria-label={t('close')} title={t('close')}>
        <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M4 4 12 12M12 4 4 12" /></svg>
      </button>
      <div class="confirm-title">{t('Import Check List')}</div>
      <div class="confirm-description">{t('Paste a link to your Google Sheets copy or choose its downloaded .xlsx file. Only checked unique gear and charms will be added.')}</div>
      <label class="import-label" for="checklist-url">{t('Google Sheets link')}</label>
      <div class="import-source">
        <input id="checklist-url" class="find" type="url" placeholder="https://docs.google.com/spreadsheets/d/…" bind:value={importUrl} />
        <button class="pick" disabled={!!saving || !importUrl.trim()} onclick={() => previewImport('url')}>{t('Preview')}</button>
        <button class="pick" disabled={!!saving} onclick={() => previewImport('file')}>{t('Choose .xlsx')}</button>
      </div>
      {#if importError}<div class="error import-message">{importError}</div>{/if}
      {#if importPreview}
        <div class="import-preview">
          <div class="import-source-name">{importPreview.source}</div>
          <div>{say('{checked} checked · {matched} matched · {skipped} skipped', { checked: importPreview.checked.length, matched: importMatch.matched.length, skipped: importMatch.skipped.length })}</div>
          {#if importMatch.matched.length}
            <div class="import-matches" role="list">
              {#each importMatch.matched as { item } (item.key)}
                <span role="listitem" class={COLORS[item.rarity]}>{item.name}</span>
              {/each}
            </div>
          {/if}
          {#if importMatch.skipped.length}
            <div class="import-skipped">{t('No exact match by name, rarity and slot')}: {importMatch.skipped.join(', ')}</div>
          {/if}
          <label class="import-label" for="checklist-character">{t('Import for character')}</label>
          <select id="checklist-character" class="picker import-character" bind:value={importTarget}>
            {#each roster as [key, c] (key)}
              <option value={key}>{c.name} · {mode(c)}</option>
            {/each}
          </select>
          {#if !roster.length}<div class="dim">{t('A character appears after the first game save.')}</div>{/if}
          <div class="import-totals">{say('{added} new for this character · {shared} new for the collection', { added: importNewForCharacter, shared: importNewForCollection })}</div>
        </div>
      {/if}
      <div class="confirm-actions import-actions">
        <button class="pick" disabled={!!saving} onclick={closeImport}>{t('Cancel')}</button>
        <button class="pick" disabled={!!saving || !importPreview || !importTarget || !importMatch.matched.length} onclick={commitImport}>{t('Import')}</button>
      </div>
    </div>
  {/if}
</div>

<style>
  .panel { position: relative; isolation: isolate; height: 100%; box-sizing: border-box; display: flex; flex-direction: column; gap: 8px; padding: 6px; color: var(--bone-6); font: 12px var(--face); }
  .heading { color: var(--bone-13); font-size: 19px; }
  .subtitle { color: var(--bone-3); }
  .rarity-summary, .tools { display: flex; gap: 6px; align-items: center; flex-wrap: wrap; }
  .unique-total { color: var(--gold-2); margin-right: 3px; white-space: nowrap; }
  .rarity-count { display: inline-flex; gap: 6px; align-items: center; white-space: nowrap; }
  .rarity-count b { font-variant-numeric: tabular-nums; }

  .pick { box-sizing: border-box; font: inherit; font-size: 11px; height: 24px; color: var(--bone-3); background: rgba(0, 0, 0, .35); border: 1px solid var(--ground-10); padding: 0 8px; cursor: pointer; }
  .pick:hover { color: var(--bone-9); }
  .pick.on { position: relative; color: var(--bone-13); border-color: var(--edge-4); background: rgba(var(--pick-rgb), .45); }
  .pick.danger { color: var(--rar-satanic); }
  .pick.danger:hover:not(:disabled) { border-color: var(--rar-satanic); }
  .pick:focus-visible, .btn:focus-visible, .find:focus-visible, .picker:focus-visible { outline: 2px solid var(--gold-2); outline-offset: 2px; }
  button:disabled { opacity: .45; cursor: default; }
  .filters { display: flex; }
  .filters .pick + .pick { margin-left: -1px; }

  .characters, .box { box-sizing: border-box; border: 6px solid transparent; border-image-slice: 6 fill; border-image-width: 6px; image-rendering: pixelated; padding: 6px 8px; }
  .characters { flex: none; max-height: 170px; overflow-y: auto; }
  .roster-title { display: flex; justify-content: space-between; color: var(--gold-2); padding-bottom: 5px; border-bottom: 1px solid var(--ground-10); }
  .character-row { display: grid; grid-template-columns: 30px minmax(0, 1fr) auto 26px auto; align-items: center; column-gap: 8px; box-sizing: border-box; min-height: 39px; padding: 4px 0; border-bottom: 1px solid var(--ground-10); }
  .character-row:last-child { border-bottom: 0; }
  .character-name { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--bone-13); }
  .character-mode { color: var(--bone-3); white-space: nowrap; justify-self: end; }
  .character-count { color: var(--gold-2); text-align: right; font-variant-numeric: tabular-nums; }
  .character-finds { padding: 7px 0 9px 38px; display: flex; flex-wrap: wrap; gap: 5px; border-bottom: 1px solid var(--ground-10); }
  .character-actions { display: flex; justify-content: flex-end; gap: 6px; padding: 6px 0 8px; border-bottom: 1px solid var(--ground-10); }
  .collection-actions { display: flex; justify-content: flex-end; gap: 6px; }
  .find-chip { font: inherit; background: var(--plain-slab-2, var(--ground-7)); border: 1px solid var(--plain-line, var(--ground-10)); padding: 2px 5px; cursor: pointer; }
  .find-chip:hover, .find-chip:focus-visible { background: var(--plain-hover, var(--ground-9)); }
  .find-chip:focus-visible, .row:focus-visible { outline: 2px solid var(--gold-2); outline-offset: -2px; }
  .dim { color: var(--bone-3); }
  .pick.expand { display: grid; place-items: center; box-sizing: border-box; width: 30px; min-width: 30px; height: 30px; min-height: 30px; padding: 0; line-height: 0; }
  .expand svg, .open svg { display: block; width: 15px; height: 15px; fill: none; stroke: currentColor; stroke-width: 1.8; stroke-linecap: round; stroke-linejoin: round; }
  .expand svg { transition: transform .15s ease; }
  .expand.expanded svg { transform: rotate(90deg); }
  .switch { box-sizing: border-box; min-width: 56px; height: 30px; flex: none; font: inherit; color: var(--bone-13); text-shadow: 0 1px 0 var(--ground-1); border: 6px solid transparent; border-image-source: var(--btn); border-image-slice: 6 fill; border-image-width: 6px; image-rendering: pixelated; cursor: pointer; }
  .btn.switch { display: grid; place-items: center; padding: 0 10px; line-height: 1; }
  .switch:hover { border-image-source: var(--btn-hover); }
  .switch:active { border-image-source: var(--btn-down); }
  .switch.on { color: var(--gold-2); }

  .find { flex: 1 1 auto; min-width: 9rem; box-sizing: border-box; font: inherit; font-size: 12px; color: var(--bone-13); background: rgba(0, 0, 0, .35); border: 1px solid var(--ground-10); padding: 4px 8px; height: 24px; }
  .find::placeholder { color: var(--bone-3); }
  .find:focus { outline: none; border-color: var(--edge-4); }
  .picker { box-sizing: border-box; appearance: none; font: inherit; font-size: 11px; color: var(--bone-13); background-color: rgba(0, 0, 0, .35); background-image: linear-gradient(45deg, transparent 50%, var(--bone-6) 50%), linear-gradient(135deg, var(--bone-6) 50%, transparent 50%); background-position: calc(100% - 12px) 50%, calc(100% - 7px) 50%; background-size: 5px 5px, 5px 5px; background-repeat: no-repeat; border: 1px solid var(--ground-10); border-radius: 0; padding: 3px 22px 3px 6px; height: 24px; cursor: pointer; }
  .picker option { background: var(--ground-7); color: var(--bone-9); }
  .picker:hover, .picker:focus { border-color: var(--edge-4); }

  .note { color: var(--bone-9); border-left: 3px solid var(--gold-2); background: var(--plain-slab, var(--ground-6)); padding: 7px 9px; }
  .error { color: var(--rar-satanic); }
  .box { min-height: 0; flex: 1 1 auto; display: flex; flex-direction: column; }
  .rows { min-height: 0; flex: 1; overflow-y: auto; }
  .row { box-sizing: border-box; width: 100%; min-height: 28px; display: flex; align-items: center; gap: 8px; padding: 0 4px; color: inherit; background: none; border: 0; border-bottom: 1px solid var(--plain-sep, var(--ground-10)); font: inherit; text-align: left; cursor: pointer; }
  .row:hover { background: var(--plain-rowhover, rgba(var(--pick-rgb), .08)); }
  .mark { width: 14px; flex: none; color: var(--gold-2); font-size: 16px; }
  .name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .kind { width: 94px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--bone-3); }
  .grade { width: 23px; color: var(--bone-3); }
  .status { width: 52px; color: var(--bone-3); text-align: right; }
  .row.has .status { color: var(--gold-2); }
  .open { display: grid; place-items: center; width: 15px; flex: none; color: var(--bone-3); }
  .row:hover .open { color: var(--gold-2); }
  .empty { text-align: center; padding: 24px; color: var(--bone-3); }
  .foot { display: flex; justify-content: space-between; align-items: center; padding: 5px 2px 0; color: var(--bone-3); font-size: 11px; }
  .paging { display: flex; align-items: center; gap: 8px; }
  .paging .pick { min-width: 24px; padding: 0 5px; }
  .detail-scrim { position: absolute; inset: 0; z-index: 4; background: rgba(0, 0, 0, .62); }
  .detail-card { position: absolute; z-index: 5; top: 50%; left: 50%; transform: translate(-50%, -50%); box-sizing: border-box; width: min(520px, calc(100% - 24px)); max-height: calc(100% - 24px); overflow-y: auto; border: 6px solid transparent; border-image-slice: 6 fill; border-image-width: 6px; background: var(--ground-1); image-rendering: pixelated; padding: 16px; color: var(--bone-9); }
  .detail-card:focus { outline: none; }
  .pick.detail-close { position: absolute; top: 8px; right: 8px; display: grid; place-items: center; box-sizing: border-box; width: 30px; min-width: 30px; height: 30px; min-height: 30px; padding: 0; line-height: 0; }
  .detail-close svg { display: block; width: 16px; height: 16px; fill: none; stroke: currentColor; stroke-width: 1.8; stroke-linecap: round; }
  .detail-name { padding-right: 32px; font-size: 18px; overflow-wrap: anywhere; }
  .detail-kind { color: var(--bone-3); padding-top: 3px; }
  .detail-odds { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 8px; margin: 16px 0; }
  .detail-odds.solo { grid-template-columns: 1fr; }
  .detail-odd { display: flex; flex-direction: column; gap: 5px; min-width: 0; padding: 9px; border: 1px solid var(--plain-line, var(--ground-10)); background: var(--plain-slab-2, var(--ground-7)); }
  .detail-odd span { color: var(--bone-3); }
  .detail-odd strong { color: var(--bone-13); font-size: 16px; font-weight: normal; font-variant-numeric: tabular-nums; overflow-wrap: anywhere; }
  .detail-odd strong.boosted { color: var(--gold-2); }
  .detail-hint { color: var(--bone-3); margin: -6px 0 14px; }
  .detail-label { color: var(--bone-13); padding-bottom: 6px; border-bottom: 1px solid var(--plain-sep, var(--ground-10)); }
  .detail-places { display: flex; flex-wrap: wrap; gap: 5px; padding-top: 9px; }
  .place { color: var(--bone-9); background: var(--plain-slab-2, var(--ground-7)); border: 1px solid var(--plain-line, var(--ground-10)); padding: 3px 6px; }
  .confirm-card { width: min(420px, calc(100% - 24px)); }
  .confirm-title { color: var(--bone-13); font-size: 17px; padding-right: 12px; }
  .confirm-description { color: var(--bone-6); padding: 10px 0 18px; line-height: 1.4; }
  .confirm-actions { display: flex; justify-content: flex-end; gap: 7px; }
  .confirm-actions .pick { min-width: 72px; }
  .import-card { width: min(640px, calc(100% - 24px)); }
  .import-label { display: block; color: var(--bone-9); margin: 8px 0 5px; }
  .import-source { display: flex; gap: 6px; flex-wrap: wrap; }
  .import-source .find { min-width: 220px; height: 26px; }
  .import-source .pick { height: 26px; }
  .import-preview { margin-top: 14px; padding: 10px; border: 1px solid var(--plain-line, var(--ground-10)); background: var(--plain-slab-2, var(--ground-7)); line-height: 1.5; }
  .import-source-name { color: var(--gold-2); overflow-wrap: anywhere; margin-bottom: 4px; }
  .import-skipped { color: var(--bone-3); max-height: 70px; overflow-y: auto; margin-top: 5px; overflow-wrap: anywhere; }
  .import-matches { display: flex; flex-wrap: wrap; gap: 4px 9px; max-height: 95px; overflow-y: auto; margin-top: 8px; padding: 5px 0; border-top: 1px solid var(--plain-sep, var(--ground-10)); border-bottom: 1px solid var(--plain-sep, var(--ground-10)); }
  .import-character { display: block; width: 100%; max-width: 350px; height: 28px; }
  .import-totals { color: var(--gold-2); margin-top: 10px; }
  .import-message { margin-top: 9px; }
  .import-actions { margin-top: 16px; }
  .c-sat { color: var(--rar-satanic); }
  .c-set { color: #40d040; }
  .c-her { color: #00ffae; }
  .c-ang { color: #f6f794; }
  .c-unh { color: #e04a7a; }
  @media (max-width: 680px) { .kind { display: none; } }
  @media (max-width: 640px) { .character-row { grid-template-columns: 30px minmax(0, 1fr) 26px auto; } .character-mode { display: none; } }
  @media (max-width: 440px) { .detail-odds { grid-template-columns: 1fr; } }
</style>

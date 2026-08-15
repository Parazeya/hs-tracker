<script>
  import { invoke } from '@tauri-apps/api/core';
  import { art } from './skin.svelte.js';
  import { listen } from '@tauri-apps/api/event';
  import { ITEMS, RARITY_BY_NAME, TIER_BY_NAME, DROP_RATE, tierLabel } from './items.js';
  import { soundUrl, play } from './audio.js';

  // only named items can be listed: an ordinary base has no identity of its own
  const NAMED = [
    ...new Map(
      Object.entries(ITEMS)
        .filter(([, name]) => RARITY_BY_NAME[name.toLowerCase()])
        .map(([key, name]) => [
          name,
          {
            name,
            type: Number(key.split(':')[0]),
            rarity: RARITY_BY_NAME[name.toLowerCase()],
            tier: TIER_BY_NAME[name.toLowerCase()] ?? 0,
            rate: DROP_RATE[name.toLowerCase()] ?? 0,
            key: name.toLowerCase(),
          },
        ]),
    ).values(),
  ].sort((a, b) => a.name.localeCompare(b.name));

  // what a character wears and carries. Orbs, vials, reagents and the like are
  // named too, but nobody wants a chime for a Goblin orb in a gear band — they
  // can still be added to a list by hand.
  const GEAR = new Set([0, 1, 2, 3, 4, 5, 6, 7, 8, 10]);

  const ALERT_RARITIES = ['Satanic', 'Set', 'Heroic', 'Angelic', 'Unholy'];
  const TIERS = [
    [0, 'any'],
    [1, 'D'],
    [2, 'C'],
    [3, 'B'],
    [4, 'A'],
    [5, 'S'],
    [6, 'SS'],
  ];
  const rarityCls = { Satanic: 'c-sat', Set: 'c-set', Heroic: 'c-her', Angelic: 'c-ang', Unholy: 'c-unh' };

  let settings = $state(null);
  let selected = $state(0);
  let query = $state('');
  let status = $state({});
  let saveTimer;

  let filters = $derived(settings?.filters ?? []);
  let filter = $derived(filters.find((f) => f.id === settings?.filter) ?? filters[0] ?? null);
  let lists = $derived(filter?.lists ?? []);
  let current = $derived(lists[selected] ?? null);
  let soundKey = $derived(current ? `list-${current.id}` : null);

  let matches = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (!q) return [];
    const owned = new Set((current?.items ?? []).map((n) => n.toLowerCase()));
    return NAMED.filter((it) => it.key.includes(q) && !owned.has(it.key)).slice(0, 40);
  });

  // An item in two lists is a conflict: only the first list's sound plays, and
  // the order of the lists decides which. Both the tab and the row say so.
  let clashes = $derived.by(() => {
    const owners = new Map();
    for (const list of lists) {
      for (const name of list.items) {
        const key = name.toLowerCase();
        owners.set(key, [...(owners.get(key) ?? []), list.name]);
      }
    }
    return new Map([...owners].filter(([, names]) => names.length > 1));
  });

  const clashesIn = (list) => list.items.filter((n) => clashes.has(n.toLowerCase())).length;
  const clashWith = (name) =>
    (clashes.get(name.toLowerCase()) ?? []).filter((n) => n !== current?.name).join(', ');

  // an item can sit in two lists, but only the first one's sound plays — so
  // say where else it is before it is added again
  let elsewhere = $derived.by(() => {
    const seen = new Map();
    for (const list of lists) {
      if (list === current) continue;
      for (const name of list.items) seen.set(name.toLowerCase(), list.name);
    }
    return seen;
  });

  // sorted by name, and narrowed by the same query that searches for new ones
  let shown = $derived.by(() => {
    const q = query.trim().toLowerCase();
    const items = [...(current?.items ?? [])].sort((a, b) => a.localeCompare(b));
    return q ? items.filter((n) => n.toLowerCase().includes(q)) : items;
  });

  $effect(() => {
    invoke('get_settings').then((s) => (settings = s));
    const unsubs = [
      listen('settings-changed', (e) => (settings = e.payload)),
      listen('sounds-changed', (e) => refreshStatus(e.payload)),
    ];
    return () => unsubs.forEach((u) => u.then((f) => f()));
  });

  let known = '';
  $effect(() => {
    const keys = lists.map((l) => l.id).join(',');
    if (keys === known) return;
    known = keys;
    for (const list of lists) refreshStatus(`list-${list.id}`);
  });

  async function refreshStatus(key) {
    status = { ...status, [key]: await invoke('sound_status', { rarity: key }).catch(() => null) };
  }

  function save() {
    clearTimeout(saveTimer);
    const snapshot = $state.snapshot(settings);
    saveTimer = setTimeout(() => invoke('save_settings', { settings: snapshot }).catch(() => {}), 150);
  }

  // "one in 576425" is true but unreadable in a row; "1/576k" is not
  function odds(rate) {
    if (!rate) return '';
    if (rate >= 1e6) return `1/${(rate / 1e6).toFixed(rate >= 1e7 ? 0 : 1)}M`;
    if (rate >= 1e3) return `1/${(rate / 1e3).toFixed(rate >= 1e4 ? 0 : 1)}k`;
    return `1/${rate}`;
  }

  const id = () => Math.random().toString(36).slice(2, 8);

  // Deleting a filter takes its lists and their sounds with it, and clearing a
  // list is just as final — so anything destructive asks once. The second click
  // does it; walking away forgets.
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

  function addFilter(name, lists = []) {
    const made = { id: id(), name, lists };
    settings.filters = [...filters, made];
    settings.filter = made.id;
    selected = 0;
    save();
    return made;
  }

  function removeFilter() {
    for (const list of lists) invoke('clear_sound', { rarity: `list-${list.id}` }).catch(() => {});
    settings.filters = filters.filter((f) => f.id !== filter.id);
    settings.filter = settings.filters[0]?.id ?? '';
    selected = 0;
    save();
  }

  // Angelic and Unholy drop under their own rules, so a drop-rate band would
  // put them next to items they have nothing in common with. They get a list
  // each instead.
  const APART = ['Angelic', 'Unholy'];

  const band = (name, items) => ({
    id: id(),
    name,
    enabled: true,
    volume: 0.7,
    items: items.map((it) => it.name),
  });

  // Drop rates are "one in N", so sorting by them splits a grade into the
  // items you see often, the ones you do not, and the chase pieces.
  function generate() {
    const bands = [];
    for (const [tier, letter] of [
      [5, 'S'],
      [6, 'SS'],
    ]) {
      const pool = NAMED.filter(
        (it) => it.tier === tier && it.rate > 0 && GEAR.has(it.type) && !APART.includes(it.rarity),
      ).sort((a, b) => a.rate - b.rate);
      if (pool.length < 3) continue;
      const cut = Math.ceil(pool.length / 3);
      for (const [n, name] of [[0, 'Common'], [1, 'Rare'], [2, 'VeryRare']]) {
        const slice = pool.slice(n * cut, (n + 1) * cut);
        if (slice.length) bands.push(band(`${letter}-${name}`, slice));
      }
    }
    for (const rarity of APART) {
      const own = NAMED.filter((it) => it.rarity === rarity && GEAR.has(it.type)).sort((a, b) =>
        a.name.localeCompare(b.name),
      );
      if (own.length) bands.push(band(rarity, own));
    }
    addFilter('Drop rate bands', bands);
  }

  /// The first list that matches wins, so the order is the priority.
  function moveList(step) {
    const to = selected + step;
    if (to < 0 || to >= lists.length) return;
    const next = [...lists];
    [next[selected], next[to]] = [next[to], next[selected]];
    filter.lists = next;
    selected = to;
    save();
  }

  function addList() {
    filter.lists = [...lists, { id: id(), name: `List ${lists.length + 1}`, enabled: true, volume: 0.7, items: [] }];
    selected = filter.lists.length - 1;
    save();
  }

  function removeList(i) {
    invoke('clear_sound', { rarity: `list-${lists[i].id}` }).catch(() => {});
    filter.lists = lists.filter((_, n) => n !== i);
    selected = Math.max(0, Math.min(selected, filter.lists.length - 1));
    save();
  }

  function addItem(name) {
    current.items = [...current.items, name].sort((a, b) => a.localeCompare(b));
    save();
  }

  /// Removes what the search is showing, or the whole list when it is not
  /// searching — the count on the button says which.
  function removeShown() {
    const gone = new Set(shown.map((n) => n.toLowerCase()));
    current.items = current.items.filter((n) => !gone.has(n.toLowerCase()));
    save();
  }

  let notice = $state('');
  let noticeTimer;
  function say(text) {
    notice = text;
    clearTimeout(noticeTimer);
    noticeTimer = setTimeout(() => (notice = ''), 4000);
  }

  async function exportFilter() {
    try {
      const name = await invoke('export_filter', { filter: $state.snapshot(filter) });
      if (name) say(`saved as ${name}`);
    } catch (e) {
      say(String(e));
    }
  }

  async function importFilter() {
    try {
      const imported = await invoke('import_filter');
      if (!imported) return;
      settings.filters = [...filters, imported];
      settings.filter = imported.id;
      selected = 0;
      save();
      say(`imported ${imported.name} — ${imported.lists.length} lists`);
    } catch (e) {
      say(String(e));
    }
  }

  function duplicateFilter() {
    addFilter(`${filter.name} copy`, lists.map((l) => ({ ...l, id: id(), items: [...l.items] })));
  }

  function removeItem(name) {
    current.items = current.items.filter((n) => n !== name);
    save();
  }

  function toggleAlert(rarity) {
    const on = new Set(settings.alerts ?? []);
    on.has(rarity) ? on.delete(rarity) : on.add(rarity);
    settings.alerts = [...on];
    save();
  }

  function setNumber(key, value) {
    if (!settings || !Number.isFinite(value) || settings[key] === value) return;
    settings[key] = value;
    save();
  }

  async function pickSound() {
    try {
      await invoke('pick_sound', { rarity: soundKey });
      refreshStatus(soundKey);
    } catch {}
  }

  async function test() {
    play(await soundUrl(soundKey), current?.volume ?? 0.7);
  }

</script>

<div class="panel">
  {#if settings}
    <div class="section" style:border-image-source="url({art('chip_dark')})">
      <div class="sechead" data-tauri-drag-region>Rarity alerts — what makes a sound at all</div>
      <div class="grid">
        {#each ALERT_RARITIES as rarity}
          <button class="secopt" onclick={() => toggleAlert(rarity)}>
            <img src={(settings.alerts ?? []).includes(rarity) ? art('check_on') : art('check_off')} alt="" />
            <span class={rarityCls[rarity]}>{rarity}</span>
          </button>
        {/each}
      </div>
      <div class="line">
        <span class="name">Min tier</span>
        <div class="tiers">
          {#each TIERS as [value, label]}
            <button class="tier" class:on={(settings.min_tier ?? 0) === value} onclick={() => setNumber('min_tier', value)}>
              {label}
            </button>
          {/each}
        </div>
      </div>
      <div class="note">
        Counters still record everything — this only silences the alerts. Grades come
        from the item tables, so an item they do not list stays quiet while a minimum
        tier is set. Finds the server announces always sound.
      </div>
    </div>

    <div class="section" style:border-image-source="url({art('chip_dark')})">
      <div class="sechead" data-tauri-drag-region>Custom filter — lists that outrank the above</div>

      <div class="line">
        <button class="check" onclick={() => { settings.use_filter = !settings.use_filter; save(); }} aria-label="use filter">
          <img src={settings.use_filter ? art('check_on') : art('check_off')} alt="" />
        </button>
        <span class="opt">Use the selected filter</span>
      </div>

      <div class="line">
        <select
          class="picker"
          value={filter?.id ?? ''}
          onchange={(e) => { settings.filter = e.currentTarget.value; selected = 0; save(); }}
        >
          {#each filters as f}
            <option value={f.id}>{f.name} · {f.lists.length} lists</option>
          {:else}
            <option value="">no filters yet</option>
          {/each}
        </select>
        <button class="btn" style:--btn="url({art('button')})" style:--btn-hover="url({art('button_hover')})" style:--btn-down="url({art('button_down')})" onclick={() => addFilter(`Filter ${filters.length + 1}`)}>New</button>
        <button class="btn" style:--btn="url({art('button')})" style:--btn-hover="url({art('button_hover')})" style:--btn-down="url({art('button_down')})" onclick={generate} title="Split S and SS gear into three bands by how rare their drop is">Generate</button>
        {#if filter}
          <button class="btn" style:--btn="url({art('button')})" style:--btn-hover="url({art('button_hover')})" style:--btn-down="url({art('button_down')})" onclick={duplicateFilter} title="Copy this filter, sounds and all">Copy</button>
        {/if}
        {#if filter}
          <button
            class="del"
            class:armed={armed === 'filter'}
            onclick={() => danger('filter', removeFilter)}
            title="Delete this filter with all its lists and sounds"
          >{armed === 'filter' ? 'delete?' : '×'}</button>
        {/if}
      </div>

      <div class="line">
        <button class="btn" style:--btn="url({art('button')})" style:--btn-hover="url({art('button_hover')})" style:--btn-down="url({art('button_down')})" onclick={importFilter} title="Load a filter someone shared with you, sounds included">Import…</button>
        {#if filter}
          <button class="btn" style:--btn="url({art('button')})" style:--btn-hover="url({art('button_hover')})" style:--btn-down="url({art('button_down')})" onclick={exportFilter} title="Save this filter to a file, sounds included">Export…</button>
        {/if}
        {#if notice}
          <span class="notice">{notice}</span>
        {/if}
      </div>

      {#if filter}
        <input
          class="field name"
          style:border-image-source="url({art('chip_dark')})"
          value={filter.name}
          oninput={(e) => { filter.name = e.currentTarget.value; save(); }}
        />
      {/if}
    </div>
  {/if}

  {#if filter}
    <div class="tabs">
      {#each lists as list, i}
        <button class="tab" class:on={i === selected} onclick={() => (selected = i)}>
          {list.name}
          {#if clashesIn(list)}
            <span class="clash" title="{clashesIn(list)} of these items are in another list too — only the list that comes first will sound">?</span>
          {/if}
          <span class="count">{list.items.length}</span>
        </button>
      {/each}
      <button class="btn add" style:--btn="url({art('button')})" style:--btn-hover="url({art('button_hover')})" style:--btn-down="url({art('button_down')})" onclick={addList}>+ list</button>
    </div>
  {/if}

  {#if current}
    <div class="head" style:border-image-source="url({art('chip_dark')})">
      <button class="check" onclick={() => { current.enabled = !current.enabled; save(); }} aria-label="enabled">
        <img src={current.enabled ? art('check_on') : art('check_off')} alt="" />
      </button>
      <input
        class="name"
        value={current.name}
        oninput={(e) => { current.name = e.currentTarget.value; save(); }}
      />
      <button class="move" disabled={selected === 0} onclick={() => moveList(-1)} title="Earlier — an earlier list wins a conflict">◀</button>
      <button class="move" disabled={selected === lists.length - 1} onclick={() => moveList(1)} title="Later">▶</button>
      <button
        class="del"
        class:armed={armed === 'list'}
        onclick={() => danger('list', () => removeList(selected))}
        title="Delete this list and its sound"
      >{armed === 'list' ? 'delete?' : '×'}</button>
    </div>

    <div class="sound" style:border-image-source="url({art('chip_dark')})">
      <span class="file">{status[soundKey] ?? 'no sound yet — the rarity alert plays instead'}</span>
      <input
        class="vol"
        type="range"
        min="0"
        max="1"
        step="0.05"
        value={current.volume}
        oninput={(e) => { current.volume = +e.currentTarget.value; save(); }}
      />
      <button class="btn" style:--btn="url({art('button')})" style:--btn-hover="url({art('button_hover')})" style:--btn-down="url({art('button_down')})" onclick={test}>Test</button>
      <button class="btn" style:--btn="url({art('button')})" style:--btn-hover="url({art('button_hover')})" style:--btn-down="url({art('button_down')})" onclick={pickSound}>Browse…</button>
    </div>

    <input
      class="field"
      style:border-image-source="url({art('chip_dark')})"
      placeholder="search to add, or to narrow the list below…"
      bind:value={query}
      onkeydown={(e) => e.key === 'Enter' && matches[0] && addItem(matches[0].name)}
    />

    {#if matches.length}
      <div class="listhead"><span>Not in this list</span></div>
    {/if}

    {#if matches.length}
      <div class="results" style:border-image-source="url({art('chip_dark')})">
        {#each matches as it}
          <button class="hit" onclick={() => addItem(it.name)}>
            <span class={rarityCls[it.rarity]}>{it.name}</span>
            {#if elsewhere.has(it.key)}
              <span class="already">in {elsewhere.get(it.key)}</span>
            {/if}
            <span class="grade">
              <span class="letter">{tierLabel(it.tier)}</span>
              <span class="odds">{odds(it.rate)}</span>
            </span>
          </button>
        {/each}
      </div>
    {/if}

    <div class="listhead">
      <span>Items in {current.name}</span>
      {#if shown.length}
        <button class="link" class:armed={armed === 'clear'} onclick={() => danger('clear', removeShown)}>
          {#if armed === 'clear'}
            {query.trim() ? `remove ${shown.length}?` : 'clear the list?'}
          {:else}
            {query.trim() ? `remove ${shown.length} shown` : 'clear'}
          {/if}
        </button>
      {/if}
      <span class="count">{query.trim() ? `${shown.length} of ${current.items.length}` : current.items.length}</span>
    </div>

    <div class="items">
      {#each shown as name}
        <div class="row {rarityCls[RARITY_BY_NAME[name.toLowerCase()]] ?? ''}">
          <span class={rarityCls[RARITY_BY_NAME[name.toLowerCase()]] ?? ''}>{name}</span>
          {#if clashWith(name)}
            <span class="clash" title="also in {clashWith(name)}">?</span>
          {/if}
          <span class="grade">
            <span class="letter">{tierLabel(TIER_BY_NAME[name.toLowerCase()] ?? 0)}</span>
            <span class="odds">{odds(DROP_RATE[name.toLowerCase()] ?? 0)}</span>
          </span>
          <button class="del" onclick={() => removeItem(name)} title="Remove" aria-label="remove">×</button>
        </div>
      {:else}
        <div class="empty">
          {query.trim() ? 'nothing in this list matches the search' : 'nothing listed yet — search above and click an item to add it'}
        </div>
      {/each}
    </div>
  {:else if filter}
    <div class="empty">this filter has no lists yet — press “+ list”</div>
  {:else if settings}
    <div class="empty">
      no filters yet — press “New” for an empty one, or “Generate” to build S and SS
      bands from the drop rates: the items you see often, the ones you do not, and
      the chase pieces, each ready for a sound of its own.
    </div>
  {/if}
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
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-family: 'CookieRun Bold', sans-serif;
    font-size: 12px;
    color: var(--bone-6);
    /* the sections stack up; when they outgrow the window the whole pane
       scrolls, so the item list never has to be squeezed to nothing */
    overflow-y: auto;
    padding-right: 2px;
  }
  .panel::-webkit-scrollbar { width: 6px; }
  .panel::-webkit-scrollbar-thumb { background: var(--dim-1); border-radius: 3px; }

  .section,
  .head,
  .sound,
  .results {
    box-sizing: border-box;
    border: 6px solid transparent;
    border-image-slice: 6 fill;
    border-image-width: 6px;
    image-rendering: pixelated;
  }

  .section {
    flex: none;
    padding: 4px 6px 6px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .sechead {
    color: var(--edge-2b);
    font-size: 10px;
    letter-spacing: 0.3px;
    text-transform: uppercase;
  }

  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 2px 10px;
  }

  .secopt,
  .check {
    display: flex;
    align-items: center;
    gap: 6px;
    font: inherit;
    color: inherit;
    background: none;
    border: none;
    padding: 2px 0;
    cursor: pointer;
    text-align: left;
  }
  .secopt img { width: 16px; height: 16px; }
  .check { flex: none; padding: 0; }
  .check img { width: 18px; height: 18px; }

  .line {
    display: flex;
    align-items: center;
    gap: 6px;
    min-height: 24px;
  }
  .line .name { flex: none; }
  .opt { flex: 1 1 auto; }

  .tiers { display: flex; gap: 3px; margin-left: auto; }
  .tier {
    font: inherit;
    font-size: 11px;
    color: var(--bone-3);
    background: rgba(0, 0, 0, 0.25);
    border: 1px solid var(--ground-10);
    padding: 2px 7px;
    cursor: pointer;
  }
  .tier.on { color: var(--bone-13); border-color: var(--edge-4); background: rgba(150, 37, 56, 0.45); }

  .note { color: var(--dim-2); font-size: 10px; line-height: 1.4; }
  .notice {
    flex: 1 1 auto;
    min-width: 0;
    color: #45c15a;
    font-size: 10px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* WebView2 leaves a select alone; WebKitGTK draws it as a native widget with
     a pale background and a blue focus ring, which is a hole in the panel. The
     appearance is taken over completely, arrow included. */
  .picker {
    flex: 1 1 auto;
    min-width: 0;
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
  .picker:focus,
  .picker:focus-visible {
    outline: none;
    border-color: var(--edge-4);
  }
  /* the popup list is the toolkit's own window; these are the only two
     properties it honours */
  .picker option {
    background: var(--ground-7);
    color: var(--bone-9);
  }

  .tabs {
    flex: none;
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .tab {
    font: inherit;
    font-size: 11px;
    color: var(--bone-3);
    background: rgba(0, 0, 0, 0.25);
    border: 1px solid var(--ground-10);
    padding: 3px 7px;
    cursor: pointer;
  }
  .tab.on { color: var(--bone-13); border-color: var(--edge-4); background: rgba(150, 37, 56, 0.35); }
  .tab .count { color: var(--edge-5); margin-left: 4px; }
  .clash {
    color: var(--gold-1);
    font-size: 11px;
    margin-left: 4px;
    cursor: help;
  }

  .move {
    flex: none;
    font: inherit;
    font-size: 10px;
    color: var(--bone-3);
    background: rgba(0, 0, 0, 0.25);
    border: 1px solid var(--ground-10);
    padding: 2px 5px;
    cursor: pointer;
  }
  .move:hover:not(:disabled) { color: var(--bone-13); border-color: var(--edge-4); }
  .move:disabled { opacity: 0.35; cursor: default; }

  .head,
  .sound {
    flex: none;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 2px 6px;
    min-height: 28px;
  }

  input.name {
    flex: 1 1 auto;
    min-width: 0;
    font: inherit;
    color: var(--bone-13);
    background: none;
    border: none;
    outline: none;
  }

  .file { flex: 1 1 auto; min-width: 0; color: var(--dim-2); font-size: 11px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  /* drawn by us, like the sliders on the other panels: an engine left to its
     own devices renders a different control on every platform */
  .vol {
    flex: none;
    width: 74px;
    height: 14px;
    appearance: none;
    -webkit-appearance: none;
    background: none;
    cursor: pointer;
  }
  .vol::-webkit-slider-runnable-track {
    height: 4px;
    background: var(--ground-7);
    border: 1px solid var(--ground-11);
  }
  .vol::-webkit-slider-thumb {
    -webkit-appearance: none;
    width: 11px;
    height: 11px;
    margin-top: -5px;
    background: var(--bone-6);
    border: 1px solid var(--ground-7);
  }
  .vol:hover::-webkit-slider-thumb { background: var(--bone-13); }

  .field {
    flex: none;
    box-sizing: border-box;
    height: 26px;
    border: 6px solid transparent;
    border-image-slice: 6 fill;
    border-image-width: 6px;
    image-rendering: pixelated;
    font: inherit;
    color: var(--bone-13);
    background: none;
    outline: none;
    padding: 0 6px;
  }

  .results {
    flex: none;
    max-height: 150px;
    overflow-y: auto;
    padding: 2px;
    display: flex;
    flex-direction: column;
  }
  .hit {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    font: inherit;
    font-size: 11px;
    color: inherit;
    background: none;
    border: none;
    text-align: left;
    padding: 3px 5px;
    cursor: pointer;
  }
  .hit:hover { background: rgba(150, 37, 56, 0.45); }
  .already { margin-left: auto; color: var(--edge-2b); font-size: 10px; }

  .items {
    flex: 1 1 auto;
    min-height: 170px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding-right: 2px;
  }
  .items::-webkit-scrollbar,
  .results::-webkit-scrollbar { width: 6px; }
  .items::-webkit-scrollbar-thumb,
  .results::-webkit-scrollbar-thumb { background: var(--dim-1); border-radius: 3px; }

  .listhead {
    flex: none;
    display: flex;
    align-items: baseline;
    gap: 6px;
    margin-top: 2px;
    padding: 0 2px 2px;
    border-bottom: 1px solid var(--ground-10);
    color: var(--edge-2b);
    font-size: 10px;
    letter-spacing: 0.3px;
    text-transform: uppercase;
  }
  .listhead .count { margin-left: auto; color: var(--edge-5); }
  .link {
    font: inherit;
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.3px;
    color: var(--bone-3);
    background: none;
    border: none;
    padding: 0;
    cursor: pointer;
  }
  .link:hover { color: var(--bone-13); }

  /* flat rows with a rarity edge: unmistakably contents, not controls */
  .row {
    flex: none;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 8px 4px 6px;
    min-height: 24px;
    background: rgba(0, 0, 0, 0.22);
    border-left: 3px solid var(--ground-10);
  }
  .row:nth-child(even) { background: rgba(0, 0, 0, 0.12); }
  .row:hover { background: rgba(150, 37, 56, 0.22); }
  .row.c-sat { border-left-color: #d24b4b; }
  .row.c-set { border-left-color: #45c15a; }
  .row.c-her { border-left-color: #35d3c1; }
  .row.c-ang { border-left-color: var(--gold-1); }
  .row.c-unh { border-left-color: #e04a7a; }
  .row span:first-child { flex: 1 1 auto; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

  .grade {
    flex: none;
    display: inline-flex;
    align-items: center;
    gap: 8px;
    color: var(--dim-2);
    font-size: 10px;
  }
  .letter { min-width: 16px; text-align: right; }
  .odds {
    min-width: 48px;
    text-align: right;
    color: var(--edge-5);
    font-variant-numeric: tabular-nums;
  }

  .del {
    flex: none;
    font: inherit;
    font-size: 14px;
    color: var(--edge-1b);
    background: none;
    border: none;
    cursor: pointer;
    padding: 0 2px;
  }
  .del:hover { color: #e05a5a; }
  .del.armed,
  .link.armed {
    color: #f0c0c0;
    background: rgba(180, 30, 30, 0.55);
    font-size: 10px;
    padding: 2px 6px;
  }

  .empty {
    color: var(--dim-2);
    text-align: center;
    font-size: 11px;
    line-height: 1.5;
    padding: 12px 8px;
  }

  .btn {
    box-sizing: border-box;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    line-height: 1;
    height: 26px;
    flex: none;
    font: inherit;
    font-size: 11px;
    color: var(--bone-13);
    border: 6px solid transparent;
    border-image-source: var(--btn);
    border-image-slice: 6 fill;
    border-image-width: 6px;
    image-rendering: pixelated;
    padding: 0 8px;
    cursor: pointer;
  }
  .btn:hover { border-image-source: var(--btn-hover); }
  .btn:active { border-image-source: var(--btn-down); }
  .btn.add { height: 24px; font-size: 10px; }

  .c-sat { color: #d24b4b; }
  .c-set { color: #45c15a; }
  .c-her { color: #35d3c1; }
  .c-ang { color: var(--gold-1); }
  .c-unh { color: #e04a7a; }
</style>

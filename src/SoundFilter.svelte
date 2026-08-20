<script>
  import { invoke } from './bridge.js';
  import { art } from './skin.svelte.js';
  import { listen } from './bridge.js';
  import { ITEMS, RARITY_BY_NAME, TIER_BY_NAME, DROP_RATE, tierLabel } from './items.js';
  import { RARITIES, soundUrl, play } from './audio.js';
  import { ALL_BUFFS, ALL_BUFF_IDS } from './buffs.js';

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

  // The same five rarities used to be configured in two places — whether they
  // alert at all here, and how loud and with which file one tab away. They are
  // one question and are now asked once, on one row each.
  const SOUND_KEY = { Satanic: 'satanic', Set: 'set', Heroic: 'heroic', Angelic: 'angelic', Unholy: 'unholy' };
  let custom = $state({});

  async function refreshSounds() {
    const next = {};
    for (const r of RARITIES) next[r] = await invoke('sound_status', { rarity: r }).catch(() => null);
    custom = next;
  }

  const testRarity = async (key) => play(await soundUrl(key), settings?.[key]?.volume ?? 0.7);

  async function pickRaritySound(key) {
    try {
      await invoke('pick_sound', { rarity: key });
      refreshSounds();
    } catch {}
  }

  let mailVolume = $derived(Math.round((settings?.mail?.volume ?? 0.7) * 100));
  let zoneVolume = $derived(Math.round((settings?.zone?.volume ?? 0.7) * 100));

  // The announcement lives here rather than in Settings: it answers the same
  // question the rest of this page answers — what is worth telling you about —
  // and asking it two tabs away is what made it look as though it did nothing.
  let session = $state(null);
  let overlay = $derived(session?.overlay ?? false);
  // With the browser sources gone the announcement is a window and nothing
  // else, so it is offered exactly where a window can be put on top of the
  // game — and a Wayland session, which cannot, is told so rather than shown
  // switches that do nothing.
  let canAnnounce = $derived(overlay);
  const FX_TIERS = ['D', 'C', 'B', 'A', 'S', 'SS'];
  let scalePct = $state(100);
  let shadePct = $state(55);
  $effect(() => {
    invoke('session_info')
      .then((s) => (session = s))
      .catch(() => (session = { overlay: true, wayland: false, through_x11: false, can_switch: false }));
  });
  $effect(() => {
    if (!settings) return;
    scalePct = Math.round((settings.flourish_scale ?? 1) * 100);
    shadePct = Math.round((settings.flourish_shade ?? 0.55) * 100);
  });

  function toggleFlourish(name) {
    const on = new Set(settings.flourish_rarities ?? []);
    on.has(name) ? on.delete(name) : on.add(name);
    settings.flourish_rarities = [...on];
    save();
  }

  function toggleBuff(id) {
    const on = new Set(settings.zone_buffs ?? ALL_BUFF_IDS);
    on.has(id) ? on.delete(id) : on.add(id);
    settings.zone_buffs = [...on];
    save();
  }

  function selectAllBuffs() {
    settings.zone_buffs = [...ALL_BUFF_IDS];
    save();
  }

  function selectNoBuffs() {
    settings.zone_buffs = [];
    save();
  }

  function setVolume(key, v) {
    if (!settings?.[key] || settings[key].volume === v) return;
    settings[key].volume = v;
    save();
  }

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

  /// Clear removes what the search is showing, so what it would remove changes
  /// with the search as well as with the list. Both belong in the key that
  /// arms it, or the confirmation is for one set and the deletion for another.
  let clearKey = $derived(`clear:${current?.id}:${query.trim().toLowerCase()}`);
  // The key names what is armed, and the template has to compare against the
  // same key it armed with. Comparing against a bare 'filter' or 'list' while
  // arming with `filter:<id>` was always false: the button never turned red,
  // never read "delete?", and the first click looked like nothing had
  // happened — on the two controls that destroy a whole filter or list.
  let filterKey = $derived(`filter:${filter?.id}`);
  let listKey = $derived(`list:${current?.id}`);

  // sorted by name, and narrowed by the same query that searches for new ones
  let shown = $derived.by(() => {
    const q = query.trim().toLowerCase();
    const items = [...(current?.items ?? [])].sort((a, b) => a.localeCompare(b));
    return q ? items.filter((n) => n.toLowerCase().includes(q)) : items;
  });

  $effect(() => {
    invoke('get_settings').then((s) => (settings = s));
    refreshSounds();
    const unsubs = [
      listen('settings-changed', (e) => (settings = e.payload)),
      listen('sounds-changed', (e) => {
        refreshStatus(e.payload);
        refreshSounds();
      }),
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

  /// The answer is written after it arrives, never around it. Written as
  /// `status = { ...status, [key]: await … }` the spread is evaluated before
  /// the await — so eight lists asked at once each copied the same empty map
  /// and the last reply overwrote the other seven. An imported filter then
  /// claimed to have no sounds while its files sat on disk and Test played
  /// them.
  async function refreshStatus(key) {
    const name = await invoke('sound_status', { rarity: key }).catch(() => null);
    status[key] = name;
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
  //
  // The key has to name what is armed, not just which button. Keyed by button
  // alone, arming the delete on one list and then picking another left the
  // second one armed without ever having been asked about: one click, gone.
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

  /// The wait here is as long as the player takes to find the file, and the
  /// app stays usable throughout — so the settings this panel was holding when
  /// the dialog opened may be old news by the time it closes. The import is
  /// added to what is on disk now, not to the copy in hand, and saved outright
  /// rather than through the debounce: the listener above brings the panel
  /// back into step.
  async function importFilter() {
    try {
      const imported = await invoke('import_filter');
      if (!imported) return;
      const base = (await invoke('get_settings').catch(() => null)) ?? $state.snapshot(settings);
      base.filters = [...(base.filters ?? []), imported];
      base.filter = imported.id;
      clearTimeout(saveTimer);
      saveTimer = null;
      await invoke('save_settings', { settings: base });
      settings = base;
      selected = 0;
      say(`imported ${imported.name} — ${imported.lists.length} lists`);
    } catch (e) {
      say(String(e));
    }
  }

  /// A copy gets fresh list ids so it cannot fight with the original — and a
  /// list's sound is a file named after its id, so the copy has to be given
  /// one of its own too. Without that the copy came out mute while the button
  /// said "sounds and all".
  function duplicateFilter() {
    const pairs = lists.map((l) => [l.id, id()]);
    const made = addFilter(
      `${filter.name} copy`,
      lists.map((l, i) => ({ ...l, id: pairs[i][1], items: [...l.items] })),
    );
    for (const [from, to] of pairs) {
      invoke('copy_sound', { from: `list-${from}`, to: `list-${to}` })
        .then(() => refreshStatus(`list-${to}`))
        .catch(() => {});
    }
    return made;
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

  /// The volume is read before the wait, not after it. `play(await …, current.volume)`
  /// reads `current` only once the file has loaded, and by then the player may
  /// have picked another list — so one list's sound played at another's volume.
  async function test() {
    const volume = current?.volume ?? 0.7;
    play(await soundUrl(soundKey), volume);
  }

</script>

<div class="panel two">
  <div class="col rules">
  {#if settings}
    <div class="section" style:border-image-source="url({art('chip_dark')})">
      <div class="sechead" data-tauri-drag-region>Rarity alerts — what makes a sound at all</div>
      {#each ALERT_RARITIES as rarity}
        {@const key = SOUND_KEY[rarity]}
        {@const on = (settings.alerts ?? []).includes(rarity)}
        {@const vol = Math.round((settings[key]?.volume ?? 0.7) * 100)}
        <div class="rrow" class:off={!on}>
          <button class="check" onclick={() => toggleAlert(rarity)} aria-label={rarity}>
            <img src={on ? art('check_on') : art('check_off')} alt="" />
          </button>
          <span class="rname {rarityCls[rarity]}">{rarity}</span>
          <input
            class="vol"
            type="range"
            min="0"
            max="100"
            disabled={!on}
            value={vol}
            oninput={(e) => setVolume(key, e.currentTarget.value / 100)}
          />
          <span class="pct">{vol}%</span>
          <span class="src" title={custom[key] ? `sounds/${custom[key]}` : 'built-in sound'}>
            {custom[key] ?? 'built-in'}
          </span>
          <div class="rbtns">
            <button class="btn sm" style:--btn="url({art('button')})" style:--btn-hover="url({art('button_hover')})" style:--btn-down="url({art('button_down')})" onclick={() => testRarity(key)}>Test</button>
            <button class="btn sm" style:--btn="url({art('button')})" style:--btn-hover="url({art('button_hover')})" style:--btn-down="url({art('button_down')})" onclick={() => pickRaritySound(key)}>Browse…</button>
            <button
              class="btn sm"
              style:--btn="url({art('button')})"
              style:--btn-hover="url({art('button_hover')})"
              style:--btn-down="url({art('button_down')})"
              disabled={!custom[key]}
              onclick={() => danger(`snd-${key}`, () => invoke('clear_sound', { rarity: key }).catch(() => {}))}
            >{armed === `snd-${key}` ? 'Sure?' : 'Default'}</button>
          </div>
        </div>
      {/each}

      <!-- not a drop, but it is a sound and it lived on the tab that went away -->
      <div class="rrow" class:off={!settings.mail?.enabled}>
        <button class="check" onclick={() => { settings.mail.enabled = !settings.mail.enabled; save(); }} aria-label="mail">
          <img src={settings.mail?.enabled ? art('check_on') : art('check_off')} alt="" />
        </button>
        <span class="rname c-gold">Mail</span>
        <input
          class="vol"
          type="range"
          min="0"
          max="100"
          disabled={!settings.mail?.enabled}
          value={mailVolume}
          oninput={(e) => setVolume('mail', e.currentTarget.value / 100)}
        />
        <span class="pct">{mailVolume}%</span>
        <span class="src" title={custom.mail ? `sounds/${custom.mail}` : 'built-in sound'}>{custom.mail ?? 'built-in'}</span>
        <div class="rbtns">
          <button class="btn sm" style:--btn="url({art('button')})" style:--btn-hover="url({art('button_hover')})" style:--btn-down="url({art('button_down')})" onclick={() => testRarity('mail')}>Test</button>
          <button class="btn sm" style:--btn="url({art('button')})" style:--btn-hover="url({art('button_hover')})" style:--btn-down="url({art('button_down')})" onclick={() => pickRaritySound('mail')}>Browse…</button>
          <button
            class="btn sm"
            style:--btn="url({art('button')})"
            style:--btn-hover="url({art('button_hover')})"
            style:--btn-down="url({art('button_down')})"
            disabled={!custom.mail}
            onclick={() => danger('snd-mail', () => invoke('clear_sound', { rarity: 'mail' }).catch(() => {}))}
          >{armed === 'snd-mail' ? 'Sure?' : 'Default'}</button>
        </div>
      </div>
      <!-- Also not a drop: the game moving the satanic zone, which is the one
           thing on the overlay worth leaving a fight for. One switch, because
           the chime and the chip's pulse are one alert told two ways — see
           App.svelte. -->
      <div class="rrow" class:off={!settings.zone?.enabled}>
        <button class="check" onclick={() => { settings.zone.enabled = !settings.zone.enabled; save(); }} aria-label="satanic zone change">
          <img src={settings.zone?.enabled ? art('check_on') : art('check_off')} alt="" />
        </button>
        <span class="rname c-sat" title="The satanic zone rotating: the chime, and the zone chip pulsing on the overlay">Zone change</span>
        <input
          class="vol"
          type="range"
          min="0"
          max="100"
          disabled={!settings.zone?.enabled}
          value={zoneVolume}
          oninput={(e) => setVolume('zone', e.currentTarget.value / 100)}
        />
        <span class="pct">{zoneVolume}%</span>
        <span class="src" title={custom.zone ? `sounds/${custom.zone}` : 'built-in sound — the satanic chime'}>{custom.zone ?? 'built-in'}</span>
        <div class="rbtns">
          <button class="btn sm" style:--btn="url({art('button')})" style:--btn-hover="url({art('button_hover')})" style:--btn-down="url({art('button_down')})" onclick={() => testRarity('zone')}>Test</button>
          <button class="btn sm" style:--btn="url({art('button')})" style:--btn-hover="url({art('button_hover')})" style:--btn-down="url({art('button_down')})" onclick={() => pickRaritySound('zone')}>Browse…</button>
          <button
            class="btn sm"
            style:--btn="url({art('button')})"
            style:--btn-hover="url({art('button_hover')})"
            style:--btn-down="url({art('button_down')})"
            disabled={!custom.zone}
            onclick={() => danger('snd-zone', () => invoke('clear_sound', { rarity: 'zone' }).catch(() => {}))}
          >{armed === 'snd-zone' ? 'Sure?' : 'Default'}</button>
        </div>
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
    </div>

    <div class="section" style:border-image-source="url({art('chip_dark')})">
      <div class="sechead sec-split" data-tauri-drag-region>
        <span>Satanic zone buffs — alert when these appear</span>
        <div class="sec-actions">
          <button class="link" onclick={selectAllBuffs}>all</button>
          <span class="sep">·</span>
          <button class="link" onclick={selectNoBuffs}>none</button>
        </div>
      </div>
      <div class="grid buff-grid">
        {#each ALL_BUFFS as b}
          {@const on = (settings.zone_buffs ?? ALL_BUFF_IDS).includes(b.id)}
          <button
            class="secopt buff-opt"
            class:off={!on}
            onclick={() => toggleBuff(b.id)}
            title="{b.name} : {b.desc}"
          >
            <img class="check-img" src={on ? art('check_on') : art('check_off')} alt="" />
            <img class="buff-img" src={b.icon} alt="" />
            <span class="buff-name">{b.name}</span>
          </button>
        {/each}
      </div>
    </div>

    {#if canAnnounce}
      <div class="section" style:border-image-source="url({art('chip_dark')})">
        <div class="sechead" data-tauri-drag-region>Announcement — the loot pillar over the screen</div>
        <div class="line">
          <button class="check" onclick={() => { settings.flourish = !settings.flourish; save(); }} aria-label="flourish">
            <img src={settings.flourish ? art('check_on') : art('check_off')} alt="" />
          </button>
          <span class="opt" title="The game's own loot pillar, played over the screen where you put it">
            Announce a drop with the game's loot pillar
          </span>
        </div>

        {#if settings.flourish}
          <div class="line">
            <button
              class="check"
              onclick={() => { settings.flourish_listed = !settings.flourish_listed; save(); }}
              aria-label="follow the filter"
            >
              <img src={settings.flourish_listed ? art('check_on') : art('check_off')} alt="" />
            </button>
            <span class="opt" title="Anything on a list of the selected filter is announced, whatever its rarity or grade">
              Announce everything the custom filter lists
            </span>
          </div>
          {#if settings.flourish_listed && !settings.use_filter}
            <div class="note warn">
              The custom filter is switched off below, so this does nothing yet.
            </div>
          {/if}

          <div class="line">
            <button
              class="check"
              onclick={() => { settings.flourish_zone = !(settings.flourish_zone ?? true); save(); }}
              aria-label="announce satanic zone"
            >
              <img src={(settings.flourish_zone ?? true) ? art('check_on') : art('check_off')} alt="" />
            </button>
            <span class="opt" title="Announce the Satanic Zone and its buffs with the loot pillar when it rotates">
              Announce Satanic Zone Buffs
            </span>
          </div>

          <div class="grid">
            {#each ALERT_RARITIES as name}
              <button class="secopt" onclick={() => toggleFlourish(name)}>
                <img src={(settings.flourish_rarities ?? []).includes(name) ? art('check_on') : art('check_off')} alt="" />
                <span class={rarityCls[name]}>{name}</span>
              </button>
            {/each}
          </div>
          <div class="line">
            <span class="name">Min Tier</span>
            <input type="range" min="1" max="6" bind:value={settings.flourish_tier} oninput={() => save()} />
            <span class="pct">{FX_TIERS[(settings.flourish_tier ?? 6) - 1]}</span>
          </div>
          <div class="line">
            <span class="name">Size</span>
            <input type="range" min="50" max="200" bind:value={scalePct} oninput={() => setNumber('flourish_scale', scalePct / 100)} />
            <span class="pct">{Math.round((settings.flourish_scale ?? 1) * 100)}%</span>
          </div>
          <div class="line">
            <span class="name">Duration</span>
            <input type="range" min="2" max="12" step="0.5" bind:value={settings.flourish_secs} oninput={() => save()} />
            <span class="pct">{(settings.flourish_secs ?? 6).toFixed(1)}s</span>
          </div>
          <div class="line">
            <span class="name">Shading</span>
            <input type="range" min="0" max="90" bind:value={shadePct} oninput={() => setNumber('flourish_shade', shadePct / 100)} />
            <span class="pct">{Math.round((settings.flourish_shade ?? 0.55) * 100)}%</span>
          </div>

          {#if overlay}
            <div class="line">
              <button class="check" onclick={() => { settings.flourish_always = !settings.flourish_always; save(); }} aria-label="flourish always">
                <img src={settings.flourish_always ? art('check_on') : art('check_off')} alt="" />
              </button>
              <span class="opt" title="It draws nothing between drops, but OBS can only capture a window that is there">
                Keep its window on screen so OBS can capture it
              </span>
            </div>
            <div class="line">
              <button
                class="btn wide"
                style:--btn="url({art('button')})"
                style:--btn-hover="url({art('button_hover')})"
                style:--btn-down="url({art('button_down')})"
                onclick={() => invoke('place_flourish', { placing: true })}
              >
                Place it on the screen…
              </button>
            </div>
          {/if}
        {/if}
      </div>
    {/if}

  {/if}
  </div>

  <div class="col detail">
  {#if settings}
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
            class:armed={armed === filterKey}
            onclick={() => danger(filterKey, removeFilter)}
            title="Delete this filter with all its lists and sounds"
          >{armed === filterKey ? 'delete?' : '×'}</button>
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
        class:armed={armed === listKey}
        onclick={() => danger(listKey, () => removeList(selected))}
        title="Delete this list and its sound"
      >{armed === listKey ? 'delete?' : '×'}</button>
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
        <button class="link" class:armed={armed === clearKey} onclick={() => danger(clearKey, removeShown)}>
          {#if armed === clearKey}
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
</div>

<style>
  /* Two columns: what alerts on the left, the filter and its lists on the
     right. One screen, because it is one decision — and the page had grown
     long enough that a rarity's sound and the list that overrides it could
     not be seen at the same time. A narrow window falls back to one column,
     where side by side would leave neither half readable. */
  .panel.two {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: 10px;
    align-items: start;
  }
  /* The query is on the viewport, but the columns live in the dashboard pane —
     186px narrower once the frame, the 116px nav and the pane's own border and
     padding are taken out. Switched on at 900px of window the left column was
     360px and a rarity row wants 449, so the page came out with two nested
     horizontal scrollbars — worse than the single column it had just left, and
     a 960px half-screen snap landed in the middle of it. 1200 is measured: it
     is where a rarity row finally fits, on one line, in the column it is
     given. Under it the single column is the better page, not the fallback. */
  @media (min-width: 1200px) {
    .panel.two { grid-template-columns: minmax(360px, 1fr) minmax(380px, 1.1fr); }
    /* A column owns its scrolling only while the two are side by side. Stacked,
       `max-height: 100%` cut each of them to half the page with the page itself
       unable to scroll, so both halves were read through a slit. */
    .col { overflow-y: auto; max-height: 100%; }
  }
  .col { display: flex; flex-direction: column; gap: 8px; min-width: 0; }
  /* A flex child shrinks below its own content by default, and in a column
     that is a short window drawing every box on top of the one above it. The
     filter's name field, its buttons and the checkbox above them all landed in
     the same twenty pixels. Nothing here may shrink; if the window is too short
     the column scrolls instead. */
  .col > * { flex: 0 0 auto; }


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

  /* One rarity, one row: whether it alerts, how loud, and which file. The
     name column is fixed so the sliders line up — figures that do not share a
     left edge cannot be compared at a glance. */
  .rrow {
    display: grid;
    grid-template-columns: 18px 62px minmax(44px, 1fr) 34px minmax(90px, 1.4fr) auto;
    align-items: center;
    gap: 6px;
    padding: 1px 0;
  }
  /* The three buttons were three `auto` tracks, and `auto` does not give: the
     row's own minimum was 427px inside a 360px column, so the file name was
     squeezed to nothing and "Default" was drawn outside the panel. As one cell
     they wrap onto a second line instead, and the file name — the only thing
     that says built-in or picked — keeps a floor of its own. */
  .rbtns {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 6px;
  }
  .rrow.off .vol,
  .rrow.off .pct,
  .rrow.off .src { opacity: 0.45; }
  .rname { font-size: 12px; }

  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 2px 10px;
  }
  .buff-grid {
    gap: 2px 8px;
  }
  .secopt {
    display: flex;
    align-items: center;
    gap: 6px;
    font: inherit;
    font-size: 11px;
    color: var(--bone-6);
    background: none;
    border: none;
    cursor: pointer;
    padding: 1px 0;
    text-align: left;
  }
  .secopt img { width: 19px; height: 19px; flex: none; }
  .secopt:hover { color: var(--bone-13); }
  .buff-opt {
    gap: 5px;
    min-width: 0;
  }
  .buff-opt.off { opacity: 0.5; }
  .buff-opt .check-img { width: 16px; height: 16px; flex: none; }
  .buff-opt .buff-img { width: 18px; height: 18px; flex: none; }
  .buff-opt .buff-name {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 11px;
  }
  .sec-split {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  .sec-actions {
    display: flex;
    gap: 4px;
    align-items: center;
  }
  .sec-actions .sep {
    color: var(--edge-2b);
    font-size: 10px;
  }

  input[type='range'] {
    flex: 1 1 auto;
    max-width: 260px;
    height: 14px;
    appearance: none;
    -webkit-appearance: none;
    background: none;
    cursor: pointer;
  }
  input[type='range']::-webkit-slider-runnable-track {
    height: 4px;
    background: var(--ground-7);
    border: 1px solid var(--ground-11);
  }
  input[type='range']::-webkit-slider-thumb {
    -webkit-appearance: none;
    width: 11px;
    height: 11px;
    margin-top: -5px;
    background: var(--bone-6);
    border: 1px solid var(--ground-7);
  }
  input[type='range']:hover:not(:disabled)::-webkit-slider-thumb { background: var(--bone-13); }
  input[type='range']:disabled { opacity: 0.4; cursor: default; }

  .btn.wide { width: 100%; max-width: 380px; }

  /* a setting that is on but cannot act yet says so where it is set */
  .note.warn { color: var(--gold, #e8c860); }
  .vol { width: 100%; min-width: 44px; }
  .pct {
    font-size: 11px;
    color: var(--edge-2b);
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .src {
    font-size: 11px;
    color: var(--edge-2b);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
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
  /* Six rows carry three of these each; at full size they alone were 194px of
     a row that had 360 to live in. The class was on the buttons from the start
     and never had a rule. */
  .btn.sm { height: 22px; font-size: 10px; padding: 0 4px; }

  .c-sat { color: #d24b4b; }
  .c-set { color: #45c15a; }
  .c-her { color: #35d3c1; }
  .c-ang { color: var(--gold-1); }
  .c-unh { color: #e04a7a; }
</style>

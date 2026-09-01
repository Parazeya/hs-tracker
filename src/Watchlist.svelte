<script>
  import { invoke, listen } from './bridge.js';
  import { art } from './skin.svelte.js';
  import { BY_ID, ITEMS, RARITY_BY_NAME, TIER_BY_NAME, DROP_RATE, tierLabel } from './items.js';
  import { locale, nameOf, say, t, typeLabel } from './say.svelte.js';
  import { soundUrl, play } from './audio.js';

  // Only named items can be listed. The parser leaves an ordinary pickup
  // nameless, so a list holding a base has nothing to match and never chimes.
  //
  // Being on one of the five journal rarities is what "named" means here. "Has
  // a rarity" does not: the tables carry one for every item, bases included.
  const LISTABLE = new Set(['Satanic', 'Set', 'Heroic', 'Angelic', 'Unholy']);

  // Vaults are numbered by identity rather than by a slot in a base table, so
  // a drop of one always arrives named — which is what makes all seven
  // listable, where an ordinary item outside the five never arrives named at
  // all and could not fire.
  const VAULT = 19;

  // Two items can wear one name — a Set gun and a Heroic orb are both "Angel" —
  // and the seven Essence Vaults share theirs across every rarity. A list keyed
  // on the name alone fires for all of them and says nothing about which
  // dropped, so where the tables answer by identity each is offered on its own
  // and listed as "Name (Rarity)". That spelling is what the engine matches; the
  // bare name still means any of them.
  //
  // The drop rate comes from the identity for the same reason: the game gives
  // each vault its own chance, and looked up by name they all come back empty.
  const SPLIT = Object.entries(BY_ID)
    .map(([key, [name, rarity, tier, rate]]) => {
      const [type, id, weapon] = key.split(':').map(Number);
      return {
        name: `${name} (${rarity})`,
        // the name without the bracket, so the screen can rebuild the bracket
        // in the reader's language while the list still stores the English
        bare: name,
        type,
        id,
        weapon,
        rarity,
        tier,
        rate: rate || DROP_RATE[name.toLowerCase()] || 0,
        key: `${name} (${rarity})`.toLowerCase(),
      };
    })
    .filter((it) => LISTABLE.has(it.rarity) || it.type === VAULT);

  /// Every item a list can name, and the table a category is counted against.
  ///
  /// Deduped by name with the FIRST identity winning, which matters because
  /// this panel prints the count on a button: "every Set Polearm (15)" has to
  /// name the same fifteen the engine will match. Two names are claimed by a
  /// listable item and a relic — `Death's Scythe` and `Shrunken Head` — and
  /// `new Map(pairs)` would keep the relic's type for both.
  const NAMED = [
    ...new Map(
      Object.entries(ITEMS)
        .filter(([, name]) => LISTABLE.has(RARITY_BY_NAME[name.toLowerCase()]))
        .map(([key, name]) => {
          const [type, , weapon] = key.split(':').map(Number);
          return [
            name,
            {
              name,
              type,
              weapon,
              rarity: RARITY_BY_NAME[name.toLowerCase()],
              tier: TIER_BY_NAME[name.toLowerCase()] ?? 0,
              rate: DROP_RATE[name.toLowerCase()] ?? 0,
              key: name.toLowerCase(),
            },
          ];
        })
        // a Map keeps the LAST of a repeated key, so the pairs go in backwards
        // and the earliest identity is the one left standing
        .reverse(),
    ).values(),
    ...SPLIT,
  ].sort((a, b) => a.name.localeCompare(b.name));

  /// What the tables say about one entry of a list, whichever way it is spelled.
  const BY_ENTRY = new Map(NAMED.map((it) => [it.key, it]));
  const facts = (entry) => {
    const key = entry.toLowerCase();
    return (
      BY_ENTRY.get(key) ?? {
        rarity: RARITY_BY_NAME[key],
        tier: TIER_BY_NAME[key] ?? 0,
        rate: DROP_RATE[key] ?? 0,
      }
    );
  };

  /// The name on the screen. What a list stores is the English — that is what
  /// the engine matches a drop against — so the word is looked up here and
  /// nowhere else. The seven Essence Vaults are stored with their rarity in a
  /// bracket to tell them apart; the bracket is rebuilt rather than translated
  /// as if it were part of the name, which it is not.
  const shownName = (entry) => {
    const it = typeof entry === 'string' ? facts(entry) : entry;
    const stored = typeof entry === 'string' ? entry : entry.name;
    if (it?.bare) return `${nameOf(it.bare, it.type, it.id, it.weapon)} (${t(it.rarity)})`;
    return nameOf(stored, it?.type, it?.id, it?.weapon);
  };

  // what a character wears and carries. Orbs, vials, reagents and the like are
  // named too, but nobody wants a chime for a Goblin orb in a gear band — they
  // can still be added to a list by hand.
  const GEAR = new Set([0, 1, 2, 3, 4, 5, 6, 7, 8, 10]);

  const ALERT_RARITIES = ['Satanic', 'Set', 'Heroic', 'Angelic', 'Unholy'];
  const rarityCls = { Satanic: 'c-sat', Set: 'c-set', Heroic: 'c-her', Angelic: 'c-ang', Unholy: 'c-unh' };

  // The vocabulary a category is picked from, counted off the table above so
  // no number on this screen is one somebody typed.
  //
  // Five rarities and no more, for the reason LISTABLE gives: those are the ones
  // a drop arrives named under. The three the Essence Vaults carry alone —
  // Superior, Rare and Mythic — are reached by kind instead, where all seven
  // sit together.
  const RARITY_CHOICES = ALERT_RARITIES.map((rarity) => ({
    rarity,
    n: NAMED.filter((it) => it.rarity === rarity).length,
  }));

  // One row per kind a player would name. The wire has a single type for every
  // weapon there is, so type 3 is split by its weapon type and everything else
  // stands on its own.
  //
  // Relics are absent: every one of them is Common, they arrive nameless, and
  // they have a picker of their own on the Alerts tab. "Relic · 156" here would
  // be a category that could never make a sound.
  const TYPE_CHOICES = (() => {
    const by = new Map();
    for (const it of NAMED) {
      const weapon = it.type === 3 ? it.weapon : null;
      const key = `${it.type}:${weapon ?? ''}`;
      const row = by.get(key) ?? { key, type: it.type, weapon, n: 0 };
      row.n += 1;
      by.set(key, row);
    }
    return [...by.values()];
  })();

  // The word for each kind, and the order they are listed in — both worked out
  // where they are printed rather than where the rows are built. The rows are
  // built once when the script runs, which is before the language file has
  // arrived, so a label written into them stays English for good.
  //
  // Two kinds answer to one word and the player has to be able to tell them
  // apart: the game's own tables spell type 18 "Flask" and weapon type 15
  // "Flask" too, so the list held two rows reading "Flask" that differed only
  // in their count. A weapon says so.
  let kindChoices = $derived.by(() => {
    const rows = TYPE_CHOICES.map((r) => ({ ...r, label: typeLabel(r.type, r.weapon ?? 0) }));
    const spoken = new Map();
    for (const r of rows) spoken.set(r.label, (spoken.get(r.label) ?? 0) + 1);
    for (const r of rows) {
      if (spoken.get(r.label) > 1 && r.weapon != null) r.label = `${r.label} (${t('weapon')})`;
    }
    return rows.sort((a, b) => b.n - a.n || a.label.localeCompare(b.label, locale()));
  });

  /// The same question the engine asks of a drop, asked of a row of the table.
  ///
  /// It has to stay the same question. `stats::Rule::matches` is the other
  /// half of it: null means "any" on every field there too, and a weapon type
  /// is only looked at once an item type has been named.
  const ruleHits = (rule, it) =>
    (!rule.rarity || rule.rarity === it.rarity) &&
    (rule.item_type == null ||
      (rule.item_type === it.type && (rule.weapon == null || rule.weapon === it.weapon)));

  const ruleMatches = (rule) => NAMED.filter((it) => ruleHits(rule, it));

  const ruleName = (rule) => {
    const kind = rule.item_type == null ? null : typeLabel(rule.item_type, rule.weapon ?? 0);
    if (rule.rarity && kind) return say('every {rarity} {kind}', { rarity: t(rule.rarity), kind });
    if (rule.rarity) return say('every {rarity} item', { rarity: t(rule.rarity) });
    return say('every {kind}', { kind });
  };

  const sameRule = (a, b) =>
    (a.rarity ?? null) === (b.rarity ?? null) &&
    (a.item_type ?? null) === (b.item_type ?? null) &&
    (a.weapon ?? null) === (b.weapon ?? null);

  let settings = $state(null);
  let selected = $state(0);
  let status = $state({});
  let saveTimer;

  // Two boxes, not one: a single field cannot both add an item and narrow the
  // list below without a label explaining which it is doing. What the single
  // box did silently — type a name, watch it move from the results into the
  // list — is the line under the results, which says so and offers the click.
  let addQuery = $state('');
  let listQuery = $state('');
  let addRarity = $state('');
  let addType = $state('');
  let more = $state(false);
  let renaming = $state(false);
  let showing = $state(null);

  let filters = $derived(settings?.filters ?? []);
  let filter = $derived(filters.find((f) => f.id === settings?.filter) ?? filters[0] ?? null);
  let lists = $derived(filter?.lists ?? []);
  let current = $derived(lists[selected] ?? null);
  let soundKey = $derived(current ? `list-${current.id}` : null);
  let rules = $derived(current?.rules ?? []);

  /// The rule the two dropdowns are describing, or null while neither is set.
  ///
  /// Neither set is not a category — it is every named item in the game — so
  /// the button simply is not there. `engine_rule` refuses the same thing on
  /// the way in, because a settings file can be edited by hand.
  let draft = $derived.by(() => {
    const kind = TYPE_CHOICES.find((row) => row.key === addType) ?? null;
    if (!addRarity && !kind) return null;
    return {
      rarity: addRarity || null,
      item_type: kind ? kind.type : null,
      weapon: kind ? kind.weapon : null,
    };
  });
  let draftCount = $derived(draft ? ruleMatches(draft).length : 0);
  let draftOwned = $derived(!!draft && rules.some((r) => sameRule(r, draft)));

  /// What each list covers today: the names on it, plus everything its rules
  /// match. The clash warning is worked out from this rather than from the
  /// names alone — otherwise a category on list 1 would quietly outrank a name
  /// on list 2 with nothing on the screen to say that it had.
  let covered = $derived(
    lists.map((l) => {
      const names = new Set((l.items ?? []).map((n) => n.toLowerCase()));
      for (const it of NAMED) {
        if ((l.rules ?? []).some((r) => ruleHits(r, it))) names.add(it.key);
      }
      return names;
    }),
  );

  // An item in two lists is a conflict: only the first list's sound plays, and
  // the order of the lists decides which. The rail says so in words, and the row
  // keeps a chip naming the other list.
  //
  // A list that is switched off is not in the running — `lib.rs` keeps only
  // `l.enabled && !l.id.is_empty()` — so counting one here would name a winner
  // that cannot sound.
  let clashes = $derived.by(() => {
    const owners = new Map();
    covered.forEach((names, i) => {
      if (!lists[i].enabled) return;
      for (const key of names) owners.set(key, [...(owners.get(key) ?? []), lists[i].name]);
    });
    return new Map([...owners].filter(([, names]) => names.length > 1));
  });

  /// The row a conflict is actually decided in favour of: the first list that
  /// is switched on, or -1 when none is.
  let winner = $derived(lists.findIndex((l) => l.enabled));

  const clashesIn = (i) =>
    lists[i].enabled ? [...covered[i]].filter((key) => clashes.has(key)).length : 0;
  const clashNames = (i) => {
    const others = new Set();
    for (const key of covered[i]) {
      for (const name of clashes.get(key) ?? []) if (name !== lists[i].name) others.add(name);
    }
    return [...others];
  };
  const clashWith = (name) =>
    !current?.enabled
      ? ''
      : (clashes.get(name.toLowerCase()) ?? []).filter((n) => n !== current?.name).join(', ');

  // an item can sit in two lists, but only the first one's sound plays — so
  // say where else it is before it is added again
  let elsewhere = $derived.by(() => {
    const seen = new Map();
    lists.forEach((list, i) => {
      if (list === current) return;
      for (const key of covered[i]) if (!seen.has(key)) seen.set(key, list.name);
    });
    return seen;
  });

  let matches = $derived.by(() => {
    const q = addQuery.trim().toLowerCase();
    if (!q && !draft) return [];
    const owned = new Set((current?.items ?? []).map((n) => n.toLowerCase()));
    return NAMED.filter(
      (it) => (!q || it.key.includes(q)) && (!draft || ruleHits(draft, it)) && !owned.has(it.key),
    ).slice(0, 40);
  });

  /// How many of what the search asked for is already on this list.
  let alreadyHere = $derived.by(() => {
    const q = addQuery.trim().toLowerCase();
    if (!q) return 0;
    return (current?.items ?? []).filter((n) => n.toLowerCase().includes(q)).length;
  });

  // sorted by name, and narrowed by the list's own box
  let shown = $derived.by(() => {
    const q = listQuery.trim().toLowerCase();
    const items = [...(current?.items ?? [])].sort((a, b) => a.localeCompare(b));
    return q ? items.filter((n) => n.toLowerCase().includes(q)) : items;
  });

  /// Clear removes what the box is showing, so what it would remove changes
  /// with the search as well as with the list. Both belong in the key that
  /// arms it, or the confirmation is for one set and the deletion for another.
  let clearKey = $derived(`clear:${current?.id}:${listQuery.trim().toLowerCase()}`);
  // The key has to name what is armed, and the template has to compare against
  // the same key it armed with. Comparing against a bare 'filter' or 'list'
  // while arming with `filter:<id>` was always false: the button never turned
  // red, never read "delete?", and the first click looked like nothing had
  // happened — on the two controls that destroy a whole filter or list.
  let filterKey = $derived(`filter:${filter?.id}`);
  let listKey = $derived(`list:${current?.id}`);

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

  /// The answer is written after it arrives, never around it: in
  /// `status = { ...status, [key]: await … }` the spread is evaluated BEFORE the
  /// await, so several lists asked at once each copy the same empty map and the
  /// last reply overwrites the rest.
  async function refreshStatus(key) {
    const name = await invoke('sound_status', { rarity: key }).catch(() => null);
    status[key] = name;
  }

  function save() {
    clearTimeout(saveTimer);
    const snapshot = $state.snapshot(settings);
    saveTimer = setTimeout(() => invoke('save_settings', { settings: snapshot }).catch(() => {}), 150);
  }

  // What a blank cell means, said instead of left blank. The tables carry a
  // chance only for items the game states one for, and an empty cell reads as a
  // hole in the app rather than as an answer.
  //
  // A function, not a string: a const is filled in once and would keep the
  // language it loaded in.
  const NO_ODDS = () =>
    t('the game states no drop chance for this — it comes from a boss, a chest or a tower rather than falling in the world');

  // "one in 576425" is true but unreadable in a row; "1/576k" is not
  function odds(rate) {
    if (!rate) return '—';
    if (rate >= 1e6) return `1/${(rate / 1e6).toFixed(rate >= 1e7 ? 0 : 1)}M`;
    if (rate >= 1e3) return `1/${(rate / 1e3).toFixed(rate >= 1e4 ? 0 : 1)}k`;
    return `1/${rate}`;
  }

  const id = () => Math.random().toString(36).slice(2, 8);

  // Deleting a watchlist takes its lists and their sounds with it, and clearing
  // a list is as final, so anything destructive asks once: the second click does
  // it, walking away forgets.
  //
  // The key names WHAT is armed, not just which button. Keyed by button alone,
  // arming the delete on one list and then selecting another leaves the second
  // armed without ever having been asked about.
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

  function addFilter(name, made = []) {
    const built = { id: id(), name, lists: made };
    settings.filters = [...filters, built];
    settings.filter = built.id;
    selected = 0;
    save();
    return built;
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
    rules: [],
  });

  // Drop rates are "one in N", so sorting by them splits a grade into the items
  // you see often, the ones you do not, and the chase pieces.
  //
  // A place to start from rather than a tool: it writes eight lists — six bands
  // of how often a thing falls, plus Angelic and Unholy, which `APART` keeps out
  // of the bands — and the player then names them and gives each a sound. Lives
  // in the ⋯ menu and the empty state, because adding a category below covers
  // the same ground with the player choosing it.
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
        if (slice.length) bands.push(band(`${letter}-${t(name)}`, slice));
      }
    }
    for (const rarity of APART) {
      const own = NAMED.filter((it) => it.rarity === rarity && GEAR.has(it.type)).sort((a, b) =>
        a.name.localeCompare(b.name),
      );
      if (own.length) bands.push(band(rarity, own));
    }
    addFilter(t('Drop rate bands'), bands);
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
    filter.lists = [
      ...lists,
      { id: id(), name: `${t('List')} ${lists.length + 1}`, enabled: true, volume: 0.7, items: [], rules: [] },
    ];
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

  function removeItem(name) {
    current.items = current.items.filter((n) => n !== name);
    save();
  }

  /// Removes what the list's own box is showing, or the whole list when it is
  /// not narrowing — the count on the button says which.
  function removeShown() {
    const gone = new Set(shown.map((n) => n.toLowerCase()));
    current.items = current.items.filter((n) => !gone.has(n.toLowerCase()));
    save();
  }

  /// A category goes on the list as a rule, never as the names it stands for.
  ///
  /// That is the decision this feature turns on, and the screen has to carry
  /// it: an update that adds a Satanic helmet to the game adds it to "every
  /// Satanic Helmet" on its own, where the 36 names there are today, written
  /// out, would go on meaning today's 36 for ever. `stats::Rule` is where the
  /// question gets asked of the drop itself.
  function addRule() {
    if (!draft || draftOwned) return;
    current.rules = [...rules, { ...draft }];
    save();
  }

  function removeRule(i) {
    current.rules = rules.filter((_, n) => n !== i);
    if (showing === i) showing = null;
    save();
  }

  /// The one direction worth being reversible: a rule turned into the names it
  /// matches today, for a player who wants to drop three of them. It stops
  /// being a rule at that moment and stops growing with the game, which is
  /// exactly the difference the two sections below are drawn to show.
  function unpackRule(i) {
    const names = ruleMatches(rules[i]).map((it) => it.name);
    const have = new Set(current.items.map((n) => n.toLowerCase()));
    current.items = [...current.items, ...names.filter((n) => !have.has(n.toLowerCase()))].sort(
      (a, b) => a.localeCompare(b),
    );
    current.rules = rules.filter((_, n) => n !== i);
    showing = null;
    save();
  }

  let notice = $state('');
  let noticeTimer;
  function notify(text) {
    notice = text;
    clearTimeout(noticeTimer);
    noticeTimer = setTimeout(() => (notice = ''), 4000);
  }

  async function exportFilter() {
    try {
      const name = await invoke('export_filter', { filter: $state.snapshot(filter) });
      if (name) notify(say('saved as {name}', { name }));
    } catch (e) {
      notify(String(e));
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
      notify(say('imported {name} — {n} lists', { name: imported.name, n: imported.lists.length }));
    } catch (e) {
      notify(String(e));
    }
  }

  /// A copy gets fresh list ids so it cannot fight with the original — and a
  /// list's sound is a file named after its id, so the copy has to be given
  /// one of its own too. Without that the copy came out mute while the button
  /// said "sounds and all".
  function duplicateFilter() {
    const pairs = lists.map((l) => [l.id, id()]);
    const made = addFilter(
      `${filter.name} ${t('copy')}`,
      lists.map((l, i) => ({
        ...l,
        id: pairs[i][1],
        items: [...l.items],
        rules: (l.rules ?? []).map((r) => ({ ...r })),
      })),
    );
    for (const [from, to] of pairs) {
      invoke('copy_sound', { from: `list-${from}`, to: `list-${to}` })
        .then(() => refreshStatus(`list-${to}`))
        .catch(() => {});
    }
    return made;
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

  const btn = (name) => `url(${art(name)})`;
</script>

<div class="panel">
{#if settings}
  <!-- The switch first, and what it does in one sentence beside it. Every
       other control on this tab is dead while it is off, so it goes at the
       top rather than eight controls down a long column. -->
  <div class="strip" style:border-image-source="url({art('chip_dark')})">
    <button class="check" onclick={() => { settings.use_filter = !settings.use_filter; save(); }} aria-label={t("use this watchlist")}>
      <img src={settings.use_filter ? art('check_on') : art('check_off')} alt="" />
    </button>
    <div class="says">
      <div class="opt">{t("Use this watchlist")}</div>
      <!-- Not "lists that outrank the above": that is true of an item on one
           and reads as true of everything else, so people take it to mean a
           filter switches the rarity alerts off and ask for the adding
           behaviour the app already has. It says which it
           is now, and it says it in both states, because "off" is the one a
           player arrives in. -->
      {#if settings.use_filter}
        <div class="note"> {t("A listed item plays its list's sound instead of its rarity's. Everything you have not listed carries on exactly as the Alerts tab says.")} </div>
      {:else}
        <div class="note warn">{t("Nothing here makes a sound. The rarity alerts on the Alerts tab still do.")}</div>
      {/if}
    </div>
  </div>

  <div class="setbar">
    {#if renaming && filter}
      <input
        class="rename"
        style:border-image-source="url({art('chip_dark')})"
        value={filter.name}
        oninput={(e) => { filter.name = e.currentTarget.value; save(); }}
        onblur={() => (renaming = false)}
        onkeydown={(e) => e.key === 'Enter' && (renaming = false)}
      />
    {:else}
      <select
        class="picker"
        value={filter?.id ?? ''}
        onchange={(e) => { settings.filter = e.currentTarget.value; selected = 0; save(); }}
      >
        {#each filters as f}
          <option value={f.id}>{f.name} · {f.lists.length} {t("lists")}</option>
        {:else}
          <option value="">{t("no watchlists yet")}</option>
        {/each}
      </select>
    {/if}
    <button class="btn" style:--btn={btn('button')} style:--btn-hover={btn('button_hover')} style:--btn-down={btn('button_down')} onclick={() => addFilter(`Filter ${filters.length + 1}`)}>{t("+ New")}</button>
    <button class="btn" style:--btn={btn(more ? 'button_down' : 'button')} style:--btn-hover={btn('button_hover')} style:--btn-down={btn('button_down')} onclick={() => (more = !more)} title={t("Rename, copy, import, export, delete")}>⋯</button>
  </div>

  {#if more}
    <!-- Buttons each used about once in the life of a watchlist, kept off the
         first screen rather than taken away. Generate is here for the second
         half of that sentence: offering it only from the empty state made it
         unreachable to anyone who already had a watchlist — the way to it was
         to delete every one they had. -->
    <div class="setbar sub">
      {#if filter}
        <button class="btn sm" style:--btn={btn('button')} style:--btn-hover={btn('button_hover')} style:--btn-down={btn('button_down')} onclick={() => (renaming = true)}>{t("Rename")}</button>
        <button class="btn sm" style:--btn={btn('button')} style:--btn-hover={btn('button_hover')} style:--btn-down={btn('button_down')} onclick={generate} title={t("Add six lists banded by how often the item falls — a place to start from")}>{t("Generate")}</button>
        <button class="btn sm" style:--btn={btn('button')} style:--btn-hover={btn('button_hover')} style:--btn-down={btn('button_down')} onclick={duplicateFilter} title={t("Copy this watchlist, sounds and all")}>{t("Copy")}</button>
      {/if}
      <button class="btn sm" style:--btn={btn('button')} style:--btn-hover={btn('button_hover')} style:--btn-down={btn('button_down')} onclick={importFilter} title={t("Load a watchlist someone shared with you, sounds included")}>{t("Import…")}</button>
      {#if filter}
        <button class="btn sm" style:--btn={btn('button')} style:--btn-hover={btn('button_hover')} style:--btn-down={btn('button_down')} onclick={exportFilter} title={t("Save this watchlist to a file, sounds included")}>{t("Export…")}</button>
        <button
          class="btn sm"
          class:armed={armed === filterKey}
          style:--btn={btn('button')}
          style:--btn-hover={btn('button_hover')}
          style:--btn-down={btn('button_down')}
          onclick={() => danger(filterKey, removeFilter)}
          title={t("Delete this watchlist with all its lists and sounds")}
        >{armed === filterKey ? t('delete it?') : t('Delete')}</button>
      {/if}
    </div>
  {/if}
  {#if notice}<div class="notice">{notice}</div>{/if}

  {#if filter}
    <div class="work">
      <!-- The number on each row IS the priority. It was two unlabelled arrows
           on a horizontal strip before, which said the lists were ordered
           without ever saying what the order decided — and the one thing it
           decides is which sound plays when two lists hold the same item. Up
           and down read as rank on a column in a way left and right on a row
           never did. -->
      <div class="rail" style:border-image-source="url({art('chip_dark')})">
        <!-- The heading names the list that actually wins, not the first row.
             A list that is switched off is not in the running at all, so on a
             rail whose first row is unticked "1 wins" is a plain lie about the
             one thing this order decides. -->
        <div class="railhead" data-tauri-drag-region>
          {t('Lists')}{#if winner >= 0}{' · '}{say('{n} wins', { n: winner + 1 })}{/if}
        </div>
        <div class="note">
          {#if winner < 0}
            {t('Nothing is switched on, so no list sounds.')}
          {:else}
            {t('When two switched-on lists hold the same item, the higher one sounds.')}
          {/if}
        </div>
        <div class="rows">
          {#each lists as list, i (list.id)}
            <div class="lrow" class:sel={i === selected} class:off={!list.enabled}>
              <span class="rank">{i + 1}</span>
              <button class="check" onclick={() => { list.enabled = !list.enabled; save(); }} aria-label={say('{name} enabled', { name: list.name })}>
                <img src={list.enabled ? art('check_on') : art('check_off')} alt="" />
              </button>
              {#if i === selected}
                <input
                  class="lname"
                  value={list.name}
                  oninput={(e) => { list.name = e.currentTarget.value; save(); }}
                />
              {:else}
                <button class="lpick" onclick={() => (selected = i)}>{list.name}</button>
              {/if}
              <span class="count">{covered[i].size}</span>
            </div>
            {#if clashesIn(i)}
              <!-- The warning the "?" chip on the old tab stood for, in the
                   words it stood for. The chip is still on the item row, where
                   it says which other list has that one item; here there is
                   room to say what it means. -->
              <div class="note warn cn">{say('{n} also in {lists}', { n: clashesIn(i), lists: clashNames(i).join(', ') })}</div>
            {/if}
          {/each}
        </div>
        <div class="railfoot">
          <button class="link" disabled={selected === 0} onclick={() => moveList(-1)} title={t("Move up — a higher list wins a conflict")}>{t("▲ up")}</button>
          <button class="link" disabled={selected >= lists.length - 1} onclick={() => moveList(1)} title={t("Move down")}>{t("▼ down")}</button>
          <button class="btn sm" style:--btn={btn('button')} style:--btn-hover={btn('button_hover')} style:--btn-down={btn('button_down')} onclick={addList}>{t("+ list")}</button>
        </div>
      </div>

      <div class="detail">
      {#if current}
        <div class="dhead" data-tauri-drag-region>
          <span class="dname">{current.name}</span>
          {#if !current.enabled}<span class="muted">{t("switched off")}</span>{/if}
          <button
            class="del"
            class:armed={armed === listKey}
            onclick={() => danger(listKey, () => removeList(selected))}
            title={t("Delete this list and its sound")}
          >{armed === listKey ? t('delete?') : '×'}</button>
        </div>

        <div class="sound" style:border-image-source="url({art('chip_dark')})">
          <!-- The file is named after the list's id, so what is on disk is
               "list-luu3rf.wav" and says nothing to anybody. What matters here
               is whether there is one at all; the path is in the tooltip for
               the times it does matter. -->
          <span class="file" class:none={!status[soundKey]} title={status[soundKey] ? `sounds/${status[soundKey]}` : ''}>
            {status[soundKey] ? t('♪ Custom sound') : t('No sound of its own — a drop here is announced by the item’s rarity')}
          </span>
          <input
            class="vol"
            type="range"
            min="0"
            max="1"
            step="0.05"
            value={current.volume}
            oninput={(e) => { current.volume = +e.currentTarget.value; save(); }}
          />
          <!-- A list with no file of its own has nothing of its own to play:
               when one of its items drops it is announced by that item's
               rarity, and which rarity that is depends on the item. So the
               button says so rather than being pressed and doing nothing,
               which is what it did. -->
          <button
            class="btn sm"
            style:--btn={btn('button')}
            style:--btn-hover={btn('button_hover')}
            style:--btn-down={btn('button_down')}
            disabled={!status[soundKey]}
            title={status[soundKey] ? t('play this list’s sound') : t('this list has no sound of its own — a drop on it is announced by the item’s rarity')}
            onclick={test}
          >{t("Test")}</button>
          <button class="btn sm" style:--btn={btn('button')} style:--btn-hover={btn('button_hover')} style:--btn-down={btn('button_down')} onclick={pickSound}>{t("Browse…")}</button>
          <button
            class="btn sm"
            class:armed={armed === `snd-${soundKey}`}
            style:--btn={btn('button')}
            style:--btn-hover={btn('button_hover')}
            style:--btn-down={btn('button_down')}
            disabled={!status[soundKey]}
            onclick={() => danger(`snd-${soundKey}`, () => invoke('clear_sound', { rarity: soundKey }).catch(() => {}))}
          >{armed === `snd-${soundKey}` ? t('Sure?') : t('Clear')}</button>
        </div>

        <div class="box" style:border-image-source="url({art('chip_dark')})">
          <div class="boxhead" data-tauri-drag-region>{t("Add items")}</div>
          <!-- The Items tab's own toolbar, in its order, so that the two tabs
               are one app: search, then rarity, then kind. -->
          <div class="tools">
            <input
              class="find"
              style:border-image-source="url({art('chip_dark')})"
              placeholder={t("Search by name")}
              bind:value={addQuery}
              onkeydown={(e) => e.key === 'Enter' && matches[0] && addItem(matches[0].name)}
            />
            <select class="picker" bind:value={addRarity}>
              <option value="">{t("Any rarity")}</option>
              {#each RARITY_CHOICES as r}<option value={r.rarity}>{t(r.rarity)} · {r.n}</option>{/each}
            </select>
            <select class="picker" bind:value={addType}>
              <option value="">{t("Any kind")}</option>
              {#each kindChoices as kind}<option value={kind.key}>{kind.label} · {kind.n}</option>{/each}
            </select>
          </div>

          {#if draft}
            <button class="bulk" disabled={draftOwned} onclick={addRule}>
              {draftOwned
                ? say('{rule} is already a rule on this list', { rule: ruleName(draft) })
                : `+ ${t('Add')} ${ruleName(draft)}  (${draftCount})`}
            </button>
            {#if !draftOwned}
              <div class="note">{t("as a rule — an item the game adds to it later joins it on its own")}</div>
            {/if}
          {/if}

          {#if matches.length}
            <div class="results">
              {#each matches as it}
                <button class="hit" onclick={() => addItem(it.name)}>
                  <span class={rarityCls[it.rarity]}>{shownName(it)}</span>
                  {#if elsewhere.has(it.key)}
                    <span class="already">{t('in')} {elsewhere.get(it.key)}</span>
                  {/if}
                  <span class="grade">
                    <span class="kind">{typeLabel(it.type, it.weapon)}</span>
                    <span class="letter">{tierLabel(it.tier)}</span>
                    <span class="odds" title={it.rate ? '' : NO_ODDS()}>{odds(it.rate)}</span>
                  </span>
                </button>
              {/each}
            </div>
          {:else if addQuery.trim() || draft}
            <div class="note">
              {alreadyHere ? t('everything that matches is already on this list') : t('nothing matches')}
            </div>
          {/if}
          {#if alreadyHere && matches.length}
            <button class="link" onclick={() => (listQuery = addQuery)}>
              {alreadyHere === 1
                ? say('{n} already on this list — show it', { n: alreadyHere })
                : say('{n} already on this list — show them', { n: alreadyHere })}
            </button>
          {/if}
        </div>

        <div class="box grow" style:border-image-source="url({art('chip_dark')})">
          <div class="boxhead" data-tauri-drag-region>{t("In this list")}</div>

          {#if rules.length}
            <!-- Rules are a section of their own, above the names, because they
                 are not names: a rule has a count that moves when the game
                 changes, and a name does not. They are told apart on the screen
                 the way they are told apart in the settings file. -->
            <div class="listhead"><span>{t("Rules")}</span><span class="count">{rules.length}</span></div>
            {#each rules as rule, i}
              <div class="rulerow">
                <span class="chip {rarityCls[rule.rarity] ?? ''}">
                  {rule.rarity ? t(rule.rarity) : t('any rarity')} · {rule.item_type == null
                    ? t('any kind')
                    : typeLabel(rule.item_type, rule.weapon ?? 0)}
                </span>
                <span class="rcount">{ruleMatches(rule).length} {t('items today')}</span>
                <button class="link" onclick={() => (showing = showing === i ? null : i)}>{showing === i ? t('hide') : t('show')}</button>
                <button
                  class="link"
                  onclick={() => unpackRule(i)}
                  title={t("Turn it into the names it matches today. It stops being a rule, and stops growing with the game.")}
                >{t("unpack")}</button>
                <button class="del" onclick={() => removeRule(i)} title={t("Remove this rule")} aria-label={t("remove rule")}>×</button>
              </div>
              {#if showing === i}
                <div class="preview">
                  {#each ruleMatches(rule) as it}
                    <span class="pname {rarityCls[it.rarity] ?? ''}">{shownName(it)}</span>
                  {/each}
                </div>
              {/if}
            {/each}
          {/if}

          <div class="listhead">
            <span>{t("Items")}</span>
            {#if shown.length}
              <button class="link" class:armed={armed === clearKey} onclick={() => danger(clearKey, removeShown)}>
                {#if armed === clearKey}
                  {listQuery.trim() ? say('remove {n}?', { n: shown.length }) : t('clear the list?')}
                {:else}
                  {listQuery.trim() ? say('remove {n} shown', { n: shown.length }) : t('clear')}
                {/if}
              </button>
            {/if}
            <span class="count">{listQuery.trim() ? say('{n} of {all}', { n: shown.length, all: current.items.length }) : current.items.length}</span>
          </div>

          {#if current.items.length > 8 || listQuery.trim()}
            <input class="find" style:border-image-source="url({art('chip_dark')})" placeholder={t("Narrow this list…")} bind:value={listQuery} />
          {/if}

          <div class="items">
            {#each shown as name}
              {@const it = facts(name)}
              <div class="row {rarityCls[it.rarity] ?? ''}">
                <span class={rarityCls[it.rarity] ?? ''}>{shownName(name)}</span>
                {#if clashWith(name)}
                  <span class="clash" title={say('also in {list} — only the list that comes first will sound', { list: clashWith(name) })}>?</span>
                {/if}
                <span class="grade">
                  <span class="letter">{tierLabel(it.tier)}</span>
                  <span class="odds" title={it.rate ? '' : NO_ODDS()}>{odds(it.rate)}</span>
                </span>
                <button class="del" onclick={() => removeItem(name)} title={t("Remove")} aria-label={t("remove")}>×</button>
              </div>
            {:else}
              <div class="empty">
                {#if listQuery.trim()}
                  {t('nothing in this list matches the search')}
                {:else if rules.length}
                  {rules.length === 1
                    ? t('no items by name — this list is the rule above')
                    : t('no items by name — this list is the rules above')}
                {:else}
                  {t('nothing listed yet — search above and click an item, or pick a rarity and a kind to take a whole category at once')}
                {/if}
              </div>
            {/each}
          </div>
        </div>
      {:else}
        <div class="empty">{t("this watchlist has no lists yet — press “+ list”")}</div>
      {/if}
      </div>
    </div>
  {:else}
    <div class="empty"> {t("No watchlist yet. Press “+ New” for an empty one, or")} <button class="prose" onclick={generate}>{t("start from the drop rates")}</button> {t("— S and SS split into the items you see often, the ones you do not, and the chase pieces, each ready for a sound of its own.")} </div>
  {/if}
{/if}
</div>

<style>
  @font-face {
    font-family: 'CookieRun Bold';
    src: url('./assets/fonts/cookierunbold.ttf') format('truetype');
  }

  .panel {
    position: relative;
    box-sizing: border-box;
    width: 100%;
    height: 100%;
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-family: var(--face);
    font-size: 12px;
    color: var(--bone-6);
    min-height: 0;
    /* Stacked, this is the one thing that scrolls; side by side the two panes
       take it over. Without it a narrow window simply clipped the list, with
       nothing to reach it by. */
    overflow-y: auto;
    padding-right: 2px;
  }
  /* Nothing may shrink: in a flex column a child gives way to its siblings
     before the container scrolls, which on a short window drew the set bar
     over the switch above it. */
  .panel > * { flex: 0 0 auto; }
  .panel::-webkit-scrollbar { width: 6px; }
  .panel::-webkit-scrollbar-thumb { background: var(--dim-1); border-radius: 3px; }

  .strip,
  .rail,
  .sound,
  .box,
  .rename,
  .find {
    box-sizing: border-box;
    border: 6px solid transparent;
    border-image-slice: 6 fill;
    border-image-width: 6px;
    image-rendering: pixelated;
  }

  .strip {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 5px 7px;
  }
  .says { min-width: 0; }
  .opt { color: var(--bone-13); }

  .setbar { display: flex; align-items: center; gap: 6px; }
  .setbar.sub { flex-wrap: wrap; }

  /* The work: the lists on the left, the one being edited on the right.
     Side by side needs about 520px of pane, which is what a 700px window
     leaves once the frame, the 116px nav and the pane's own border are out —
     a 168px rail and a 350px detail. Under that they stack, and the rail turns
     into a strip of wrapping chips whose NUMBERS keep the priority readable
     when it wraps, which is the one thing the old left/right arrows on a
     horizontal strip could never do. */
  .work {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: 8px;
  }
  @media (min-width: 700px) {
    /* Side by side, and now the work is what has the height rather than the
       page: the panel stops scrolling and each pane takes over its own. That
       is the rule the old page had to learn twice — stacked, a height cap on
       either half left both of them read through a slit with the page unable
       to scroll at all. */
    .work { flex: 1 1 auto; min-height: 0; grid-template-columns: 168px minmax(0, 1fr); }
    .rail,
    .detail { min-height: 0; overflow-y: auto; }
  }
  .detail { display: flex; flex-direction: column; gap: 6px; min-width: 0; }
  .detail > * { flex: 0 0 auto; }

  .rail { padding: 4px 5px 5px; display: flex; flex-direction: column; gap: 3px; }
  .railhead,
  .boxhead {
    color: var(--edge-2b);
    font-size: 10px;
    letter-spacing: 0.3px;
    text-transform: uppercase;
  }
  .rows { display: flex; flex-direction: column; gap: 2px; margin: 2px 0; }
  @media (max-width: 699px) {
    /* stacked: the lists become a wrapping strip of numbered chips */
    .rows { flex-direction: row; flex-wrap: wrap; gap: 4px; }
    .lrow { flex: 0 0 auto; background: rgba(0, 0, 0, 0.25); border: 1px solid var(--ground-10); padding: 2px 5px; }
    .lname, .lpick { max-width: 110px; }
    .cn { flex-basis: 100%; }
  }
  .lrow {
    display: flex;
    align-items: center;
    gap: 5px;
    min-height: 22px;
    min-width: 0;
    padding: 1px 3px;
  }
  .lrow.sel { background: rgba(var(--pick-rgb), 0.35); }
  .lrow.off .lname,
  .lrow.off .lpick,
  .lrow.off .count { opacity: 0.5; }
  /* The rank is the whole point of the rail, so it is drawn like a figure and
     not like a bullet: fixed width, so 1 and 10 keep the same left edge. */
  .rank {
    flex: none;
    width: 12px;
    text-align: right;
    color: var(--edge-5);
    font-size: 11px;
    font-variant-numeric: tabular-nums;
  }
  .lname,
  .lpick {
    flex: 1 1 auto;
    min-width: 0;
    font: inherit;
    font-size: 11px;
    color: var(--bone-13);
    background: none;
    border: none;
    outline: none;
    padding: 0;
    text-align: left;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .lpick { color: var(--bone-3); cursor: pointer; }
  .lpick:hover { color: var(--bone-13); }
  .count { flex: none; color: var(--edge-5); font-size: 10px; font-variant-numeric: tabular-nums; }
  .cn { padding-left: 17px; }
  .railfoot { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }

  .dhead { display: flex; align-items: baseline; gap: 8px; padding: 0 2px; }
  .dname { color: var(--bone-13); font-size: 13px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .muted { color: var(--gold-1); font-size: 10px; }
  .dhead .del { margin-left: auto; }

  .sound { display: flex; align-items: center; gap: 6px; padding: 2px 6px; min-height: 28px; flex-wrap: wrap; }
  .file { flex: 1 1 140px; min-width: 0; color: var(--bone-6); font-size: 11px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  /* With a sound, this is two words and shares the row. Without one it is the
     sentence that says what happens instead, and squeezed in beside a slider
     and three buttons it came out as "No sound of its own — a drop here…" —
     the half that matters cut off, and nothing in the tooltip either, because
     there is no file to name. So it takes the whole line and the controls go
     under it. */
  .file.none {
    flex: 1 1 100%;
    color: var(--dim-2);
    white-space: normal;
    overflow: visible;
    line-height: 1.4;
  }

  .box { padding: 4px 6px 6px; display: flex; flex-direction: column; gap: 4px; min-width: 0; }
  /* Only the list of items may give way. A flex child shrinks below its own
     content by default, and in a box whose height is already spoken for that
     is what happened to the rule preview: "show" opened it and it came out a
     ten-pixel strip with a scrollbar in it, which reads as a broken control
     rather than as ninety-six pixels of names. */
  .box > * { flex: 0 0 auto; }
  /* Only the list itself grows into whatever height is left; the box above it
     is as tall as its contents. */
  .box.grow { flex: 1 1 auto; min-height: 0; }

  .tools { display: flex; gap: 5px; flex-wrap: wrap; }
  /* `flex-basis` is read along the container's own axis. The search box above
     sits in a row, where 150px is a width; the one narrowing the list below
     sits in a column, where it was a *height* — a one-line field a hundred
     pixels tall with the placeholder floating in the middle of it. */
  .find { flex: 1 1 150px; min-width: 0; height: 24px; font: inherit; font-size: 11px; color: var(--bone-13); background: none; outline: none; padding: 0 6px; }
  .box > .find { flex: none; }

  /* The one control that adds a whole category, and it says exactly what it
     will add and how much of it. Wide, because it is the answer to the two
     dropdowns above it and belongs to them, not to the results below. */
  .bulk {
    box-sizing: border-box;
    width: 100%;
    font: inherit;
    font-size: 11px;
    color: var(--bone-15);
    background: rgba(var(--pick-rgb), 0.45);
    border: 1px solid var(--edge-4);
    padding: 5px 8px;
    cursor: pointer;
    text-align: left;
  }
  .bulk:hover:not(:disabled) { background: rgba(var(--pick-rgb), 0.7); }
  .bulk:disabled { color: var(--dim-2); background: rgba(0, 0, 0, 0.25); border-color: var(--ground-10); cursor: default; }

  .results {
    flex: none;
    max-height: 152px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    padding: 1px;
    background: rgba(0, 0, 0, 0.22);
  }
  .hit {
    display: flex;
    align-items: center;
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
  .hit:hover { background: rgba(var(--pick-rgb), 0.45); }
  .hit > span:first-child { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .already { margin-left: auto; color: var(--edge-2b); font-size: 10px; white-space: nowrap; }

  /* A rule, drawn so it cannot be mistaken for one of the rows below it: a
     chip with a count that moves, against a flat row with an item's name. */
  .rulerow { display: flex; align-items: center; gap: 6px; padding: 2px 2px; flex-wrap: wrap; }
  .chip {
    flex: none;
    font-size: 11px;
    color: var(--bone-13);
    background: rgba(0, 0, 0, 0.35);
    border: 1px solid var(--edge-4);
    padding: 2px 7px;
  }
  .rcount { color: var(--edge-5); font-size: 10px; }
  .rulerow .del { margin-left: auto; }
  /* Read-only on purpose: what a rule matches is not a list to prune, it is an
     answer to "what does this cover". Pruning is what "unpack" is for, and it
     stops the rule being a rule. */
  .preview {
    max-height: 96px;
    overflow-y: auto;
    display: flex;
    flex-wrap: wrap;
    gap: 2px 10px;
    padding: 3px 6px 5px;
    background: rgba(0, 0, 0, 0.25);
  }
  .pname { font-size: 10px; opacity: 0.75; }

  .items {
    flex: 1 1 auto;
    min-height: 120px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding-right: 2px;
  }
  .items::-webkit-scrollbar,
  .results::-webkit-scrollbar,
  .preview::-webkit-scrollbar,
  .rail::-webkit-scrollbar,
  .detail::-webkit-scrollbar { width: 6px; }
  .items::-webkit-scrollbar-thumb,
  .results::-webkit-scrollbar-thumb,
  .preview::-webkit-scrollbar-thumb,
  .rail::-webkit-scrollbar-thumb,
  .detail::-webkit-scrollbar-thumb { background: var(--dim-1); border-radius: 3px; }

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
  .listhead .count { margin-left: auto; }

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
  .row:hover { background: rgba(var(--pick-rgb), 0.22); }
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
  /* The kind is the one column that goes when there is no room for it: the
     search is by name, and a name that is not there at all is worse than a
     kind that is not. */
  .kind { color: var(--edge-2b); }
  @media (max-width: 860px) {
    .kind { display: none; }
  }
  .letter { min-width: 16px; text-align: right; }
  .odds { min-width: 48px; text-align: right; color: var(--edge-5); font-variant-numeric: tabular-nums; }

  .clash { color: var(--gold-1); font-size: 11px; cursor: help; }

  .check {
    display: flex;
    align-items: center;
    flex: none;
    background: none;
    border: none;
    padding: 0;
    cursor: pointer;
  }
  .check img { width: 18px; height: 18px; }
  .lrow .check img { width: 15px; height: 15px; }

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
  .link.armed,
  .btn.armed {
    color: #f0c0c0;
    background: rgba(180, 30, 30, 0.55);
    font-size: 10px;
    padding: 2px 6px;
  }

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
  .link:hover:not(:disabled) { color: var(--bone-13); }
  .link:disabled { opacity: 0.35; cursor: default; }
  /* A link inside a sentence, not a control beside one. It is its own class
     rather than `.link` plus a modifier: written that way the small-caps the
     other links wear went on winning, and "START FROM THE DROP RATES" shouted
     out of the middle of the empty state's one paragraph. */
  .prose {
    font: inherit;
    font-size: inherit;
    color: var(--bone-13);
    background: none;
    border: none;
    padding: 0;
    cursor: pointer;
    text-decoration: underline;
  }
  .prose:hover { color: var(--bone-15); }

  .note { color: var(--dim-2); font-size: 10px; line-height: 1.4; }
  /* a setting that is on but cannot act yet says so where it is set */
  .note.warn { color: var(--gold, #e8c860); }
  .notice { color: #45c15a; font-size: 10px; }

  .empty {
    color: var(--dim-2);
    text-align: center;
    font-size: 11px;
    line-height: 1.6;
    padding: 12px 8px;
  }

  /* WebView2 leaves a select alone; WebKitGTK draws it as a native widget with
     a pale background and a blue focus ring, which is a hole in the panel. The
     appearance is taken over completely, arrow included. */
  .picker {
    flex: 1 1 120px;
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
  .picker:focus-visible { outline: none; border-color: var(--edge-4); }
  /* the popup list is the toolkit's own window; these are the only two
     properties it honours */
  .picker option { background: var(--ground-7); color: var(--bone-9); }

  .rename { flex: 1 1 auto; min-width: 0; height: 26px; font: inherit; color: var(--bone-13); background: none; outline: none; padding: 0 6px; }

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
  .vol::-webkit-slider-runnable-track { height: 4px; background: var(--ground-7); border: 1px solid var(--ground-11); }
  .vol::-webkit-slider-thumb {
    -webkit-appearance: none;
    width: 11px;
    height: 11px;
    margin-top: -5px;
    background: var(--bone-6);
    border: 1px solid var(--ground-7);
  }
  .vol:hover::-webkit-slider-thumb { background: var(--bone-13); }

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
  .btn:hover:not(:disabled) { border-image-source: var(--btn-hover); }
  .btn:active:not(:disabled) { border-image-source: var(--btn-down); }
  .btn:disabled { opacity: 0.45; cursor: default; }
  .btn.sm { height: 22px; font-size: 10px; padding: 0 6px; }

  .c-sat { color: #d24b4b; }
  .c-set { color: #45c15a; }
  .c-her { color: #35d3c1; }
  .c-ang { color: var(--gold-1); }
  .c-unh { color: #e04a7a; }
</style>

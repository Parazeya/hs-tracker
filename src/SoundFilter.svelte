<script>
  import { invoke } from './bridge.js';
  import { art, css } from './skin.svelte.js';
  import { listen } from './bridge.js';
  import { ITEMS } from './items.js';
  import { RARITIES, soundUrl, play } from './audio.js';
  import { ALL_BUFFS } from './buffs.js';

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

  // Whether the buff list is on show. Local, and seeded from the setting
  // rather than stored beside it: what is persisted is the list, and a second
  // switch saying "the list is in use" is a second thing that can disagree
  // with it.
  // null means "not decided here yet", so the list itself answers. It has to
  // go back to null whenever the settings are replaced from outside — a tray
  // change, an import — or a player who had closed the picker would be shown
  // "every rotation alerts" while a list carried over from the new settings
  // quietly filtered them.
  let picking = $state(null);
  let pickingOn = $derived(picking ?? (settings?.zone_buffs?.length ?? 0) > 0);
  let buffsPicked = $derived(settings?.zone_buffs?.length ?? 0);
  let allBuffsPicked = $derived(buffsPicked === ALL_BUFFS.length);

  function togglePicking() {
    const on = !pickingOn;
    picking = on;
    // Switching the narrowing off switches it off, rather than hiding it: a
    // list that goes on filtering out of sight is a silence with nothing on the
    // page to account for it.
    if (!on && buffsPicked) {
      settings.zone_buffs = [];
      save();
    }
  }

  function toggleBuff(id) {
    const on = new Set(settings.zone_buffs ?? []);
    on.has(id) ? on.delete(id) : on.add(id);
    // sorted, so the settings file does not churn its order on every click
    settings.zone_buffs = [...on].sort((a, b) => a - b);
    save();
  }

  function tickAllBuffs() {
    settings.zone_buffs = ALL_BUFFS.map((b) => b.id);
    save();
  }

  function clearBuffs() {
    settings.zone_buffs = [];
    save();
  }

  // Every relic in the game, by the id the wire numbers it with.
  //
  // Type 16 is the relics and nothing else, and the table's ids run 0..155 with
  // no gaps — which is also every id seen dropping across the owner's captures.
  // So the id IS the relic, and that is what gets stored: the picker never
  // writes a name, because three relic names belong to another item as well
  // (`Shrunken Head` to a Satanic charm, `Death's Scythe` to a Set polearm,
  // `Satan's Horn` to a Common collectible) and the last shares the rarity too,
  // so no spelling could have separated it. `hunted_relic` matches the id.
  const RELIC_TYPE = 16;
  // Grouped by name, not one row per id. Two of the 156 are both called "Bomb"
  // — ids 65 and 150 — and as two rows they were two identical ticks: pick one,
  // and the other Bomb drops in silence with nothing on the screen to say why.
  // A player means "the relic called Bomb", so the row carries both ids and
  // ticks both. That leaves 155 rows for 156 relics, which is why the counts
  // below say 155.
  const ALL_RELICS = [
    ...Object.entries(ITEMS)
      .filter(([key]) => key.startsWith(`${RELIC_TYPE}:`))
      .reduce((by, [key, name]) => {
        const row = by.get(name) ?? { name, ids: [], key: name.toLowerCase() };
        row.ids.push(Number(key.split(':')[1]));
        return by.set(name, row);
      }, new Map())
      .values(),
  ].sort((a, b) => a.name.localeCompare(b.name));

  // How often a relic falls, measured over the owner's own captures: 827 of
  // them across 19.16 hours of active play. It is on the screen because "tick
  // all" reads like a harmless convenience and is not one.
  const RELICS_PER_HOUR = 43;

  let relicQuery = $state('');
  let relicsBrowsing = $state(false);
  // Counted in ROWS, so the number on the screen and the number of rows ticked
  // are the same thing. `settings.relics` holds ids and is one longer whenever
  // Bomb is hunted, which is the engine's business and not the note's.
  //
  // `every`, not `some`: only this picker writes the field and it always writes
  // a row's ids together, so a half-ticked Bomb can only come from a file
  // edited by hand — and showing that as unticked while one of them still
  // chimes is the error a player can hear and then act on.
  let pickedIds = $derived(new Set(settings?.relics ?? []));
  let relicsPicked = $derived(ALL_RELICS.filter((r) => r.ids.every((id) => pickedIds.has(id))).length);
  let allRelicsPicked = $derived(relicsPicked === ALL_RELICS.length);
  let relicVolume = $derived(Math.round((settings?.relic?.volume ?? 0.5) * 100));

  // At rest this shows what is being hunted, not the whole table: the question
  // a player has when they open the panel is "what am I listening for", and 156
  // rows do not answer it. Typing searches all of them; "show all" is there for
  // browsing, which is a different question and a rarer one.
  let relicShown = $derived.by(() => {
    const q = relicQuery.trim().toLowerCase();
    if (q) return ALL_RELICS.filter((r) => r.key.includes(q));
    if (relicsBrowsing) return ALL_RELICS;
    return ALL_RELICS.filter((r) => r.ids.every((id) => pickedIds.has(id)));
  });

  function toggleRelic(row) {
    const on = new Set(settings.relics ?? []);
    const hunted = row.ids.every((id) => on.has(id));
    for (const id of row.ids) (hunted ? on.delete(id) : on.add(id));
    // sorted, so the settings file does not churn its order on every click
    settings.relics = [...on].sort((a, b) => a - b);
    save();
  }

  function tickAllRelics() {
    settings.relics = ALL_RELICS.flatMap((r) => r.ids).sort((a, b) => a - b);
    save();
  }

  function clearRelics() {
    settings.relics = [];
    save();
  }

  function setVolume(key, v) {
    if (!settings?.[key] || settings[key].volume === v) return;
    settings[key].volume = v;
    save();
  }

  $effect(() => {
    invoke('get_settings').then((s) => (settings = s));
    refreshSounds();
    const unsubs = [
      listen('settings-changed', (e) => ((settings = e.payload), (picking = null))),
      listen('sounds-changed', () => refreshSounds()),
    ];
    return () => unsubs.forEach((u) => u.then((f) => f()));
  });

  function save() {
    clearTimeout(saveTimer);
    const snapshot = $state.snapshot(settings);
    saveTimer = setTimeout(() => invoke('save_settings', { settings: snapshot }).catch(() => {}), 150);
  }

  /// Whether a rarity makes a sound at all — the first control on this panel.
  ///
  /// It went the same way `setNumber` did, and for the same reason: the custom
  /// filter moved to its own tab and took the definitions with it while the
  /// callers stayed. An undefined call in a template throws where nobody
  /// looks, so every tick on the rarity rows stopped answering the mouse and
  /// the panel looked untouched. Found by auditing for the shape rather than
  /// by anyone reporting it, which is the only way this kind is found.
  function toggleAlert(rarity) {
    const on = new Set(settings.alerts ?? []);
    on.has(rarity) ? on.delete(rarity) : on.add(rarity);
    settings.alerts = [...on];
    save();
  }

  /// One number of the settings, written and saved.
  ///
  /// It went missing when the custom filter moved to its own tab — the half
  /// that used it stayed here and the definition left with the other half — and
  /// nothing said so: an undefined call in a template throws where nobody is
  /// looking, so Min tier, Size and Shading simply stopped answering the mouse
  /// and the panel looked fine. Reported as "can't change Min Tier" (#9).
  function setNumber(key, value) {
    if (!settings || !Number.isFinite(value) || settings[key] === value) return;
    settings[key] = value;
    save();
  }

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

</script>

<div class="panel two">
  <div class="col">
  {#if settings}
    <div class="section" style:border-image-source={css('chip_dark')}>
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
            <button class="btn sm" style:--btn={css('button')} style:--btn-hover={css('button_hover')} style:--btn-down={css('button_down')} onclick={() => testRarity(key)}>Test</button>
            <button class="btn sm" style:--btn={css('button')} style:--btn-hover={css('button_hover')} style:--btn-down={css('button_down')} onclick={() => pickRaritySound(key)}>Browse…</button>
            <button
              class="btn sm"
              style:--btn={css('button')}
              style:--btn-hover={css('button_hover')}
              style:--btn-down={css('button_down')}
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
          <button class="btn sm" style:--btn={css('button')} style:--btn-hover={css('button_hover')} style:--btn-down={css('button_down')} onclick={() => testRarity('mail')}>Test</button>
          <button class="btn sm" style:--btn={css('button')} style:--btn-hover={css('button_hover')} style:--btn-down={css('button_down')} onclick={() => pickRaritySound('mail')}>Browse…</button>
          <button
            class="btn sm"
            style:--btn={css('button')}
            style:--btn-hover={css('button_hover')}
            style:--btn-down={css('button_down')}
            disabled={!custom.mail}
            onclick={() => danger('snd-mail', () => invoke('clear_sound', { rarity: 'mail' }).catch(() => {}))}
          >{armed === 'snd-mail' ? 'Sure?' : 'Default'}</button>
        </div>
      </div>
      <!-- Also not a drop: the game moving the satanic zone, which is the one
           thing on the overlay worth leaving a fight for. One switch, because
           the chime and the chip's pulse are one alert told two ways — see
           App.svelte. The pillar has its own, under Announcement; which
           rotations count at all is the section below. -->
      <div class="rrow" class:off={!settings.zone?.enabled}>
        <button class="check" onclick={() => { settings.zone.enabled = !settings.zone.enabled; save(); }} aria-label="satanic zone change">
          <img src={settings.zone?.enabled ? art('check_on') : art('check_off')} alt="" />
        </button>
        <span class="rname c-zone" title="The satanic zone rotating: the chime, and the zone chip pulsing on the overlay">Zone change</span>
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
        <span class="src" title={custom.zone ? `sounds/${custom.zone}` : 'built-in sound — the zone chime'}>{custom.zone ?? 'built-in'}</span>
        <div class="rbtns">
          <button class="btn sm" style:--btn={css('button')} style:--btn-hover={css('button_hover')} style:--btn-down={css('button_down')} onclick={() => testRarity('zone')}>Test</button>
          <button class="btn sm" style:--btn={css('button')} style:--btn-hover={css('button_hover')} style:--btn-down={css('button_down')} onclick={() => pickRaritySound('zone')}>Browse…</button>
          <button
            class="btn sm"
            style:--btn={css('button')}
            style:--btn-hover={css('button_hover')}
            style:--btn-down={css('button_down')}
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

    <!-- Outside the `canAnnounce` guard below on purpose: this narrows the
         chime as much as the pillar, and the chime is the half that still works
         on a session with no overlay at all. -->
    <div class="section" style:border-image-source={css('chip_dark')}>
      <div class="sechead" data-tauri-drag-region>Zone buffs — which rotations are worth the alert</div>
      <div class="line">
        <button class="check" onclick={togglePicking} aria-label="narrow the zone alert">
          <img src={pickingOn ? art('check_on') : art('check_off')} alt="" />
        </button>
        <span class="opt" title="A rotation is announced only when the zone it lands on carries at least one of the buffs you tick">
          Only alert when the new zone rolls one of these buffs
        </span>
      </div>

      {#if !pickingOn}
        <div class="note">Every rotation alerts. Tick buffs to alert on fewer.</div>
      {:else if buffsPicked === 0}
        <!-- The one state a player can be misled by: the list is on show, it is
             empty, and empty is not silence. Say so where they are looking. -->
        <div class="note warn">Nothing ticked yet, so every rotation still alerts.</div>
      {:else if allBuffsPicked}
        <!-- Not quite the same as an empty list, and the difference is a
             silence: a rotation the game gives no buffs at all matches nothing
             on any list, and so does a buff this table does not know yet. An
             empty list asks no question and lets both through. -->
        <div class="note">
          Every buff in the table ticked. A rotation with no buffs at all, or with one this
          table does not know, still passes in silence — clear the list to hear every rotation.
        </div>
      {:else}
        <div class="note">{buffsPicked} ticked — a rotation with none of them passes in silence.</div>
      {/if}

      {#if pickingOn}
        <!-- Twenty-five rows is a third of the page, so they are only drawn
             when they are being used; the line above says what they say. -->
        <div class="grid buffs" class:none-yet={buffsPicked === 0}>
          {#each ALL_BUFFS as b (b.id)}
            {@const on = (settings.zone_buffs ?? []).includes(b.id)}
            <button class="secopt buff" class:off={!on} onclick={() => toggleBuff(b.id)} title="{b.name} : {b.desc}">
              <img src={on ? art('check_on') : art('check_off')} alt="" />
              <img class="bicon" src={b.icon} alt="" />
              <span class="bname">{b.name}</span>
            </button>
          {/each}
        </div>
        <div class="line ends">
          <button class="link" onclick={tickAllBuffs}>tick all {ALL_BUFFS.length}</button>
          <!-- Never "none": with an empty list meaning every rotation, a link
               named as the opposite of "all" lands on the same behaviour. -->
          <button class="link" class:armed={armed === 'zone-buffs'} onclick={() => danger('zone-buffs', clearBuffs)}>
            {armed === 'zone-buffs' ? 'clear it?' : 'clear — alert on every rotation'}
          </button>
        </div>
      {/if}
    </div>
  {/if}
  </div>

  <div class="col">
  {#if settings}
    <!-- Beside the buff picker, and the opposite way round. That list narrows
         an alert the game already makes, so an empty one lets every rotation
         through; this list IS the alert, so an empty one is silence. A player
         who has just read that section carries the wrong rule into this one —
         it is the next thing along in either layout — which is why the note
         below never leaves it unsaid. -->
    <div class="section" style:border-image-source={css('chip_dark')}>
      <div class="sechead" data-tauri-drag-region>Relics — a chime for the ones you are hunting</div>

      <div class="rrow" class:off={!settings.relic?.enabled}>
        <button class="check" onclick={() => { settings.relic.enabled = !settings.relic.enabled; save(); }} aria-label="relic chime">
          <img src={settings.relic?.enabled ? art('check_on') : art('check_off')} alt="" />
        </button>
        <span class="rname c-relic" title="A relic you ticked hitting the floor: the chime, the drop feed and the pillar">Relic</span>
        <input
          class="vol"
          type="range"
          min="0"
          max="100"
          disabled={!settings.relic?.enabled}
          value={relicVolume}
          oninput={(e) => setVolume('relic', e.currentTarget.value / 100)}
        />
        <span class="pct">{relicVolume}%</span>
        <span class="src" title={custom.relic ? `sounds/${custom.relic}` : 'the built-in relic chime'}>{custom.relic ?? 'built-in'}</span>
        <span class="btns">
          <button class="btn sm" style:--btn={css('button')} style:--btn-hover={css('button_hover')} style:--btn-down={css('button_down')} onclick={() => testRarity('relic')}>Test</button>
          <button class="btn sm" style:--btn={css('button')} style:--btn-hover={css('button_hover')} style:--btn-down={css('button_down')} onclick={() => pickRaritySound('relic')}>Browse…</button>
          <button
            class="btn sm"
            style:--btn={css('button')}
            style:--btn-hover={css('button_hover')}
            style:--btn-down={css('button_down')}
            disabled={!custom.relic}
            onclick={() => danger('snd-relic', () => invoke('clear_sound', { rarity: 'relic' }).catch(() => {}))}
          >{armed === 'snd-relic' ? 'Sure?' : 'Default'}</button>
        </span>
      </div>

      {#if !settings.relic?.enabled}
        <!-- The switch above answers before any of the rest of it does: with
             the chime off the engine is sent an empty list, so a screen that
             says "3 of 155 hunted" is describing a hunt that is not running.
             The grid goes with it, the way the buff picker's does. -->
        <div class="note">
          The relic chime is off, so none of these sound
          {#if relicsPicked}, including the {relicsPicked} ticked{/if}. Switch it on above.
        </div>
      {:else if relicsPicked === 0}
        <!-- The misleading state, and it is misleading the other way round from
             the buff picker's empty list: nothing ticked here is silence, not
             everything. Said in words, where they are looking. -->
        <div class="note warn">Nothing ticked, so no relic chimes. Search below and click one to hunt it.</div>
      {:else if allRelicsPicked}
        <!-- The other one. All 156 is not a filter at all, and the rate is the
             only thing that makes that concrete. -->
        <div class="note warn">
          All {ALL_RELICS.length} ticked — that is about {RELICS_PER_HOUR} chimes an hour, one every
          83 seconds, and every one of them reaches the drop feed too. Clear it and tick the few you
          are actually hunting.
        </div>
      {:else}
        <div class="note">
          {relicsPicked} of {ALL_RELICS.length} hunted — every other relic falls in silence.
        </div>
      {/if}

      {#if settings.relic?.enabled}
      <input
        class="field"
        style:border-image-source={css('chip_dark')}
        placeholder="search relics by name…"
        bind:value={relicQuery}
      />

      {#if relicShown.length}
        <div class="grid relics">
          {#each relicShown as r (r.name)}
            {@const on = r.ids.every((id) => pickedIds.has(id))}
            <button class="secopt relic" class:off={!on} onclick={() => toggleRelic(r)} title={r.name}>
              <img src={on ? art('check_on') : art('check_off')} alt="" />
              <span class="bname">{r.name}</span>
            </button>
          {/each}
        </div>
      {:else if relicQuery.trim()}
        <div class="note">no relic matches “{relicQuery.trim()}”.</div>
      {/if}

      <div class="line ends">
        <button class="link" onclick={() => (relicsBrowsing = !relicsBrowsing)}>
          {relicsBrowsing ? 'show only the ones I hunt' : `show all ${ALL_RELICS.length}`}
        </button>
        <!-- The one link on this panel that asks twice. "tick all" reads like
             the buff picker's harmless one above and is the opposite: there it
             restores the default, here it buys 43 chimes an hour. The cost is
             on the link rather than in a note beside it, because the note is
             not what gets clicked. -->
        <button class="link" class:armed={armed === 'relics-all'} onclick={() => danger('relics-all', tickAllRelics)}>
          {armed === 'relics-all'
            ? `all ${ALL_RELICS.length}? that is ~${RELICS_PER_HOUR} chimes an hour`
            : `tick all ${ALL_RELICS.length}`}
        </button>
        {#if relicsPicked}
          <button class="link" class:armed={armed === 'relics'} onclick={() => danger('relics', clearRelics)}>
            {armed === 'relics' ? 'clear it?' : 'clear — hunt no relic'}
          </button>
        {/if}
      </div>
      {/if}
    </div>

    {#if canAnnounce}
      <div class="section" style:border-image-source={css('chip_dark')}>
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
            <span class="opt" title="Anything on a list of the selected watchlist is announced, whatever its rarity or grade">
              Announce everything the watchlist names
            </span>
          </div>
          {#if settings.flourish_listed && !settings.use_filter}
            <!-- It used to say "switched off below", and below is where it was.
                 The watchlist has a tab of its own now, so the sentence has to
                 name it: a warning that points at a place the reader is already
                 looking at, and it is not there, reads as the warning being
                 wrong rather than the setting. -->
            <div class="note warn">
              The watchlist is switched off on its own tab, so this does nothing yet.
            </div>
          {/if}

          <div class="line">
            <button
              class="check"
              onclick={() => { settings.flourish_zone = !settings.flourish_zone; save(); }}
              aria-label="announce the satanic zone"
            >
              <img src={settings.flourish_zone ? art('check_on') : art('check_off')} alt="" />
            </button>
            <span class="opt" title="The satanic zone rotating gets the pillar too, drawn its own way — the zone and the buffs it rolled">
              Announce the satanic zone when it rotates
            </span>
          </div>
          {#if settings.flourish_zone && !settings.zone?.enabled}
            <div class="note">
              The chime for it is off above; the pillar still plays.
            </div>
          {/if}

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
                style:--btn={css('button')}
                style:--btn-hover={css('button_hover')}
                style:--btn-down={css('button_down')}
                onclick={() => invoke('place_flourish', { placing: true })}
              >
                Change location
              </button>
            </div>
          {/if}
        {/if}
      </div>
    {/if}

    <!-- A feature that moves without a sign left behind reads as a feature
         that was deleted. This is where the custom filter used to be. -->
    <div class="note pointer">
      Named items, and a sound of their own, live on the Watchlist tab.
    </div>
  {/if}
  </div>
</div>

<style>
  /* Two columns: what makes a sound at all on the left, the two narrower
     alerts and the pillar on the right. The right column used to be the custom
     filter, which now has a tab of its own — without a second column the page
     was one 1,150px-wide strip of rows on the owner's own 1400px window, with
     a rarity row's controls half a screen from its name.
     A narrow window falls back to one column, where side by side would leave
     neither half readable. */
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
    .panel.two { grid-template-columns: minmax(360px, 1fr) minmax(380px, 1fr); }
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

  .section {
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
  .secopt img { flex: none; }
  .secopt:hover { color: var(--bone-13); }
  /* Twenty-five names in two columns. The longest, "Artifact Excavator", is
     98px at this size against a 195px cell at the narrowest the window goes —
     so they are set rather than measured, and the ellipsis is a belt. */
  .buffs { gap: 2px 8px; }
  .buff { gap: 5px; min-width: 0; }
  /* Dimmed only once something is ticked, so the difference means something.
     Dimming every row the moment the picker opens — which is the state it
     always opens in — made the whole control read as disabled, at about 2:1
     against the panel. */
  .buff.off { opacity: 0.62; }
  .buffs.none-yet .buff.off { opacity: 1; }
  .buff img { width: 16px; height: 16px; }
  .buff .bicon { width: 18px; height: 18px; }
  .buff .bname {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 11px;
  }
  .line.ends { justify-content: space-between; gap: 10px; }

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
  .note.pointer { padding: 2px 2px 4px; }
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


  .link.armed {
    color: #f0c0c0;
    background: rgba(180, 30, 30, 0.55);
    font-size: 10px;
    padding: 2px 6px;
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
  /* Six rows carry three of these each; at full size they alone were 194px of
     a row that had 360 to live in. The class was on the buttons from the start
     and never had a rule. */
  .btn.sm { height: 22px; font-size: 10px; padding: 0 4px; }

  .c-sat { color: #d24b4b; }
  .c-set { color: #45c15a; }
  .c-her { color: #35d3c1; }
  .c-ang { color: var(--gold-1); }
  .c-unh { color: #e04a7a; }
  /* The rotation is not a rarity, and wearing the satanic red said it was —
     one more colour in a column of five that all mean "an item this good".
     This is the peach the announcement writes SATANIC ZONE in, which nothing
     that drops owns. Mail, the other row here that is not a rarity, is gold
     for the same reason. */
  .c-zone { color: #ffb08a; }
  /* Mail is not a rarity either, and this is the class it has always asked for
     — it was simply never defined here, and styles are scoped, so the row
     inherited the ordinary text colour and was the only one of the seven with
     no colour of its own. */
  .c-gold { color: var(--gold-1); }
  /* A hunted relic is not a rarity either — all 156 of them are Common, which
     is the whole reason this row exists instead of a rarity switch. Teal, so it
     reads as its own kind of alert beside the five colours that all mean "an
     item this good" and the peach the zone owns. */
  .c-relic { color: #7fd6c2; }

  /* 156 rows against the buffs' 25, so this one scrolls rather than growing the
     panel by a screen and a half. The height is a little over ten rows: enough
     that a search reads as a list, short enough that the links under it stay in
     view — they are how the state is undone, and a control whose escape hatch
     is below the fold is the one that traps people. */
  .relics {
    gap: 2px 8px;
    max-height: 168px;
    overflow-y: auto;
    padding-right: 4px;
  }
  .relic { gap: 5px; min-width: 0; }
  /* The buff grid dims what is not ticked. That cannot work here: the usual way
     to reach this grid is to search with nothing hunted yet, and every result
     would be dimmed at once — which is the state the buff picker learnt reads
     as "this whole control is disabled". So ticked is marked by GAINING the
     relic colour rather than by everything else losing brightness, and the
     checkbox says the same thing a second time. */
  .relic:not(.off) .bname { color: #7fd6c2; }
  .relic img { width: 16px; height: 16px; }
  .relic .bname {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 11px;
  }
</style>

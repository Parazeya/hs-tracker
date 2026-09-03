<script>
  // rendered either as its own window or as a dashboard section
  let { embedded = false } = $props();

  import { invoke } from './bridge.js';
  import { art, css } from './skin.svelte.js';
  import { listen } from './bridge.js';
  import { buffInfo, debuffInfo, zoneAct, zoneName } from './buffs.js';
  import { ITEMS, DROP_CHASE, DROP_PLACES, DROP_RATE, DROP_ZONES, RARITY_BY_NAME, TIER_BY_NAME, rarityByName, tierLabel, zoneCode } from './items.js';
  import { itemName, language, locale, nameOf, placeList, satanicZoneName, say, t, typeLabel } from './say.svelte.js';
  import { fmt, difficulty, RARITIES, RARITY_CLASS } from './format.js';

  // The four counters the backend keeps, and what to call them. Every resource
  // it names belongs to one of them; see `RESOURCES` in stats.rs.
  const RESOURCES = [
    ['keys', 'Keys'],
    ['materials', 'Materials'],
    ['socketables', 'Socketables'],
    ['collectibles', 'Collectibles'],
  ];

  let snap = $state(null);
  let extra = $state(null);
  // See App.svelte: a `bind:this` an effect reads has to be state, or it
  // works only for as long as nothing is ever conditional.
  let canvas = $state(null);

  // pushed by the backend, and only while this window is on screen
  let clock = $state({ secs: 0, at: Date.now() });
  function received(s) {
    snap = s;
    clock = { secs: s.session_secs, at: Date.now() };
  }

  // A canvas is outside the template, so nothing repaints it when the words
  // change: the two captions and the empty-state line would have kept the
  // language they were painted in. Reading language() here is what subscribes.
  $effect(() => {
    language();
    drawGraph();
  });

  $effect(() => {
    invoke('snapshot').then(received).catch(() => {});
    invoke('get_extra')
      .then((e) => {
        extra = e;
        drawGraph();
      })
      .catch(() => {});
    invoke('get_settings').then((s) => (settings = s));
    const unsubs = [
      listen('settings-changed', (e) => (settings = e.payload)),
      listen('stats', (e) => received(e.payload)),
      listen('stats-extra', (e) => {
        extra = e.payload;
        drawGraph();
      }),
    ];
    return () => unsubs.forEach((u) => u.then((f) => f()));
  });


  function dur(secs) {
    const h = Math.floor(secs / 3600);
    const m = Math.floor((secs % 3600) / 60);
    const s = Math.floor(secs % 60);
    return h > 0
      ? `${h}:${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`
      : `${m}:${String(s).padStart(2, '0')}`;
  }

  // One formatter, hoisted. `extra` arrives as a fresh object, so all 400
  // journal rows re-run their template on every push; a Date and a fresh
  // options literal per row costs about 10ms of blocking main thread each time,
  // against a fifth of a millisecond through one `Intl.DateTimeFormat`.
  //
  // Kept per language rather than built once, or it would go on reading the
  // clock in whatever language the page loaded in.
  let TIME = null;
  let timeIn = '';
  const time = (ms) => {
    const want = locale();
    if (want !== timeIn) {
      TIME = new Intl.DateTimeFormat(want, { hour: '2-digit', minute: '2-digit', second: '2-digit' });
      timeIn = want;
    }
    return TIME.format(ms);
  };


  const item = (name) => snap?.items?.[name] ?? { total: 0, mf: 0, per_hour: 0 };

  // the character save counts these; they only appear once something has moved
  let bosses = $derived((snap?.tallies ?? []).filter((t) => t.group === 'boss'));
  let chests = $derived((snap?.tallies ?? []).filter((t) => t.group === 'chest'));
  // Neither killed nor opened: a Chaos Tower floor and a wormhole are counted
  // when they are CLEARED, so they are counted apart from bosses — under
  // Bosses, walking up a staircase reads as a kill.
  let cleared = $derived((snap?.tallies ?? []).filter((t) => t.group === 'clear'));

  let charSub = $derived.by(() => {
    const c = extra?.character;
    if (!c) return t('waiting for character…');
    const parts = [];
    if (c.name) parts.push(c.name);
    parts.push(`${t('Lv')} ${c.level}`, `${t('HLv')} ${c.herolevel}`, difficulty(c.difficulty, c.hell_sub));
    if (c.hardcore) parts.push(t('HC'));
    return parts.join(' · ');
  });

  // gold, xp and kills only travel when the game saves the character or banks
  // gold; a stale number is the game being quiet, not the tracker being stuck
  const ago = (secs) => (secs < 90 ? `${secs}${t('s')}` : `${Math.floor(secs / 60)}${t('m')}`);
  let lag = $derived.by(() => {
    const save = snap?.save_age_secs;
    const bank = snap?.bank_age_secs;
    const parts = [];
    if (save != null && save >= 45) parts.push(`${t('character save')} ${ago(save)} ${t('ago')}`);
    if (bank != null && bank >= 45) parts.push(`${t('balance')} ${ago(bank)} ${t('ago')}`);
    if (save == null && bank == null) {
      // the totals on screen are then last run's, marked with an asterisk
      return snap?.carried_bank || snap?.carried_totals
        ? t('waiting for the first game save — gold, xp and kills arrive with it; * marks totals carried over from the last run')
        : t('waiting for the first game save — gold, xp and kills arrive with it');
    }
    return parts.length ? `${t('last from the game')} · ${parts.join(' · ')}` : '';
  });

  // Four fit, and a rotation can carry five, so the count is shown: two columns
  // cut silently at four read as complete when they are not.
  const SHOWN = 4;

  /// Relics. They reach this timeline only when one is being hunted, and they
  /// are hunted from Alerts by identity rather than by name — see the drop rows
  /// below, and `hunted_relic` in the engine.
  const RELIC = 16;
  let allBuffs = $derived(snap?.satanic_zone?.buffs ?? []);
  let allDebuffs = $derived(snap?.satanic_zone?.debuffs ?? []);
  let buffs = $derived(allBuffs.slice(0, SHOWN).map(buffInfo));
  let debuffs = $derived(allDebuffs.slice(0, SHOWN).map(debuffInfo));
  let moreBuffs = $derived(Math.max(0, allBuffs.length - SHOWN));
  let moreDebuffs = $derived(Math.max(0, allDebuffs.length - SHOWN));

  // the window is resizable, so the graph is redrawn at whatever size the box
  // ends up being rather than stretched from the size it was first drawn at
  $effect(() => {
    if (!canvas) return;
    const observer = new ResizeObserver(() => drawGraph());
    observer.observe(canvas);
    return () => observer.disconnect();
  });

  // zones rotate on the half hour (:00 / :30), aligned to the wall clock
  let nowTick = $state(Date.now());
  $effect(() => {
    const t = setInterval(() => (nowTick = Date.now()), 1000);
    return () => clearInterval(t);
  });
  let zoneReset = $derived.by(() => {
    const d = new Date(nowTick);
    const next = new Date(d);
    next.setMinutes(d.getMinutes() < 30 ? 30 : 60, 0, 0);
    return {
      at: next.toLocaleTimeString(locale(), { hour: '2-digit', minute: '2-digit' }),
      in: dur(Math.max(0, Math.floor((next.getTime() - nowTick) / 1000))),
    };
  });

  // Whether the zone on screen is still the one the server is running.
  //
  // The game asks the server only as part of saving, and can go twenty minutes
  // without doing so. The rotation does not wait: stand still across :30 and
  // this box goes on naming the previous half hour's zone, with its buffs and
  // its drops, beside a countdown that has already run out.
  //
  // Nothing here can ask the server — it only reads the replies the game gets —
  // so the answer is to stop stating a stale zone as fact.
  let zoneStale = $derived.by(() => {
    const at = snap?.satanic_at;
    if (!at) return false;
    const since = new Date(nowTick);
    since.setMinutes(since.getMinutes() < 30 ? 0 : 30, 0, 0);
    return at < since.getTime();
  });
  let zoneSeen = $derived(
    snap?.satanic_at
      ? new Date(snap.satanic_at).toLocaleTimeString(locale(), { hour: '2-digit', minute: '2-digit' })
      : ''
  );

  function dropLabel(d) {
    if (d.name) return d.name;
    const known = itemName(d.item_type, d.item_id, d.weapon_type);
    if (known) return known;
    if (d.item_id > 0) return `${typeLabel(d.item_type, d.weapon_type)} #${d.item_id}`;
    const parts = [];
    if (d.item_type > 0) parts.push(typeLabel(d.item_type, d.weapon_type));
    if (d.seed > 0) parts.push(`${t('Seed')} ${String(d.seed).slice(-6)}`);
    return parts.join(' · ') || t('Unknown item');
  }

  // Fixed, so the names beside it line up — but how wide it has to be is a fact
  // about the language: "Satanic" is seven characters and "Сатанинский" eleven.
  // Taken from the words rather than widened for the worst case, so English
  // keeps the width it had.
  let rarWidth = $derived(
    Math.max(54, ...RARITIES.map((name) => Math.ceil(t(name).length * 6.2))),
  );

  function dropRarity(d) {
    if (d.rarity) return d.rarity;
    const byName = rarityByName(dropLabel(d));
    return byName ?? 'Drop';
  }

  // rolling per-hour rates from the 15s cumulative series
  function rates() {
    const s = extra?.series ?? [];
    const out = [];
    const K = 4;
    for (let i = 1; i < s.length; i++) {
      const j = Math.max(0, i - K);
      const dt = s[i].t - s[j].t;
      if (dt <= 0) continue;
      out.push({
        t: s[i].t,
        gold: ((s[i].gold - s[j].gold) * 3600) / dt,
        xp: ((s[i].xp - s[j].xp) * 3600) / dt,
      });
    }
    return out;
  }

  function drawGraph() {
    if (!canvas) return;
    // The canvas is stretched by the layout, so its backing store is sized to
    // whatever the box currently is — otherwise the browser scales a 506px
    // bitmap up and the labels smear along with it.
    const box = canvas.getBoundingClientRect();
    const W = Math.max(1, Math.round(box.width));
    const H = Math.max(1, Math.round(box.height));
    const dpr = window.devicePixelRatio || 1;
    if (canvas.width !== Math.round(W * dpr) || canvas.height !== Math.round(H * dpr)) {
      canvas.width = Math.round(W * dpr);
      canvas.height = Math.round(H * dpr);
    }
    const ctx = canvas.getContext('2d');
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, W, H);
    // Canvas does not resolve var(): the assignment is dropped and whatever
    // was set last stands, which for the empty-state line was black on a
    // near-black panel — invisible on every fresh session. Read per draw, not
    // once: the canvas is resized on every observer step, which resets the
    // context, and a season remaps both tokens.
    const skin = getComputedStyle(canvas);
    const token = (name, fallback) => skin.getPropertyValue(name).trim() || fallback;
    // Gold is the accent in the game skin and gold is what it means, so one
    // token served both. Under a skin whose accent is violet it stopped
    // meaning anything: the gold line and the xp line came out the same
    // colour and the graph had one line in it as far as the eye went.
    const GOLD = token('--graph-gold', token('--gold-2', '#e8c860'));
    // the xp line was written out here, which is fine until a skin needs it
    // darker: #a06ae0 measures 3.0 on paper and the line disappeared into it
    const XP = token('--graph-xp', '#a06ae0');
    const FAINT = token('--edge-8', '#8a7a5a');
    const FACE = `11px ${token('--face', "'CookieRun Bold', sans-serif")}`;
    const data = rates();
    if (data.length < 2) {
      ctx.fillStyle = FAINT;
      ctx.font = FACE;
      ctx.fillText(t('the graph appears after a couple of minutes of farming'), 10, H / 2 + 4);
      return;
    }
    const t0 = data[0].t;
    const t1 = data[data.length - 1].t;
    const span = Math.max(1, t1 - t0);
    const maxGold = Math.max(...data.map((d) => d.gold), 1);
    const maxXp = Math.max(...data.map((d) => d.xp), 1);
    const px = (t) => ((t - t0) / span) * (W - 8) + 4;
    // A skin can ask for the area under the line to be filled. Only the plain
    // one does: the game's panel is a lit painting already and a wash of colour
    // on it reads as dirt, while on a flat near-black card the line alone is a
    // scratch on a wall. Read as a token rather than passed in, because this is
    // the same place the two colours come from.
    const FILL = token('--graph-fill', '') === 'on';
    const line = (key, max, color) => {
      const at = (d) => [px(d.t), H - 6 - (d[key] / max) * (H - 22)];
      // an eight-digit hex is the cheapest way to an alpha of a colour we were
      // handed, and the only shape either token ever takes
      if (FILL && /^#[0-9a-f]{6}$/i.test(color)) {
        const wash = ctx.createLinearGradient(0, 0, 0, H);
        wash.addColorStop(0, color + '38');
        wash.addColorStop(1, color + '00');
        ctx.beginPath();
        ctx.moveTo(px(data[0].t), H);
        data.forEach((d) => ctx.lineTo(...at(d)));
        ctx.lineTo(px(data[data.length - 1].t), H);
        ctx.closePath();
        ctx.fillStyle = wash;
        ctx.fill();
      }
      ctx.beginPath();
      data.forEach((d, i) => (i === 0 ? ctx.moveTo(...at(d)) : ctx.lineTo(...at(d))));
      ctx.strokeStyle = color;
      ctx.lineWidth = 1.5;
      ctx.stroke();
    };
    line('gold', maxGold, GOLD);
    line('xp', maxXp, XP);
    // the two captions sit side by side however wide the box gets
    ctx.font = FACE;
    ctx.textBaseline = 'top';
    ctx.fillStyle = GOLD;
    ctx.fillText(`${t('gold/h peak')} ${fmt(Math.round(maxGold))}`, 8, 4);
    ctx.fillStyle = XP;
    const xp = `${t('xp/h peak')} ${fmt(Math.round(maxXp))}`;
    ctx.fillText(xp, Math.max(W / 2, W - 8 - ctx.measureText(xp).width), 4);
  }

  const rarityCls = {
    Satanic: 'c-sat',
    Heroic: 'c-her',
    Angelic: 'c-ang',
    Unholy: 'c-unh',
    Mythic: 'c-myt',
    Set: 'c-set',
    Runeword: 'c-gold',
  };

  // Most items drop anywhere; a few hundred are tied to an act, its dungeons or
  // its bosses. Knowing which ones is the difference between farming here on
  // purpose and farming here out of habit.
  const TIED = Object.entries(DROP_ZONES).map(([key, codes]) => ({ key, codes }));
  const PROPER = new Map(Object.values(ITEMS).map((n) => [n.toLowerCase(), n]));

  const odds = (rate) =>
    !rate
      ? ''
      : rate >= 1e6
        ? `1/${(rate / 1e6).toFixed(rate >= 1e7 ? 0 : 1)}M`
        : rate >= 1e3
          ? `1/${(rate / 1e3).toFixed(rate >= 1e4 ? 0 : 1)}k`
          : `1/${rate}`;

  // Only what is tied to one patch of ground. Tied is not exclusive: the item
  // drops anywhere, it just rolls on a far better chance there — the one the
  // game prints in green — so that is the number worth showing.
  const detail = (key) => {
    const base = DROP_RATE[key] ?? 0;
    const chase = DROP_CHASE[key] ?? base;
    return {
      name: PROPER.get(key) ?? key,
      rarity: RARITY_BY_NAME[key],
      tier: TIER_BY_NAME[key] ?? 0,
      rate: chase,
      hint: `${t('1 in')} ${chase.toLocaleString(locale())} ${t('in the zone')}, ${t('1 in')} ${base.toLocaleString(locale())} ${t('anywhere')}${
        DROP_PLACES[key] ? ` · ${placeList(DROP_PLACES[key], ', ')}` : ''
      }`,
    };
  };
  const byWorth = (a, b) => b.tier - a.tier || a.rate - b.rate;
  const pick = (want) => TIED.filter(({ codes }) => codes.some(want)).map(({ key }) => detail(key)).sort(byWorth);

  // The satanic zone, not the room the player is standing in.
  //
  // Where the player is standing is the one thing the game will not say. The
  // room appears only in the game's own state packet — since the August 2026
  // patch about twenty times rarer than it was, and mostly while the map is
  // open — and nothing else on the wire carries it: not the save's fields, not
  // the drop packets, not the market, not the chat. A list built on it sits for
  // hours on a zone the player has left.
  //
  // The satanic zone is the opposite: the server announces it by name, over and
  // over, because every client has to agree on it. It is also the zone worth
  // reading a drop list for.
  let szCode = $derived(zoneCode(snap?.satanic_zone?.zone));
  let here = $derived(szCode ? pick((c) => c === szCode) : []);

  // The game says outright when the character is standing in the satanic zone,
  // but that flag rides the same scarce heartbeat the room did — so it is held
  // against the act, which every save write states. Walk into another act and
  // the chip goes, even though no heartbeat has said so yet.
  let szHere = $derived(Boolean(snap?.satanic_here && snap?.act && zoneAct(snap?.satanic_zone?.zone) === snap.act));

  // A drop worth hearing next time is easiest to add the moment it lands, so
  // the timeline can push a name straight into a list of the active filter.
  let settings = $state(null);
  let adding = $state(null);

  /// Bring a just-opened popup fully into the scroller it lives in.
  ///
  /// `block: 'nearest'` moves the list as little as it can, so a picker that is
  /// already visible does not make the page jump under the cursor.
  function reveal(node) {
    node.scrollIntoView({ block: 'nearest', inline: 'nearest' });
  }
  let added = $state(null);
  let addedTimer;

  let lists = $derived.by(() => {
    const filter = (settings?.filters ?? []).find((f) => f.id === settings?.filter);
    return filter?.lists ?? [];
  });

  function addTo(list, name) {
    adding = null;
    if (!name || list.items.some((n) => n.toLowerCase() === name.toLowerCase())) return;
    list.items = [...list.items, name].sort((a, b) => a.localeCompare(b));
    invoke('save_settings', { settings: $state.snapshot(settings) }).catch(() => {});
    added = `${name} → ${list.name}`;
    clearTimeout(addedTimer);
    addedTimer = setTimeout(() => (added = null), 2500);
  }

</script>

<div class="panel">
  <div class="body">
    <!-- what the run is doing right now: three numbers and the clock -->
    <div class="run" data-tauri-drag-region>
      <button
        class="clock"
        class:held={snap?.paused}
        style:border-image-source={css('chip_dark')}
        onclick={() => invoke('set_paused', { paused: !snap?.paused }).catch(() => {})}
        title={t("Stop the clock. The counters keep counting — what a pause changes is what they are divided by. Ctrl+Shift+P")}
      >
        <div class="value">
          {snap?.paused ? '' : ''}{snap ? dur(clock.secs + (snap.paused ? 0 : (nowTick - clock.at) / 1000)) : '0:00'}
          {#if snap?.paused}<img class="frost" src={art('frozen_icon')} alt="" />{/if}
        </div>
        <div class="sub">
          {snap?.finished
            ? t('the run is over')
            : snap?.paused
              ? t('paused — click to carry on')
              : charSub}
        </div>
      </button>
      <div class="card" style:border-image-source={css('chip_dark')}>
        <div class="label">{t("Gold")}</div>
        <div class="value c-gold">{fmt(snap?.gold?.earned)}</div>
        <div class="sub" title={snap?.carried_bank ? t('the balance the last run ended on — the game has not sent a new one yet') : t('bank balance as the game last reported it')}>
          {fmt(snap?.gold?.per_hour)}{t('/h')} · {t('bank')} {fmt(snap?.gold?.total)}{snap?.carried_bank ? ' *' : ''}
        </div>
      </div>
      <div class="card" style:border-image-source={css('chip_dark')}>
        <div class="label">{t("XP")}</div>
        <div class="value c-xp">{fmt(snap?.xp?.earned)}</div>
        <div class="sub" title={t("the big number is what this session earned; 'in level' is the game's own bar — the experience banked towards the next hero level")}>
          {fmt(snap?.xp?.per_hour)}{t('/h')} · {t('in level')} {fmt(snap?.xp?.total)}
        </div>
      </div>
      <div class="card" style:border-image-source={css('chip_dark')}>
        <div class="label">{t("Kills")}</div>
        <div class="value c-her">{fmt(snap?.kills?.earned)}</div>
        <div class="sub" title={snap?.carried_totals ? t('the total the last run ended on — the game has not saved the character yet') : t('lifetime total as the game last saved it')}>
          {fmt(snap?.kills?.per_hour)}{t('/h')} · {t('total')} {fmt(snap?.kills?.total)}{snap?.carried_totals ? ' *' : ''}
        </div>
      </div>
    </div>

    {#if lag}
      <div class="lag" data-tauri-drag-region>{lag}</div>
    {/if}

    <!-- Ending a run is a thing the player does, not a side effect of clearing
         the counters. Reset files the run and the next second is already part
         of the next one; this files it and then waits, so the walk to town and
         the hour in the stash land in no run at all. -->
    <div class="ends" data-tauri-drag-region>
      {#if snap?.finished}
        <button class="end open" onclick={() => invoke('start_run').catch(() => {})}>
          {t('Start a new run')}
        </button>
        <span class="dim">{t('Nothing is counted until it does.')}</span>
      {:else}
        <button class="end" onclick={() => invoke('finish_run').catch(() => {})}>
          {t('Finish the run')}
        </button>
        <span class="dim">{t('Files it in Runs and stops counting.')}</span>
      {/if}
    </div>

    <div class="cols">
      <!-- left: what dropped -->
      <div class="col">
        <div class="box" style:border-image-source={css('chip_dark')}>
          <div class="box-head"><span class="accent">{t("Loot")}</span><span class="right">{t("this session")}</span></div>
          <div class="rows">
            <div class="row colhead">
              <span class="rowname"></span>
              <span class="rowval">{t("drops")}</span>
              <!-- The game's own claim, not ours: it flags the drop as owed to
                   magic find and we only count what it flagged. -->
              <span class="rowmf" title={t("Of those, the ones the game itself credited to Magic Find")}>{t('mf')}</span>
              <span class="rowrate">{t("per hour")}</span>
            </div>
            {#each RARITIES as name}
              {@const it = item(name)}
              <div class="row">
                <span class="rowname {RARITY_CLASS[name]}">{t(name)}</span>
                <span class="rowval {RARITY_CLASS[name]}">{fmt(it.total)}</span>
                <span class="rowmf c-blue" title={t("credited to Magic Find by the game")}>{it.mf ? fmt(it.mf) : '—'}</span>
                <span class="dim rowrate">{fmt(it.per_hour)}/h</span>
              </div>
            {/each}
          </div>
          <div class="subhead">{t("Notable")}</div>
          <div class="tally">
            {#each snap?.notable ?? [] as n}
              <div class="tallyrow"><span class="dim">{t(nameOf(n.label))}</span><b class="c-gold">{fmt(n.total)}</b></div>
            {/each}
          </div>

          <div class="subhead">{t("Resources")}</div>
          <div class="tally">
            {#each RESOURCES as [kind, label]}
              <div class="tallyrow"><span class="dim">{t(label)}</span><b>{fmt(snap?.resources?.[kind])}</b></div>
              {#each (snap?.resource_items ?? []).filter((r) => r.kind === kind) as r (r.name)}
                <div class="tallyrow named"><span class="dim">{t(nameOf(r.name))}</span><b>{fmt(r.total)}</b></div>
              {/each}
            {/each}
          </div>
        </div>

        {#if bosses.length || chests.length || cleared.length}
          <div class="box" style:border-image-source={css('chip_dark')}>
            <div class="box-head"><span class="accent">{t("Killed & opened")}</span><span class="right">{t("this session")}</span></div>
            {#if bosses.length}
              <div class="subhead">{t("Bosses")}</div>
              <div class="tally">
                {#each bosses as b}
                  <div class="tallyrow"><span class="dim">{t(nameOf(b.label))}</span><b class="c-sat">{fmt(b.total)}</b></div>
                {/each}
              </div>
            {/if}
            {#if chests.length}
              <div class="subhead">{t("Chests")}</div>
              <div class="tally">
                {#each chests as c}
                  <div class="tallyrow"><span class="dim">{t(nameOf(c.label))}</span><b class="c-gold">{fmt(c.total)}</b></div>
                {/each}
              </div>
            {/if}
            {#if cleared.length}
              <div class="subhead">{t("Cleared")}</div>
              <div class="tally">
                {#each cleared as c}
                  <div class="tallyrow"><span class="dim">{t(nameOf(c.label))}</span><b class="c-gold">{fmt(c.total)}</b></div>
                {/each}
              </div>
            {/if}
          </div>
        {/if}

    <div class="box grow" style:border-image-source={css('chip_dark')}>
      <div class="box-head">
        <span class="accent">{t("Item timeline")}</span>
        {#if added}<span class="added">{added}</span>{/if}
        <span class="right">{say('{n} drops', { n: extra?.drops?.length ?? 0 })}</span>
      </div>
      <div class="list" style:--rar-w="{rarWidth}px">
        {#each extra?.drops ?? [] as d}
          <div class="drop">
            <span class="ts">{time(d.ts_ms)}</span>
            <span class="rar {rarityCls[dropRarity(d)] ?? ''}">{t(dropRarity(d))}</span>
            <span class="name {rarityCls[dropRarity(d)] ?? ''}" title={nameOf(dropLabel(d))}>{nameOf(dropLabel(d))}</span>
            <span class="dim tier">{tierLabel(d.tier)}</span>
            <span class="c-blue mf">{d.mf ? t('MF') : ''}</span>
            {#if d.announced}<span class="dim">{t("server")}</span>{/if}
            <!-- Not for a relic, however well `dropLabel` names one. A relic
                 arrives on the wire with no name at all — every one of them is
                 Common, and three share a name with another item — so it is
                 alerted on by identity in Alerts, and `listed_sound` has
                 nothing to match a list entry against. Offering the button here
                 would put "Jungle Vial" on a list that can never fire, which
                 reads as a broken filter rather than as the wrong door. -->
            {#if lists.length && dropLabel(d) && d.item_type !== RELIC}
              <button class="tolist" title={t("Add to a sound list")} onclick={() => (adding = adding === d.ts_ms ? null : d.ts_ms)}>+</button>
              {#if adding === d.ts_ms}
                <!-- The timeline scrolls, and this opens inside it: on a drop
                     in the lower half the list was simply cut off by the
                     scroller. Scrolled to rather than repositioned, because
                     the row it belongs to has to stay visible with it. -->
                <div
                  class="picker"
                  use:reveal
                >
                  {#each lists as list}
                    <button onclick={() => addTo(list, dropLabel(d))}>{list.name}</button>
                  {/each}
                </div>
              {/if}
            {/if}
          </div>
        {:else}
          <div class="dim empty">{t("nothing yet — valuable drops land here")}</div>
        {/each}
      </div>
    </div>
      </div>

      <!-- right: where you are and how the run is trending -->
      <div class="col">
    <div class="box" style:border-image-source={css('chip_dark')}>
      <div class="box-head">
        <span class="accent">{t("Satanic Zone")}</span>
        {#if zoneStale}
          <span class="stale" title={t("the rotation has come round since the game last asked the server, and the game asks only when it saves")}> {t("unconfirmed")} </span>
        {/if}
        <span class="right">{t('resets in')} {zoneReset.in} · {t('at')} {zoneReset.at}</span>
      </div>
      {#if snap?.satanic_zone}
        <div class="szname" class:unsure={zoneStale}>
          {satanicZoneName(snap.satanic_zone.zone, zoneName(snap.satanic_zone.zone))}
          {#if zoneStale}<span class="seen">{t('last confirmed')} {zoneSeen}</span>{/if}
        </div>
        <div class="effects">
          <div class="effcol">
            <div class="effhead pros">{t("Pros")}</div>
            {#each buffs as b}
              <div class="buffrow">
                <img src={b.icon} alt="" />
                <div>
                  <div class="buffname">{b.name}</div>
                  <div class="buffdesc" title={b.desc}>{b.desc}</div>
                </div>
              </div>
            {:else}
              <div class="buffdesc">—</div>
            {/each}
            <!-- said rather than left off: four fit and a rotation can carry
                 five, and a list that stops without saying so reads as the
                 whole of it -->
            {#if moreBuffs}<div class="buffdesc more">+{moreBuffs} {t('more')}</div>{/if}
          </div>
          <div class="effcol">
            <div class="effhead cons">{t("Cons")}</div>
            {#each debuffs as d}
              <div class="buffrow">
                <div>
                  <div class="buffname cons">{d.name}</div>
                  <div class="buffdesc" title={d.desc}>{d.desc}</div>
                </div>
              </div>
            {:else}
              <div class="buffdesc">—</div>
            {/each}
            {#if moreDebuffs}<div class="buffdesc more">+{moreDebuffs} {t('more')}</div>{/if}
          </div>
        </div>
      {:else}
        <div class="sub center">{t("no satanic zone data yet")}</div>
      {/if}
    </div>

    <div class="box" style:border-image-source={css('chip_dark')}>
      <div class="box-head">
        <span class="accent">{t("Drops in the Satanic Zone")}</span>
        {#if zoneStale}
          <span class="stale" title="{t('this list is for the zone last confirmed at')} {zoneSeen}; {t('the rotation has come round since')}"> {t("unconfirmed")} </span>
        {/if}
        {#if szHere}
          <span class="szhere" title={t("the game reports you as standing in it")}>{t("you are here")}</span>
        {/if}
        <span class="right" title={t("the zone the server is running satanic this hour")}>
          {#if snap?.satanic_zone}{satanicZoneName(snap.satanic_zone.zone, zoneName(snap.satanic_zone.zone))}
          {:else}{t('waiting for the game')}{/if}
        </span>
      </div>
      <div class="vitals">
        <span class="dim">{t("Level")}</span>
        <b>{snap?.character?.level || '—'}</b>
        <span class="dim">{t("Hero")}</span>
        <b>{snap?.character?.herolevel || '—'}</b>
        <span class="dim" title={t("the act the character save last stated — the game no longer names the zone often enough to show one")}>{t("Act")}</span>
        <b>{snap?.act || '—'}</b>
      </div>
      {#if here.length}
        <div class="tied">
          {#each here as it}
            <div class="drop">
              <span class="name {rarityCls[it.rarity] ?? ''}" title={it.hint}>{nameOf(it.name)}</span>
              <span class="dim tier">{tierLabel(it.tier)}</span>
              <span class="dim odds" title={it.hint}>{odds(it.rate)}</span>
            </div>
          {/each}
        </div>
      {:else}
        <div class="dim empty">
          {szCode
            ? t('nothing rolls better there than it does anywhere else')
            : t('the zone appears once the game announces it')}
        </div>
      {/if}
    </div>

        <div class="box" style:border-image-source={css('chip_dark')}>
          <div class="box-head"><span class="accent">{t("Session rates")}</span></div>
          <canvas bind:this={canvas}></canvas>
        </div>
      </div>
    </div>
  </div>
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
    font-family: var(--face);
    font-size: 12px;
    color: var(--bone-6);
  }

  .cards {
    display: flex;
    gap: 6px;
    flex: none;
  }

  .card {
    box-sizing: border-box;
    flex: 1;
    min-width: 0;
    overflow: hidden;
    border: 6px solid transparent;
    border-image-slice: 6 fill;
    border-image-width: 6px;
    image-rendering: pixelated;
    padding: 2px 8px 4px;
  }

  .label {
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--bone-4);
  }
  .value { font-size: 19px; line-height: 22px; }

  .resline {
    flex: none;
    display: flex;
    gap: 18px;
    justify-content: center;
    font-size: 10px;
    color: var(--edge-8);
  }
  .sub {
    font-size: 10px;
    color: var(--edge-8);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .center { text-align: center; padding: 6px 0; }

  .box {
    box-sizing: border-box;
    flex: none;
    border: 6px solid transparent;
    border-image-slice: 6 fill;
    border-image-width: 6px;
    image-rendering: pixelated;
    padding: 4px 8px 6px;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }

  /* the panel scrolls as a whole, so the timeline keeps a fixed frame */
  .box.grow { flex: none; height: 190px; }

  .lag {
    flex: none;
    font-size: 10px;
    color: var(--dim-2);
    text-align: center;
    margin-top: -2px;
  }

  .added { color: #45c15a; font-size: 10px; margin-left: 8px; }

  .tied {
    max-height: 190px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .tied::-webkit-scrollbar { width: 6px; }
  .tied::-webkit-scrollbar-thumb { background: var(--dim-1); border-radius: 3px; }
  .tied .where { min-width: 84px; text-align: right; }
  .note { font-size: 10px; line-height: 1.4; padding: 4px 2px 0; }

  .tolist {
    flex: none;
    font: inherit;
    font-size: 11px;
    color: var(--bone-3);
    background: rgba(0, 0, 0, 0.3);
    border: 1px solid var(--ground-10);
    padding: 0 5px;
    cursor: pointer;
  }
  .tolist:hover { color: var(--bone-13); border-color: var(--edge-4); }

  .picker {
    position: absolute;
    right: 8px;
    top: 100%;
    z-index: 4;
    display: flex;
    flex-direction: column;
    background: var(--ground-5);
    border: 1px solid var(--edge-2);
    padding: 2px;
  }
  .picker button {
    font: inherit;
    font-size: 11px;
    color: var(--bone-6);
    background: none;
    border: none;
    text-align: left;
    padding: 3px 8px;
    cursor: pointer;
  }
  .picker button:hover { background: rgba(var(--pick-rgb), 0.55); color: var(--bone-13); }

  .body {
    flex: 1 1 auto;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding-right: 2px;
  }
  .body::-webkit-scrollbar { width: 6px; }
  .body::-webkit-scrollbar-thumb { background: var(--dim-1); border-radius: 3px; }

  /* An `fr` track's floor is its item's min-content, and `.clock` — unlike the
     three cards beside it — had no `min-width: 0`. `.clock .sub` is the whole
     character line and does not wrap, so the length of the character's name
     set the width of Gold, XP and Kills: an 18-character name took 275px of a
     171px track and pushed the three figures out over their own frames. */
  .run {
    flex: none;
    display: grid;
    grid-template-columns: minmax(0, 1.4fr) repeat(3, minmax(0, 1fr));
    gap: 6px;
  }

  .clock {
    box-sizing: border-box;
    min-width: 0;
    overflow: hidden;
    border: 6px solid transparent;
    border-image-slice: 6 fill;
    border-image-width: 6px;
    image-rendering: pixelated;
    padding: 6px 8px;
    display: flex;
    flex-direction: column;
    justify-content: center;
  }
  .clock {
    /* it is a button now, but it is still the same tile */
    font: inherit;
    text-align: left;
    cursor: pointer;
    background: none;
  }
  .clock .value { font-size: 20px; color: var(--bone-13); display: flex; align-items: center; gap: 6px; }
  .clock:hover .sub { color: var(--bone-8); }
  /* held: the game's own ice, on the tile whose clock has stopped */
  .clock.held .value { color: #bfe4ff; }
  .clock.held .sub { color: #7fa8c4; }
  .clock .frost { width: 16px; height: 16px; image-rendering: pixelated; }

  .cols {
    flex: 1 1 auto;
    min-height: 380px;
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 6px;
  }
  @media (max-width: 720px) {
    .cols { grid-template-columns: 1fr; }
    .run { grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); }
  }

  .col {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .rows { display: flex; flex-direction: column; gap: 1px; }
  .rows .row {
    display: flex;
    align-items: baseline;
    gap: 8px;
    padding: 3px 4px;
    background: rgba(0, 0, 0, 0.2);
  }
  .rows .row:nth-child(even) { background: rgba(0, 0, 0, 0.1); }
  .rowname { flex: 1 1 auto; min-width: 0; }
  .rowval { min-width: 44px; text-align: right; font-size: 13px; }
  /* narrow on purpose: it is a footnote to the count beside it, not a column
     anyone reads down */
  .rowmf { min-width: 32px; text-align: right; font-size: 10px; }
  .rowrate { min-width: 54px; text-align: right; font-size: 10px; }

  /* the numbers on their own said nothing; the header says what they are */
  .rows .row.colhead {
    background: none;
    color: var(--edge-2b);
    font-size: 9px;
    letter-spacing: 0.4px;
    text-transform: uppercase;
    padding-bottom: 1px;
  }
  .row.colhead .rowval,
  .row.colhead .rowmf { font-size: 9px; }

  .subhead {
    color: var(--edge-2b);
    font-size: 9px;
    letter-spacing: 0.4px;
    text-transform: uppercase;
    padding: 6px 2px 2px;
  }

  /* counted things read as a table of values, not as buttons */
  .tally {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(128px, 1fr));
    column-gap: 14px;
  }
  .tallyrow {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 8px;
    padding: 2px 2px 2px 0;
    border-bottom: 1px solid rgba(58, 43, 43, 0.7);
  }
  /* one resource under the count it belongs to */
  .tallyrow.named { padding-left: 12px; border-bottom-color: rgba(58, 43, 43, 0.35); }
  .tallyrow.named span, .tallyrow.named b { font-size: 10px; opacity: 0.78; }
  .tallyrow span { font-size: 11px; }
  .tallyrow b { font-size: 12px; color: var(--bone-6); }

  .ends {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 2px 6px;
  }
  .ends .dim { font-size: 10px; }
  .end {
    padding: 3px 10px;
    color: var(--bone-6);
    background: rgba(0, 0, 0, 0.35);
    border: 1px solid rgba(58, 43, 43, 0.9);
    border-radius: 4px;
    font: inherit;
    font-size: 11px;
    cursor: pointer;
  }
  .end:hover { color: var(--bone-8); border-color: var(--bone-6); }
  .end.open { color: var(--gold-2); border-color: rgba(120, 96, 40, 0.9); }

  .box-head {
    flex: none;
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--bone-4);
    margin-bottom: 3px;
  }
  .accent { color: #ca4545; }
  .right { color: var(--edge-8); text-transform: none; letter-spacing: 0; }

  .szname { font-size: 15px; margin-bottom: 4px; }
  /* Named, but no longer vouched for. Dimmed rather than hidden: it is still
     the best answer there is, and a player who has not moved is probably still
     standing in it. */
  .szname.unsure { color: var(--bone-7); }
  .seen { font-size: 11px; color: var(--bone-7); margin-left: 6px; }
  .stale {
    font-size: 11px;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: #d8a24a;
    margin-left: 6px;
  }

  .effects {
    display: flex;
    gap: 12px;
  }
  .effcol {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .effhead {
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }
  .effhead.pros { color: #00c88a; }
  .effhead.cons { color: #ca4545; }
  .buffrow {
    display: flex;
    gap: 8px;
    align-items: center;
    min-width: 0;
  }
  .buffrow img { width: 21px; height: 21px; flex: none; }
  .buffrow > div { min-width: 0; }
  .buffname { font-size: 12px; color: var(--bone-9); line-height: 14px; }
  .buffname.cons { color: #d09090; }
  .buffdesc {
    font-size: 10px;
    color: var(--edge-8);
    line-height: 12px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  canvas { display: block; flex: none; width: 100%; height: 84px; }

  .list {
    flex: 1 1 auto;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .list::-webkit-scrollbar { width: 6px; }
  .list::-webkit-scrollbar-thumb { background: var(--dim-1); border-radius: 3px; }

  .drop {
    position: relative;
    display: flex;
    gap: 8px;
    align-items: baseline;
    white-space: nowrap;
    flex: none;
  }
  .ts { color: var(--edge-6); font-size: 11px; width: 62px; flex: none; }
  /* Fixed, so the names beside it line up — but how wide it has to be is a
     fact about the language, not about English. See `rarWidth`. */
  .rar { width: var(--rar-w, 54px); flex: none; font-size: 11px; }
  .name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; }
  /* fixed, right-aligned trailing columns — with an auto-width chance the
     grade drifted a few pixels on every row */
  .tier { width: 28px; flex: none; text-align: right; }
  .odds { width: 54px; flex: none; text-align: right; }
  .mf { width: 20px; flex: none; }
  .dim { color: var(--edge-8); font-size: 11px; }
  .zone { overflow: hidden; text-overflow: ellipsis; }
  .empty { padding: 8px 0; text-align: center; width: 100%; }

  .c-ang { color: #f6f794; }
  .c-her { color: #00ffae; }
  .c-sat { color: var(--rar-satanic); }
  .c-blue { color: var(--mf); }
  .c-myt { color: #c060e0; }
  .c-unh { color: #e04a7a; }
  .c-set { color: #40d040; }
  .c-ble { color: var(--bone-14); }
  .vitals {
    display: flex;
    align-items: baseline;
    gap: 6px;
    padding: 4px 2px 2px;
    font-size: 12px;
  }
  .vitals b { font-size: 13px; color: var(--bone-11); }
  .vitals .dim + b { margin-right: 8px; }
  .szhere {
    font-size: 11px;
    color: var(--rar-satanic);
    margin-left: 8px;
  }

  .c-gold { color: var(--gold-2); }
  .c-xp { color: #a06ae0; }
  .buffdesc.more { opacity: 0.75; font-style: italic; }
</style>

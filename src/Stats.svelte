<script>
  // rendered either as its own window or as a dashboard section
  let { embedded = false } = $props();

  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { buffInfo, debuffInfo, zoneName } from './buffs.js';
  import {
    ITEMS,
    DROP_CHASE,
    DROP_PLACES,
    DROP_RATE,
    DROP_ZONES,
    RARITY_BY_NAME,
    TIER_BY_NAME,
    itemName,
    rarityByName,
    tierLabel,
    typeLabel,
    zoneCode,
    zoneLabel,
  } from './items.js';

  import chipBg from './assets/game/chip_dark.png';

  let snap = $state(null);
  let extra = $state(null);
  let canvas;

  // pushed by the backend, and only while this window is on screen
  let clock = $state({ secs: 0, at: Date.now() });
  function received(s) {
    snap = s;
    clock = { secs: s.session_secs, at: Date.now() };
  }

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

  function fmt(n) {
    const v = n ?? 0;
    const abs = Math.abs(v);
    if (abs >= 1e9) return `${(v / 1e9).toFixed(2)}kkk`;
    if (abs >= 1e6) return `${(v / 1e6).toFixed(2)}kk`;
    if (abs >= 10_000) return `${(v / 1e3).toFixed(1)}k`;
    return v.toLocaleString('en-US');
  }

  function dur(secs) {
    const h = Math.floor(secs / 3600);
    const m = Math.floor((secs % 3600) / 60);
    const s = Math.floor(secs % 60);
    return h > 0
      ? `${h}:${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`
      : `${m}:${String(s).padStart(2, '0')}`;
  }

  const time = (ms) =>
    new Date(ms).toLocaleTimeString('en-GB', { hour: '2-digit', minute: '2-digit', second: '2-digit' });

  const DIFFICULTIES = ['Normal', 'Nightmare', 'Hell'];

  const item = (name) => snap?.items?.[name] ?? { total: 0, mf: 0, per_hour: 0 };

  let charSub = $derived.by(() => {
    const c = extra?.character;
    if (!c) return 'waiting for character…';
    const parts = [];
    if (c.name) parts.push(c.name);
    parts.push(`Lv ${c.level}`, `HLv ${c.herolevel}`, DIFFICULTIES[c.difficulty] ?? `D${c.difficulty}`);
    if (c.hardcore) parts.push('HC');
    return parts.join(' · ');
  });

  // gold, xp and kills only travel when the game saves the character or banks
  // gold; a stale number is the game being quiet, not the tracker being stuck
  const ago = (secs) => (secs < 90 ? `${secs}s` : `${Math.floor(secs / 60)}m`);
  let lag = $derived.by(() => {
    const save = snap?.save_age_secs;
    const bank = snap?.bank_age_secs;
    const parts = [];
    if (save != null && save >= 45) parts.push(`character save ${ago(save)} ago`);
    if (bank != null && bank >= 45) parts.push(`balance ${ago(bank)} ago`);
    if (save == null && bank == null) {
      // the totals on screen are then last run's, marked with an asterisk
      return snap?.carried_bank || snap?.carried_totals
        ? 'waiting for the first game save — gold, xp and kills arrive with it; * marks totals carried over from the last run'
        : 'waiting for the first game save — gold, xp and kills arrive with it';
    }
    return parts.length ? `last from the game · ${parts.join(' · ')}` : '';
  });

  let buffs = $derived((snap?.satanic_zone?.buffs ?? []).slice(0, 4).map(buffInfo));
  let debuffs = $derived((snap?.satanic_zone?.debuffs ?? []).slice(0, 4).map(debuffInfo));

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
      at: next.toLocaleTimeString('en-GB', { hour: '2-digit', minute: '2-digit' }),
      in: dur(Math.max(0, Math.floor((next.getTime() - nowTick) / 1000))),
    };
  });

  function dropLabel(d) {
    if (d.name) return d.name;
    const known = itemName(d.item_type, d.item_id, d.weapon_type);
    if (known) return known;
    if (d.item_id > 0) return `${typeLabel(d.item_type, d.weapon_type)} #${d.item_id}`;
    const parts = [];
    if (d.item_type > 0) parts.push(typeLabel(d.item_type, d.weapon_type));
    if (d.seed > 0) parts.push(`Seed ${String(d.seed).slice(-6)}`);
    return parts.join(' · ') || 'Unknown item';
  }

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
    const data = rates();
    if (data.length < 2) {
      ctx.fillStyle = '#8a7a5a';
      ctx.font = '11px sans-serif';
      ctx.fillText('the graph appears after a couple of minutes of farming', 10, H / 2 + 4);
      return;
    }
    const t0 = data[0].t;
    const t1 = data[data.length - 1].t;
    const span = Math.max(1, t1 - t0);
    const maxGold = Math.max(...data.map((d) => d.gold), 1);
    const maxXp = Math.max(...data.map((d) => d.xp), 1);
    const px = (t) => ((t - t0) / span) * (W - 8) + 4;
    const line = (key, max, color) => {
      ctx.beginPath();
      data.forEach((d, i) => {
        const x = px(d.t);
        const y = H - 6 - (d[key] / max) * (H - 22);
        i === 0 ? ctx.moveTo(x, y) : ctx.lineTo(x, y);
      });
      ctx.strokeStyle = color;
      ctx.lineWidth = 1.5;
      ctx.stroke();
    };
    line('gold', maxGold, '#e8c860');
    line('xp', maxXp, '#a06ae0');
    // the two captions sit side by side however wide the box gets
    ctx.font = '11px sans-serif';
    ctx.textBaseline = 'top';
    ctx.fillStyle = '#e8c860';
    ctx.fillText(`gold/h peak ${fmt(Math.round(maxGold))}`, 8, 4);
    ctx.fillStyle = '#a06ae0';
    const xp = `xp/h peak ${fmt(Math.round(maxXp))}`;
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

  // Only what is tied to the ground under your feet. Tied is not exclusive:
  // the item drops anywhere, it just rolls on a far better chance here — the
  // one the game prints in green — so that is the number worth showing.
  let here = $derived.by(() => {
    const code = zoneCode(snap?.room);
    if (!code) return [];
    return TIED.filter(({ codes }) => codes.includes(code))
      .map(({ key }) => {
        const base = DROP_RATE[key] ?? 0;
        const chase = DROP_CHASE[key] ?? base;
        return {
          name: PROPER.get(key) ?? key,
          rarity: RARITY_BY_NAME[key],
          tier: TIER_BY_NAME[key] ?? 0,
          rate: chase,
          hint: `1 in ${chase.toLocaleString('en-US')} here, 1 in ${base.toLocaleString('en-US')} anywhere${
            DROP_PLACES[key] ? ` · ${DROP_PLACES[key].join(', ')}` : ''
          }`,
        };
      })
      .sort((a, b) => b.tier - a.tier || a.rate - b.rate);
  });

  // A drop worth hearing next time is easiest to add the moment it lands, so
  // the timeline can push a name straight into a list of the active filter.
  let settings = $state(null);
  let adding = $state(null);
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
      <div class="clock" style:border-image-source="url({chipBg})">
        <div class="value">{snap ? dur(clock.secs + (nowTick - clock.at) / 1000) : '0:00'}</div>
        <div class="sub">{charSub}</div>
      </div>
      <div class="card" style:border-image-source="url({chipBg})">
        <div class="label">Gold</div>
        <div class="value c-gold">{fmt(snap?.gold.earned)}</div>
        <div class="sub" title={snap?.carried_bank ? 'the balance the last run ended on — the game has not sent a new one yet' : 'bank balance as the game last reported it'}>
          {fmt(snap?.gold.per_hour)}/h · bank {fmt(snap?.gold.total)}{snap?.carried_bank ? ' *' : ''}
        </div>
      </div>
      <div class="card" style:border-image-source="url({chipBg})">
        <div class="label">XP</div>
        <div class="value c-xp">{fmt(snap?.xp.earned)}</div>
        <div class="sub" title="the big number is what this session earned; 'in level' is the game's own bar — the experience banked towards the next hero level">
          {fmt(snap?.xp.per_hour)}/h · in level {fmt(snap?.xp.total)}
        </div>
      </div>
      <div class="card" style:border-image-source="url({chipBg})">
        <div class="label">Kills</div>
        <div class="value c-her">{fmt(snap?.kills.earned)}</div>
        <div class="sub" title={snap?.carried_totals ? 'the total the last run ended on — the game has not saved the character yet' : 'lifetime total as the game last saved it'}>
          {fmt(snap?.kills.per_hour)}/h · total {fmt(snap?.kills.total)}{snap?.carried_totals ? ' *' : ''}
        </div>
      </div>
    </div>

    {#if lag}
      <div class="lag" data-tauri-drag-region>{lag}</div>
    {/if}

    <div class="cols">
      <!-- left: what dropped -->
      <div class="col">
        <div class="box" style:border-image-source="url({chipBg})">
          <div class="box-head"><span class="accent">Loot</span><span class="right">this session</span></div>
          <div class="rows">
            <div class="row colhead">
              <span class="rowname"></span>
              <span class="rowval">drops</span>
              <span class="rowrate">per hour</span>
            </div>
            {#each [['Satanic', item('Satanic'), 'c-sat'], ['Set', item('Set'), 'c-set'], ['Heroic', item('Heroic'), 'c-her'], ['Angelic', item('Angelic'), 'c-ang'], ['Unholy', item('Unholy'), 'c-unh']] as [name, it, cls]}
              <div class="row">
                <span class="rowname {cls}">{name}</span>
                <span class="rowval {cls}">{fmt(it.total)}</span>
                <span class="dim rowrate">{fmt(it.per_hour)}/h</span>
              </div>
            {/each}
          </div>
          <div class="subhead">Notable</div>
          <div class="tally">
            {#each snap?.notable ?? [] as n}
              <div class="tallyrow"><span class="dim">{n.label}</span><b class="c-gold">{fmt(n.total)}</b></div>
            {/each}
          </div>

          <div class="subhead">Resources</div>
          <div class="tally">
            {#each [['Keys', snap?.resources?.keys], ['Materials', snap?.resources?.materials], ['Socketables', snap?.resources?.socketables], ['Collectibles', snap?.resources?.collectibles]] as [label, value]}
              <div class="tallyrow"><span class="dim">{label}</span><b>{fmt(value)}</b></div>
            {/each}
          </div>
        </div>

    <div class="box grow" style:border-image-source="url({chipBg})">
      <div class="box-head">
        <span class="accent">Item timeline</span>
        {#if added}<span class="added">{added}</span>{/if}
        <span class="right">{extra?.drops?.length ?? 0} drops</span>
      </div>
      <div class="list">
        {#each extra?.drops ?? [] as d}
          <div class="drop">
            <span class="ts">{time(d.ts_ms)}</span>
            <span class="rar {rarityCls[dropRarity(d)] ?? ''}">{dropRarity(d)}</span>
            <span class="name {rarityCls[dropRarity(d)] ?? ''}" title={dropLabel(d)}>{dropLabel(d)}</span>
            <span class="dim tier">{tierLabel(d.tier)}</span>
            <span class="c-blue mf">{d.mf ? 'MF' : ''}</span>
            {#if d.announced}<span class="dim">server</span>{/if}
            {#if lists.length && dropLabel(d)}
              <button class="tolist" title="Add to a sound list" onclick={() => (adding = adding === d.ts_ms ? null : d.ts_ms)}>+</button>
              {#if adding === d.ts_ms}
                <div class="picker">
                  {#each lists as list}
                    <button onclick={() => addTo(list, dropLabel(d))}>{list.name}</button>
                  {/each}
                </div>
              {/if}
            {/if}
          </div>
        {:else}
          <div class="dim empty">nothing yet — valuable drops land here</div>
        {/each}
      </div>
    </div>
      </div>

      <!-- right: where you are and how the run is trending -->
      <div class="col">
    <div class="box" style:border-image-source="url({chipBg})">
      <div class="box-head">
        <span class="accent">Satanic Zone</span>
        <span class="right">resets in {zoneReset.in} · at {zoneReset.at}</span>
      </div>
      {#if snap?.satanic_zone}
        <div class="szname">{zoneName(snap.satanic_zone.zone)}</div>
        <div class="effects">
          <div class="effcol">
            <div class="effhead pros">Pros</div>
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
          </div>
          <div class="effcol">
            <div class="effhead cons">Cons</div>
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
          </div>
        </div>
      {:else}
        <div class="sub center">no satanic zone data yet</div>
      {/if}
    </div>

    <div class="box" style:border-image-source="url({chipBg})">
      <div class="box-head">
        <span class="accent">Drops in this area</span>
        <span class="right">{snap?.room ? zoneLabel(snap.room) : 'waiting for the game'}</span>
      </div>
      {#if here.length}
        <div class="tied">
          {#each here as it}
            <div class="drop">
              <span class="name {rarityCls[it.rarity] ?? ''}" title={it.hint}>{it.name}</span>
              <span class="dim tier">{tierLabel(it.tier)}</span>
              <span class="dim odds" title={it.hint}>{odds(it.rate)}</span>
            </div>
          {/each}
        </div>
      {:else}
        <div class="dim empty">
          {snap?.room
            ? 'nothing rolls better here than it does anywhere else'
            : 'the area appears once the game reports it'}
        </div>
      {/if}
    </div>

        <div class="box" style:border-image-source="url({chipBg})">
          <div class="box-head"><span class="accent">Session rates</span></div>
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
    font-family: 'CookieRun Bold', sans-serif;
    font-size: 12px;
    color: #c3af75;
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
    color: #9a8a68;
  }
  .value { font-size: 19px; line-height: 22px; }

  .resline {
    flex: none;
    display: flex;
    gap: 18px;
    justify-content: center;
    font-size: 10px;
    color: #8a7a5a;
  }
  .sub {
    font-size: 10px;
    color: #8a7a5a;
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
    color: #7b6a63;
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
  .tied::-webkit-scrollbar-thumb { background: #4a3a3a; border-radius: 3px; }
  .tied .where { min-width: 84px; text-align: right; }
  .note { font-size: 10px; line-height: 1.4; padding: 4px 2px 0; }

  .tolist {
    flex: none;
    font: inherit;
    font-size: 11px;
    color: #8d7d63;
    background: rgba(0, 0, 0, 0.3);
    border: 1px solid #3a2b2b;
    padding: 0 5px;
    cursor: pointer;
  }
  .tolist:hover { color: #f0e0b0; border-color: #7a4a4a; }

  .picker {
    position: absolute;
    right: 8px;
    z-index: 4;
    display: flex;
    flex-direction: column;
    background: #1d1414;
    border: 1px solid #5a3a3a;
    padding: 2px;
  }
  .picker button {
    font: inherit;
    font-size: 11px;
    color: #c3af75;
    background: none;
    border: none;
    text-align: left;
    padding: 3px 8px;
    cursor: pointer;
  }
  .picker button:hover { background: rgba(150, 37, 56, 0.55); color: #f0e0b0; }

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
  .body::-webkit-scrollbar-thumb { background: #4a3a3a; border-radius: 3px; }

  .run {
    flex: none;
    display: grid;
    grid-template-columns: 1.4fr 1fr 1fr 1fr;
    gap: 6px;
  }

  .clock {
    box-sizing: border-box;
    border: 6px solid transparent;
    border-image-slice: 6 fill;
    border-image-width: 6px;
    image-rendering: pixelated;
    padding: 6px 8px;
    display: flex;
    flex-direction: column;
    justify-content: center;
  }
  .clock .value { font-size: 20px; color: #f0e0b0; }

  .cols {
    flex: 1 1 auto;
    min-height: 380px;
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 6px;
  }
  @media (max-width: 720px) {
    .cols { grid-template-columns: 1fr; }
    .run { grid-template-columns: 1fr 1fr; }
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
  .rowrate { min-width: 54px; text-align: right; font-size: 10px; }

  /* the numbers on their own said nothing; the header says what they are */
  .rows .row.colhead {
    background: none;
    color: #8d5f5f;
    font-size: 9px;
    letter-spacing: 0.4px;
    text-transform: uppercase;
    padding-bottom: 1px;
  }
  .row.colhead .rowval { font-size: 9px; }

  .subhead {
    color: #8d5f5f;
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
  .tallyrow span { font-size: 11px; }
  .tallyrow b { font-size: 12px; color: #c3af75; }

  .box-head {
    flex: none;
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: #9a8a68;
    margin-bottom: 3px;
  }
  .accent { color: #ca4545; }
  .right { color: #8a7a5a; text-transform: none; letter-spacing: 0; }

  .szname { font-size: 15px; margin-bottom: 4px; }

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
  .buffname { font-size: 12px; color: #e0cc90; line-height: 14px; }
  .buffname.cons { color: #d09090; }
  .buffdesc {
    font-size: 10px;
    color: #8a7a5a;
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
  .list::-webkit-scrollbar-thumb { background: #4a3a3a; border-radius: 3px; }

  .drop {
    position: relative;
    display: flex;
    gap: 8px;
    align-items: baseline;
    white-space: nowrap;
    flex: none;
  }
  .ts { color: #7a6a4e; font-size: 11px; width: 62px; flex: none; }
  .rar { width: 54px; flex: none; font-size: 11px; }
  .name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; }
  /* fixed, right-aligned trailing columns — with an auto-width chance the
     grade drifted a few pixels on every row */
  .tier { width: 28px; flex: none; text-align: right; }
  .odds { width: 54px; flex: none; text-align: right; }
  .mf { width: 20px; flex: none; }
  .dim { color: #8a7a5a; font-size: 11px; }
  .zone { overflow: hidden; text-overflow: ellipsis; }
  .empty { padding: 8px 0; text-align: center; width: 100%; }

  .c-ang { color: #f6f794; }
  .c-her { color: #00ffae; }
  .c-sat { color: #ca1717; }
  .c-blue { color: #5050ae; }
  .c-myt { color: #c060e0; }
  .c-unh { color: #e04a7a; }
  .c-set { color: #40d040; }
  .c-ble { color: #f0e8b0; }
  .c-gold { color: #e8c860; }
  .c-xp { color: #a06ae0; }
</style>

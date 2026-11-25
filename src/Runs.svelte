<script>
  import { nameOf, say, t, locale, zoneLabel } from './say.svelte.js';
  import { invoke, listen } from './bridge.js';
  import { fmt, RARITIES, RARITY_CLASS, difficulty } from './format.js';
  import { art } from './skin.svelte.js';
  import { tierLabel } from './items.js';
  import { cardPng, drawRunCard } from './runcard.js';
  import {
    COMPARISON_MINUTES, MIN_COMPARISONS, dominantZone, medianRate, perHour, runComparison, runDifficulty,
  } from './run-comparison.js';

  let runs = $state([]);
  let picked = $state(null);
  let zoneFilter = $state(null);
  let difficultyFilter = $state(null);

  // A run is filed when the session ends — the reset button, the hotkey, the
  // tray, the game closing, the app quitting. So the list only grows while this
  // panel is open if one of those happens, and the event says when.
  $effect(() => {
    // The list gains entries at the front, so the index of whatever is being
    // read moves under it: a run filed while this panel was open slid the
    // detail pane onto its neighbour. `started_ms` is the run's own identity
    // and does not move.
    const load = () =>
      invoke('get_runs')
        .then((list) => {
          runs = list ?? [];
          if (picked === null) picked = runs[0]?.started_ms ?? null;
        })
        .catch(() => {});
    load();
    const unsubs = [listen('runs-changed', load)];
    return () => unsubs.forEach((u) => u.then((f) => f()));
  });
  function dur(secs) {
    const h = Math.floor(secs / 3600);
    const m = Math.floor((secs % 3600) / 60);
    return h > 0 ? `${h}${t('h')} ${String(m).padStart(2, '0')}${t('m')}` : `${m}${t('m')}`;
  }

  const day = (ms) =>
    new Date(ms).toLocaleString(locale(), { day: '2-digit', month: 'short', hour: '2-digit', minute: '2-digit' });

  const zoneName = (room) => room ? zoneLabel(room) : t('Unknown zone');
  const difficultyName = (entry) => {
    if (!entry.key) return t('Unknown difficulty');
    return entry.key === '2:0' ? t('Hell (grade unknown)') : difficulty(entry.difficulty, entry.hellSub);
  };
  const rateText = (rate) => rate === null ? '—' : `${fmt(rate)}${t('/h')}`;
  let groups = $derived(runs.map((r) => ({ run: r, zone: dominantZone(r), difficulty: runDifficulty(r) })));
  let zones = $derived([...new Set(groups.map((entry) => entry.zone))]
    .sort((a, b) => zoneName(a).localeCompare(zoneName(b), locale())));
  let difficulties = $derived([...new Map(groups.map((entry) => [entry.difficulty.key, entry.difficulty])).values()]
    .sort((a, b) => (a.difficulty ?? Infinity) - (b.difficulty ?? Infinity) || (a.hellSub ?? 0) - (b.hellSub ?? 0)));
  let visible = $derived(groups.filter((entry) =>
    (zoneFilter === null || entry.zone === zoneFilter) &&
    (difficultyFilter === null || entry.difficulty.key === difficultyFilter),
  ));
  let selected = $derived(visible.find((entry) => entry.run.started_ms === picked) ?? visible[0]);
  let run = $derived(selected?.run ?? null);
  let comparison = $derived(run ? runComparison(run, runs) : null);
  let baselines = $derived(comparison?.longEnough ? RATES.map(([, , of]) => medianRate(comparison.peers, of)) : []);

  // what a run is worth in one line, for the list
  const headline = (r) => `${fmt(r.gold)} ${t('gold')} · ${fmt(r.kills)} ${t('kills')}`;

  const drops = (r) => r.items ? RARITIES.reduce((sum, name) => sum + (r.items[name] ?? 0), 0) : null;

  // The four numbers a run is read by, in the order the panel shows them.
  const RATES = [
    ['Gold', 'c-gold', (r) => r.gold],
    ['XP', 'c-xp', (r) => r.xp],
    ['Kills', 'c-her', (r) => r.kills],
    ['Drops', '', drops],
  ];

  const against = (now, was) => now !== null && was > 0 ? Math.round(((now - was) / was) * 100) : null;

  // The bosses and chests a run put down. They are filed with every run and
  // drawn on the card you can copy from here, and were the one thing on that
  // picture the panel behind it could not show you. Runs filed before 0.9.8
  // carry none, which is what the empty guard is for.
  const tallies = (r, group) => (r.tallies ?? []).filter((t) => t.group === group && t.total > 0);
  const bosses = (r) => tallies(r, 'boss');
  const chests = (r) => tallies(r, 'chest');
  // Cleared rather than killed — see the note beside TALLIES in stats.rs.
  const cleared = (r) => tallies(r, 'clear');

  let armed = $state(false);

  // The card is a picture on purpose: a run pasted into a chat as text loses
  // its shape, and a screenshot of the panel drags the whole window along.
  // Which button was pressed, so the answer lands on that one rather than on
  // whichever happens to be first.
  let carded = $state({ which: '', text: '' });
  let cardTimer = null;
  async function copyCard(mode) {
    const snapshot = $state.snapshot(run);
    clearTimeout(cardTimer);
    try {
      // the card is drawn with the app's own font; canvas will fall back to a
      // system one unless it is loaded before the first fillText
      await document.fonts?.load?.('16px "CookieRun Bold"');
      const coin = new Image();
      coin.src = art('coin_strip');
      await coin.decode().catch(() => {});
      const canvas = drawRunCard(snapshot, { coin }, { mode });
      await invoke('copy_image', await cardPng(canvas));
      carded = { which: mode, text: 'copied' };
    } catch (e) {
      carded = { which: mode, text: String(e) };
    }
    cardTimer = setTimeout(() => (carded = { which: '', text: '' }), 2500);
  }
  const cardLabel = (mode, idle) =>
    carded.which !== mode
      ? idle
      : carded.text === 'copied'
        ? t('copied — paste it anywhere')
        : carded.text;

  // Below four there is nothing a ledger would add: the summary card's own
  // line already prints every name, so the full card would be a bigger picture
  // of the same thing.
  let worthFull = $derived((run?.notable?.length ?? 0) > 3);
  let armTimer;
  function clearAll() {
    if (!armed) {
      armed = true;
      clearTimeout(armTimer);
      armTimer = setTimeout(() => (armed = false), 4000);
      return;
    }
    armed = false;
    invoke('clear_runs')
      .then(() => {
        runs = [];
        picked = null;
        clearFilters();
      })
      .catch(() => {});
  }

  function clearFilters() {
    zoneFilter = null;
    difficultyFilter = null;
  }
</script>

<div class="panel">
  {#if runs.length}
    <div class="cols">
      <div class="list" style:border-image-source="url({art('chip_dark')})">
        <div class="head">
          <span class="accent">{t("Runs")}</span>
          <span class="right">
            {visible.length === runs.length ? runs.length : say('{n} of {all}', { n: visible.length, all: runs.length })}
          </span>
        </div>
        <div class="filters">
          <select aria-label={t('Dominant zone')} bind:value={zoneFilter}>
            <option value={null}>{t('All zones')}</option>
            {#each zones as room}<option value={room}>{zoneName(room)}</option>{/each}
          </select>
          <select aria-label={t('Difficulty')} bind:value={difficultyFilter}>
            <option value={null}>{t('All difficulties')}</option>
            {#each difficulties as entry}<option value={entry.key}>{difficultyName(entry)}</option>{/each}
          </select>
        </div>
        <div class="scroll">
          {#each visible as entry (entry.run.started_ms)}
            {@const r = entry.run}
            <button class="row" class:on={entry === selected} aria-pressed={entry === selected} onclick={() => (picked = r.started_ms)}>
              <span class="when">{day(r.started_ms)}</span>
              <span class="len">{dur(r.secs)}</span>
              <span class="place" title={`${zoneName(entry.zone)} · ${difficultyName(entry.difficulty)}`}>
                {zoneName(entry.zone)} · {difficultyName(entry.difficulty)}
              </span>
              <span class="sum">{headline(r)}</span>
            </button>
          {/each}
        </div>
        <button
          class="btn"
          class:armed
          style:--btn="url({art('button')})"
          style:--btn-hover="url({art('button_hover')})"
          style:--btn-down="url({art('button_down')})"
          onclick={clearAll}
        >
          {armed ? t('Sure? — this cannot be undone') : t('Clear history')}
        </button>
      </div>

      {#if run && comparison}
        <div class="detail">
          <div class="box" style:border-image-source="url({art('chip_dark')})">
            <div class="head">
              <span class="accent">{day(run.started_ms)}</span>
              <button
                class="card-btn"
                onclick={() => copyCard('summary')}
                title={t("Draw this run as a picture and put it on the clipboard")}
              >
                {cardLabel('summary', t('Copy card'))}
              </button>
              {#if worthFull}
                <button
                  class="card-btn wide"
                  onclick={() => copyCard('full')}
                  title={t("Every find in this run, best grade first")}
                >
                  {cardLabel('full', say('Full · {n}', { n: run.notable.length }))}
                </button>
              {/if}
              <span class="right">{dur(run.secs)}</span>
            </div>
            <div class="sub">
              {run.character ?? t('unknown character')}
              {#if run.level}· {t('Lv')} {run.level}{/if}
              {#if run.herolevel}· {t('HLv')} {run.herolevel}{/if}
              · {difficultyName(selected.difficulty)}
            </div>
            <div class="rates">
              {#each RATES as [label, cls, of], i}
                {@const total = of(run)}
                {@const rate = perHour(total, run.secs)}
                {@const baseline = baselines[i] ?? null}
                {@const apart = against(rate, baseline)}
                <div class="rate">
                  <div class="label">{t(label)}</div>
                  <div class="value {cls}">{Number.isFinite(total) ? fmt(total) : '—'}</div>
                  <div class="sub">{rateText(rate)}</div>
                  {#if apart != null}
                    <div class="vs" class:up={apart > 0} class:down={apart < 0}>{apart > 0 ? '+' : ''}{apart}% {t('vs usual')}</div>
                  {/if}
                  {#if baseline !== null}
                    <div class="sub">{say('Median: {rate}/h', { rate: fmt(baseline) })}</div>
                  {/if}
                </div>
              {/each}
            </div>
            <div class="comparison">
              <span>{t('Dominant zone')} · {zoneName(selected.zone)}</span>
              {#if !comparison.eligible}
                <span>{t('Comparison requires a known zone and difficulty')}</span>
              {:else}
                <span>{say('{n} comparable runs', { n: comparison.peers.length })}</span>
                {#if !comparison.longEnough}
                  <span>{say('Comparison starts at {n} minutes', { n: COMPARISON_MINUTES })}</span>
                {:else if comparison.peers.length < MIN_COMPARISONS}
                  <span>{say('At least {n} comparable runs are needed', { n: MIN_COMPARISONS })}</span>
                {/if}
              {/if}
            </div>
          </div>

          <div class="box" style:border-image-source="url({art('chip_dark')})">
            <div class="head"><span class="accent">{t("Loot")}</span></div>
            <div class="tally">
              {#each RARITIES as name}
                <div class="tallyrow">
                  <span class={RARITY_CLASS[name]}>{t(name)}</span>
                  <b>{fmt(run.items?.[name] ?? 0)}</b>
                </div>
              {/each}
            </div>
          </div>

          {#if bosses(run).length || chests(run).length || cleared(run).length}
            <div class="box" style:border-image-source="url({art('chip_dark')})">
              <div class="head"><span class="accent">{t("Killed & opened")}</span></div>
              {#if bosses(run).length}
                <div class="subhead">{t("Bosses")}</div>
                <div class="tally">
                  {#each bosses(run) as row}
                    <div class="tallyrow"><span class="dim">{t(nameOf(row.label))}</span><b class="c-sat">{fmt(row.total)}</b></div>
                  {/each}
                </div>
              {/if}
              {#if chests(run).length}
                <div class="subhead">{t("Chests")}</div>
                <div class="tally">
                  {#each chests(run) as row}
                    <div class="tallyrow"><span class="dim">{t(nameOf(row.label))}</span><b class="c-gold">{fmt(row.total)}</b></div>
                  {/each}
                </div>
              {/if}
              {#if cleared(run).length}
                <div class="subhead">{t("Cleared")}</div>
                <div class="tally">
                  {#each cleared(run) as row}
                    <div class="tallyrow"><span class="dim">{t(nameOf(row.label))}</span><b class="c-gold">{fmt(row.total)}</b></div>
                  {/each}
                </div>
              {/if}
            </div>
          {/if}

          {#if run.notable?.length}
            <div class="box grow" style:border-image-source="url({art('chip_dark')})">
              <div class="head">
                <span class="accent">{t("Finds")}</span>
                <span class="right">{run.notable.length}</span>
              </div>
              <div class="scroll">
                {#each run.notable as item}
                  <div class="find">
                    <span class="name {RARITY_CLASS[item.rarity] ?? ''}">{nameOf(item.name)}</span>
                    <span class="dim tier">{tierLabel(item.tier)}</span>
                  </div>
                {/each}
              </div>
            </div>
          {/if}
        </div>
      {/if}
      {#if !visible.length}
        <div class="detail"><div class="empty">
          <div>{t('No runs match these filters')}</div>
          <button class="card-btn" onclick={clearFilters}>{t('Clear filters')}</button>
        </div></div>
      {/if}
    </div>
  {:else}
    <div class="empty"> {t("No runs yet. One is filed when a session ends — the Reset button, the tray, Ctrl+Shift+R, or the game closing. A run under a minute, or one where nothing was earned, is not worth keeping and is dropped.")} </div>
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
    box-sizing: border-box;
    width: 100%;
    height: 100%;
    display: flex;
    flex-direction: column;
    font-family: var(--face);
    font-size: 12px;
    color: var(--bone-6);
  }

  .cols {
    flex: 1 1 auto;
    min-height: 0;
    display: flex;
    gap: 6px;
  }

  /* The list took a fixed 260 of the 434px pane at the smallest window the app
     allows, and the detail column — where the figures are — got what was left.
     It gives ground now, down to a width that still reads. */
  .list {
    flex: 0 1 260px;
    min-width: 150px;
    box-sizing: border-box;
    border: 6px solid transparent;
    border-image-slice: 6 fill;
    border-image-width: 6px;
    image-rendering: pixelated;
    padding: 4px 6px 6px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .detail {
    flex: 1 1 auto;
    min-width: 220px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    overflow-y: auto;
  }
  .detail::-webkit-scrollbar { width: 6px; }
  .detail::-webkit-scrollbar-thumb { background: var(--dim-1); border-radius: 3px; }

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
    gap: 3px;
  }
  .box.grow { flex: 1 1 auto; min-height: 120px; }

  .head {
    display: flex;
    align-items: baseline;
    gap: 6px;
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.4px;
    color: var(--edge-2b);
  }
  .accent { color: var(--edge-2b); }
  .right { margin-left: auto; color: var(--dim-2); }

  .filters { display: grid; gap: 3px; }
  .filters select {
    min-width: 0;
    width: 100%;
    padding: 3px 4px;
    color: var(--bone-9);
    border: 1px solid var(--ground-10);
    background: var(--ground-6);
    font: inherit;
    font-size: 11px;
  }

  .scroll {
    flex: 1 1 auto;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .scroll::-webkit-scrollbar { width: 6px; }
  .scroll::-webkit-scrollbar-thumb { background: var(--dim-1); border-radius: 3px; }

  .row {
    display: grid;
    grid-template-columns: 1fr auto;
    grid-template-areas: 'when len' 'place place' 'sum sum';
    gap: 0 6px;
    font: inherit;
    font-size: 11px;
    color: var(--bone-6);
    text-align: left;
    background: rgba(0, 0, 0, 0.2);
    border: none;
    border-left: 2px solid transparent;
    padding: 4px 6px;
    cursor: pointer;
  }
  .row:hover { background: rgba(0, 0, 0, 0.35); }
  .row.on { border-left-color: var(--edge-2b); background: rgba(var(--pick-rgb), 0.25); }
  .when { grid-area: when; }
  .len { grid-area: len; color: var(--dim-2); }
  .place { grid-area: place; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .sum { grid-area: sum; font-size: 10px; color: var(--edge-8); }

  /* Four across only while four fit. `flex: 1` with `min-width: 0` let each
     box shrink to 28px at the 620px minimum window while the figure inside
     still needed 60, and nothing clipped it: Gold, XP, Kills and Drops were
     printed on top of one another. */
  .rates {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(84px, 1fr));
    gap: 6px;
    padding-top: 2px;
  }
  .rate { min-width: 0; overflow: hidden; }
  .vs { margin-top: 1px; font-size: 10px; color: var(--bone-4); }
  .vs.up { color: var(--gold-2); }
  .vs.down { color: #b06a6a; }

  .rate .sub { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .label {
    font-size: 9px;
    text-transform: uppercase;
    letter-spacing: 0.4px;
    color: var(--bone-4);
  }
  .value { font-size: 16px; line-height: 20px; }
  .sub { font-size: 10px; color: var(--edge-8); }
  .comparison {
    margin-top: 3px;
    padding-top: 4px;
    border-top: 1px solid var(--ground-10);
    color: var(--bone-6);
    font-size: 10px;
    display: flex;
    gap: 3px 10px;
    flex-wrap: wrap;
  }

  /* the same small caption Stats uses over its own tallies */
  .subhead {
    font-size: 9px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--bone-4);
    margin: 6px 0 2px;
  }
  .tally { display: grid; grid-template-columns: 1fr 1fr; gap: 1px 14px; }
  .tallyrow { display: flex; justify-content: space-between; gap: 8px; }
  .tallyrow b { font-weight: normal; color: var(--bone-9); }
  /* `.tallyrow b` is (0,1,1) and a bare class is (0,1,0), so the colours on
     the bosses and chests rows lost to it and every figure came out bone. */
  .tallyrow b.c-sat { color: var(--rar-satanic); }
  .tallyrow b.c-gold { color: var(--gold-2); }

  .find { display: flex; align-items: baseline; gap: 8px; padding: 1px 0; }
  .find .name { flex: 1 1 auto; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .tier { flex: none; width: 24px; text-align: right; }
  .dim { color: var(--edge-8); font-size: 11px; }

  .empty {
    margin: auto;
    max-width: 420px;
    text-align: center;
    font-size: 11px;
    line-height: 17px;
    color: var(--edge-8);
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
    font-size: 10px;
    color: var(--bone-13);
    text-shadow: 0 1px 0 var(--ground-1);
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
  .btn.armed { color: #f0c0c0; }

  .card-btn.wide { margin-left: 4px; }
  .card-btn {
    font: inherit;
    font-size: 11px;
    color: var(--edge-9);
    background: none;
    border: 1px solid var(--edge-1);
    border-radius: 2px;
    padding: 1px 6px;
    margin-left: 8px;
    cursor: pointer;
  }
  .card-btn:hover { color: var(--gold-2); border-color: var(--edge-3); }

  .c-sat { color: var(--rar-satanic); }
  .c-set { color: #40d040; }
  .c-her { color: #00ffae; }
  .c-ang { color: #f6f794; }
  .c-unh { color: #e04a7a; }
  .c-gold { color: var(--gold-2); }
  .c-xp { color: #a06ae0; }
</style>

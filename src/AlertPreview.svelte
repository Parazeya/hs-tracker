<script>
  import { onMount } from 'svelte';
  import { play, resolveSound, soundChannel } from './audio.js';
  import { invoke, listen } from './bridge.js';
  import { BY_ID, ITEMS } from './items.js';
  import { itemName, t, typeLabel } from './say.svelte.js';
  import { flush, store } from './settings.svelte.js';

  const catalogue = Object.entries(ITEMS).map(([key, name]) => ({
    key,
    name,
    identity: key.split(':').map(Number),
    rarity: BY_ID[key]?.[1],
  }));
  const fold = (value) => value.normalize('NFKD').replace(/\p{M}/gu, '').replace(/[’‘`´]/g, "'").toLowerCase();
  const label = (item) => itemName(...item.identity) || item.name;

  let query = $state('');
  let input;
  let selected = $state(null);
  let retry = $state(0);
  let pending = $state(false);
  let result = $state(null);
  let error = $state(false);
  let matches = $derived.by(() => {
    const words = fold(query).trim().split(/\s+/).filter(Boolean);
    if (!words.length || selected) return [];
    return catalogue.filter((item) => {
      const names = fold(`${label(item)} ${item.name}`);
      return words.every((word) => names.includes(word));
    }).slice(0, 10);
  });

  function choose(item) {
    selected = item;
    query = label(item);
    input?.focus();
  }

  function ruleLabel(rule) {
    if (rule === 'watchlist') return t('Watchlist');
    if (rule === 'relic') return t('Hunted relic');
    if (rule === 'muted') return t('Item exception');
    if (rule === 'minimum_tier') return t('Below minimum tier');
    if (rule === 'rarity_disabled') return t('Rarity alert disabled');
    if (rule === 'resource') return t('Resource outside watchlist');
    return t('Rarity');
  }

  async function check(item) {
    await flush();
    const [itemType, itemId, weaponType] = item.identity;
    const response = await invoke('get_alert_preview', { itemType, itemId, weaponType });
    if (!response) throw new Error('Missing catalogue item');
    const [decision, rarity] = response;
    const settings = store.settings;
    const channel = decision.sound && soundChannel(settings, decision.sound);
    const volume = channel?.volume ?? 0.7;
    let sound = null;
    if (decision.sound && channel?.enabled !== false && volume > 0) {
      sound = await resolveSound(decision.sound, settings?.chimes?.[decision.sound]);
      const fallback = rarity.toLowerCase();
      if (!sound && fallback !== decision.sound) {
        sound = await resolveSound(fallback, settings?.chimes?.[fallback]);
      }
    }
    return { decision, channel, volume, sound };
  }

  $effect(() => {
    store.changed;
    retry;
    const item = selected;
    result = null;
    error = false;
    pending = !!item;
    if (!item) return;
    let active = true;
    check(item).then(
      (data) => { if (active) { result = data; pending = false; } },
      () => { if (active) { error = true; pending = false; } },
    );
    return () => { active = false; };
  });

  onMount(() => {
    const unsubscribe = listen('sounds-changed', () => retry++);
    return () => { unsubscribe.then((stop) => stop()); };
  });
</script>

<details class="alert-preview">
  <summary>{t('Preview item alert')}</summary>
  <label for="alert-item-search">{t('Item')}</label>
  <input
    id="alert-item-search"
    bind:this={input}
    value={query}
    placeholder={t('Search items by name')}
    oninput={(event) => { selected = null; query = event.currentTarget.value; }}
  />
  {#if matches.length}
    <ul class="matches">
      {#each matches as item (item.key)}
        <li><button type="button" onclick={() => choose(item)}>
          <strong>{label(item)}</strong>
          <small>{[item.rarity && t(item.rarity), typeLabel(item.identity[0], item.identity[2])].filter(Boolean).join(' · ')}</small>
        </button></li>
      {/each}
    </ul>
  {:else if query.trim() && !selected}
    <p>{t('No matching items')}</p>
  {/if}
  <div aria-live="polite">
    {#if pending}
      <p>{t('Checking alert…')}</p>
    {:else if error}
      <p>{t('Could not check the alert')} <button type="button" onclick={() => retry++}>{t('Try again')}</button></p>
    {:else if result}
      <dl>
        <dt>{t('Applied rule')}</dt>
        <dd>{ruleLabel(result.decision.rule)}{result.channel?.name ? ` · ${result.channel.name}` : ''}</dd>
        <dt>{t('Sound')}</dt>
        <dd>
          {#if result.sound}
            {result.sound.file ?? `${t('built-in')} · ${t(result.sound.builtIn[0].toUpperCase() + result.sound.builtIn.slice(1))}`}
            <button type="button" onclick={() => play(result.sound.url, result.volume)}>{t('Test')}</button>
          {:else}
            {t('Off')}
          {/if}
        </dd>
        <dt>{t('Announcement')}</dt>
        <dd>{result.decision.flourish ? t('On') : t('Off')}</dd>
        {#if result.sound || result.decision.flourish}
          <dt>{t('Timing')}</dt>
          <dd>{result.decision.on_ground ? t('On drop') : t('On pickup')}</dd>
        {/if}
      </dl>
    {/if}
  </div>
</details>

<style>
  .alert-preview { padding: .55rem .65rem; border: 1px solid var(--border, #544039); background: var(--well, #211716); }
  summary { cursor: pointer; font-weight: bold; color: var(--gold, #ddc886); }
  label { display: block; margin: .55rem 0 .25rem; }
  input, button { font: inherit; color: inherit; background: #1b1110; border: 1px solid #634c45; }
  input { width: 100%; max-width: 24rem; padding: .35rem .45rem; }
  button { cursor: pointer; padding: .2rem .45rem; }
  button:hover, button:focus-visible { border-color: var(--gold, #ddc886); }
  .matches { list-style: none; padding: 0; margin: .25rem 0 0; max-width: 24rem; max-height: 12rem; overflow: auto; }
  .matches button { display: flex; flex-wrap: wrap; align-items: baseline; justify-content: space-between; gap: .25rem .6rem; width: 100%; text-align: left; }
  small, dt { color: var(--dim, #bba8a0); }
  dl { display: grid; grid-template-columns: auto minmax(0, 1fr); align-items: baseline; gap: .3rem .6rem; margin: .55rem 0 0; }
  dd { margin: 0; overflow-wrap: anywhere; }
  dd button { margin-inline-start: .4rem; }
  p { margin: .4rem 0 0; }
</style>

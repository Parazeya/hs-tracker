<script>
  import { onMount } from 'svelte';
  import { rowsOf } from './panel.js';
  import { ITEMS, TYPE_NAMES, WEAPON_NAMES, TIER_LETTERS, zoneLabel } from './items.js';
  import { streamClient } from './stream-client.js';
  import { currentEntries, reading, TINTS } from './stream-model.js';
  import { accentOf, lookForDrop, nameStyle } from './look.js';
  import { mapEnvelope, mirrorBox, sourceUrl, validMapState } from './stream-protocol.js';

  const view = location.pathname.split('/').filter(Boolean)[0] ?? 'stats';
  const dictionaries = import.meta.glob('./lang/*.json');
  let vocabulary = $state(null);
  let appearance = $state({});
  let snapshot = $state(null);
  let drops = $state([]);
  let notifications = $state([]);
  let connection = $state('connecting');
  let now = $state(Date.now());
  let snapshotAt = 0;
  let serverOffset = 0;
  let map = $state(null);
  let mapRevision = 0;
  let frame = $state(null);
  let mirrorReady = false;
  let viewport = $state({ width: window.innerWidth, height: window.innerHeight });
  let loadingLanguage = '';
  const text = label => vocabulary?.ui?.[label] ?? label;
  const label = entry => {
    const key = `${entry.item_type}:${entry.item_id}:${entry.weapon_type ?? 0}`;
    const known = vocabulary?.items?.[key] ?? ITEMS[key];
    return known ?? entry.name ?? (entry.item_type === 3 ? WEAPON_NAMES[entry.weapon_type] : TYPE_NAMES[entry.item_type]) ?? 'Item';
  };
  const seconds = $derived((snapshot?.session_secs ?? 0)
    + (snapshot && !snapshot.paused && connection === 'connected' ? Math.max(0, Math.floor((now - snapshotAt) / 1000)) : 0));
  const liveDrops = $derived(currentEntries(drops, now + serverOffset));
  const liveNotifications = $derived(currentEntries(notifications, now + serverOffset, 90000));
  const activeNotification = $derived(liveNotifications.find(entry => entry.start_ms <= now + serverOffset) ?? null);
  const notificationLook = $derived(lookForDrop(appearance.looks, activeNotification?.name, activeNotification?.rarity));
  const notificationTint = $derived(TINTS[activeNotification?.rarity] ?? '#d1ba93');
  const notificationAccent = $derived(accentOf(notificationLook, notificationTint));
  const box = $derived(map ? mirrorBox(map.viewport, viewport) : null);
  const mapUrl = sourceUrl(location.origin, 'mirror', location.origin);
  function applyMap() {
    if (!mirrorReady || !frame?.contentWindow || !map) return;
    frame.contentWindow.postMessage({ channel: 'hs-map', version: 1, type: 'apply', revision: mapRevision, state: map }, location.origin);
  }
  async function language(code) {
    if (loadingLanguage === code) return;
    loadingLanguage = code;
    const load = dictionaries[`./lang/${code}.json`];
    const result = load ? await load().catch(() => null) : null;
    if (loadingLanguage === code) vocabulary = result?.default ?? null;
  }
  onMount(() => {
    const stop = streamClient({
      onStatus: status => { connection = status; },
      onEpoch: () => {
        snapshot = null; drops = []; notifications = []; map = null;
        mirrorReady = false;
      },
      onFrame: (update, transport) => {
        serverOffset = transport.serverMs - Date.now();
        switch (update.topic) {
          case 'snapshot': snapshot = update.payload; snapshotAt = Date.now(); break;
          case 'appearance': appearance = update.payload ?? {}; language(appearance.language ?? 'en'); break;
          case 'drops': drops = update.payload; break;
          case 'notifications': notifications = update.payload; break;
          case 'map': if (validMapState(update.payload)) { map = update.payload; mapRevision += 1; queueMicrotask(applyMap); } break;
        }
      },
    });
    const timer = setInterval(() => { now = Date.now(); }, 200);
    const resize = () => { viewport = { width: window.innerWidth, height: window.innerHeight }; };
    const message = event => {
      if (event.source === frame?.contentWindow && event.origin === location.origin && mapEnvelope(event.data, 'ready')) {
        mirrorReady = true;
        applyMap();
      }
    };
    window.addEventListener('resize', resize);
    window.addEventListener('message', message);
    return () => { stop(); clearInterval(timer); window.removeEventListener('resize', resize); window.removeEventListener('message', message); };
  });
</script>

<main class:disconnected={connection !== 'connected'} style:--accent={/^#[0-9a-f]{6}$/i.test(appearance.accent ?? '') ? appearance.accent : '#cda868'}>
  {#if view === 'stats'}
    <section class="stats" aria-label="Session statistics">
      <header><b>HS Tracker</b><span>{snapshot?.paused ? text('Paused') : connection === 'connected' ? text('Session') : text('Reconnecting…')}</span></header>
      {#if snapshot}
        <div class="readings">
          {#each rowsOf(appearance.slots).flat() as id, i (i)}
            {@const cell = reading(id, snapshot, seconds)}
            <div class="cell"><small>{text(cell[0])}</small><strong>{cell[1]}</strong></div>
          {/each}
        </div>
        <footer><span>{vocabulary?.rooms?.[snapshot.room] ?? zoneLabel(snapshot.room)}</span><span>MF {snapshot.mf ?? 0}%</span></footer>
      {:else}<p>{text(connection === 'connected' ? 'Waiting for session…' : 'Connecting to Tracker…')}</p>{/if}
    </section>
  {:else if view === 'drops'}
    <section class="drop-list" aria-label="Recent drops">
      {#each liveDrops as entry (`${entry.stream_id}:${entry.until_ms}`)}
        <article style:--rarity={TINTS[entry.rarity] ?? '#d1ba93'}><b>{label(entry)}</b><span>{TIER_LETTERS[entry.tier - 1] ?? ''}{entry.mf ? ' · MF' : ''}</span></article>
      {/each}
    </section>
  {:else if view === 'notifications'}
    {#if activeNotification}
      {#key `${activeNotification.stream_id}:${activeNotification.until_ms}`}
        <section class="announcement" style:--rarity={notificationAccent}>
          <div class="beam"></div>
          <div class="notice">
            <small>{text(activeNotification.kind === 'test' ? 'Test output' : activeNotification.kind === 'zone' ? 'Satanic Zone' : activeNotification.collection_new ? 'New collection find' : activeNotification.rarity ?? 'Drop')}</small>
            <strong style={nameStyle(notificationLook, notificationTint)}>{activeNotification.kind === 'zone' ? zoneLabel(activeNotification.zone) : label(activeNotification)}</strong>
            <span>{activeNotification.kind === 'zone' ? `${activeNotification.buffs?.length ?? 0} ${text('buffs')}` : TIER_LETTERS[activeNotification.tier - 1] ?? ''}</span>
          </div>
        </section>
      {/key}
    {/if}
  {:else if view === 'map'}
    {#if map && box}
      <div class="map-box" style:width="{box.width}px" style:height="{box.height}px" style:left="{box.left}px" style:top="{box.top}px" style:transform="scale({box.scale})">
        <iframe bind:this={frame} src={mapUrl} title="Tracker map view" sandbox="allow-scripts allow-same-origin" referrerpolicy="no-referrer"></iframe>
      </div>
    {:else}<div class="empty-map">{text('Open the map in Tracker to choose the view shown here.')}</div>{/if}
  {/if}
</main>

<style>
  @font-face { font-family:HSStream;src:url('./assets/fonts/cookierunbold.ttf'); }
  :global(html), :global(body), :global(#app) { margin:0;width:100%;height:100%;overflow:hidden;background:transparent!important; }
  :global(*), :global(*::before), :global(*::after) { box-sizing:border-box; }
  main { position:fixed;inset:0;color:#f2e9da;font:16px/1.35 HSStream,system-ui,sans-serif;--accent:#cda868;--rarity:#cda868; }
  main.disconnected .stats { opacity:.65; }
  .stats { width:100%;padding:12px;background:linear-gradient(135deg,#221a1fee,#100d13ee);border:1px solid #66513b;border-radius:8px;box-shadow:inset 0 0 0 2px #160e16; }
  header { display:flex;justify-content:space-between;align-items:center;border-bottom:1px solid #69543a;padding:0 4px 9px;color:var(--accent); }
  header span { font-size:12px;color:#bbb0a7; }
  .readings { display:grid;grid-template-columns:repeat(3,minmax(0,1fr));gap:7px;padding:10px 0; }
  .cell { min-width:0;padding:6px 8px;background:#0c0910aa;border-radius:3px;display:flex;flex-direction:column;gap:3px; }
  .cell small { color:#b8a68b;font-size:11px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis; }
  .cell strong { font-size:22px;font-weight:normal;font-variant-numeric:tabular-nums; }
  footer { display:flex;justify-content:space-between;gap:8px;border-top:1px solid #4a3a30;padding:7px 3px 0;color:#c9b68d;font-size:12px; }
  footer span { overflow:hidden;text-overflow:ellipsis;white-space:nowrap; }
  p { margin:12px 0;font:14px system-ui; }
  .drop-list { display:flex;flex-direction:column;gap:7px;padding:4px; }
  article { display:flex;justify-content:space-between;gap:12px;padding:10px 13px;background:#140f18e8;border-left:4px solid var(--rarity);border-radius:3px;box-shadow:0 2px 6px #0006; }
  article b { color:var(--rarity);font-weight:normal; } article span { flex-shrink:0;color:#e7d9bd; }
  .announcement { position:absolute;inset:0;display:grid;place-items:center; }
  .beam { position:absolute;left:calc(50% - 2px);width:4px;height:80%;background:var(--rarity);opacity:.3;transform:scaleY(.8);animation:beam-in .5s ease-out both; }
  .notice { z-index:1;width:90%;padding:24px 32px;text-align:center;background:linear-gradient(90deg,transparent,#140e1aee 15%,#140e1aee 85%,transparent);border-block:1px solid var(--rarity);display:flex;flex-direction:column;gap:8px;animation:appear .35s ease-out both; }
  .notice small { color:var(--rarity);font-size:18px; }.notice strong { color:var(--rarity);font-size:clamp(20px,4vw,38px);font-weight:normal; }.notice span { font-size:20px; }
  .map-box { position:absolute;transform-origin:top left; }.map-box iframe { width:100%;height:100%;border:0;pointer-events:none; }
  .empty-map { position:absolute;inset:0;display:grid;place-items:center;background:#151019dd;font:18px system-ui;text-align:center;padding:25px; }
  @keyframes appear {from{opacity:0;transform:translateY(12px)}to{opacity:1;transform:none}}@keyframes beam-in{from{opacity:0;transform:scaleY(.1)}to{opacity:.3;transform:scaleY(.8)}}
</style>

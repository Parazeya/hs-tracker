<script>
  import { onMount } from 'svelte';
  import { invoke, listen, native } from './bridge.js';
  import { t } from './say.svelte.js';
  import { MAP_CHANNEL, MAP_VERSION, mapEnvelope, validMapState } from './map-frame.js';

  let mapUrl = $state('');
  let mapOrigin = '';
  let frame = $state(null);
  let sourceRevision = -1;
  let restoreState;
  let latestState;
  let generation = $state(0);
  let loading = $state(true);
  let slow = $state(false);
  let error = $state('');
  let timer;
  function wait() {
    loading = true;
    slow = false;
    clearTimeout(timer);
    timer = setTimeout(() => { slow = true; }, 15000);
  }
  function loaded() {
    loading = false;
    clearTimeout(timer);
  }
  async function reload() {
    error = '';
    const request = ++generation;
    mapUrl = '';
    sourceRevision = -1;
    wait();
    try {
      const base = native ? await invoke('get_map_source') : location.origin;
      if (request !== generation) return;
      const url = new URL('/hs-map/index.html', base);
      url.searchParams.set('mode', 'source');
      url.searchParams.set('parentOrigin', location.origin);
      mapOrigin = url.origin;
      restoreState = latestState;
      mapUrl = url.href;
    } catch (reason) {
      if (request === generation) {
        loaded();
        error = String(reason);
      }
    }
  }
  async function close() {
    try { await invoke('hide_map'); } catch (reason) { error = String(reason); }
  }
  onMount(() => {
    reload();
    const changed = (event) => {
      if (event.source !== frame?.contentWindow || event.origin !== mapOrigin) return;
      if (mapEnvelope(event.data, 'ready')) {
        loaded();
        // A new map document starts its revision counter at the beginning.
        sourceRevision = event.data.revision;
        if (restoreState) {
          frame.contentWindow.postMessage({
            channel: MAP_CHANNEL, version: MAP_VERSION, type: 'apply', revision: sourceRevision, state: restoreState,
          }, mapOrigin);
          restoreState = null;
        }
        return;
      }
      if (!mapEnvelope(event.data, 'state') || !validMapState(event.data.state)
        || event.data.revision <= sourceRevision) return;
      sourceRevision = event.data.revision;
      latestState = event.data.state;
    };
    window.addEventListener('message', changed);
    const unlisten = listen('map-window-error', (event) => { error = String(event.payload); });
    return () => { clearTimeout(timer); window.removeEventListener('message', changed); unlisten.then((stop) => stop()); };
  });
</script>

<div class="map-view">
  <header>
    <strong>{t('World map')}</strong>
    <span class="shortcut">Ctrl+Shift+M</span>
    <span class="status" role="status">
      {error || (loading ? (slow ? t('The map is taking longer to load.') : t('Loading map…')) : '')}
    </span>
    <button class="reload" onclick={reload}>{t('Reload map')}</button>
    <button class="close" onclick={close} aria-label={t('Close map')} title={`${t('Close map')} · Ctrl+Shift+M`}>✕</button>
  </header>
  {#key generation}
    {#if mapUrl}
      <!-- Packaged current map, isolated on loopback. Only validated camera and
           selection data cross this boundary; the iframe has no native grants. -->
      <iframe bind:this={frame} src={mapUrl} title={t('World map')} sandbox="allow-scripts allow-same-origin" referrerpolicy="no-referrer"></iframe>
    {/if}
  {/key}
</div>

<style>
  .map-view { position: fixed; inset: 0; display: flex; flex-direction: column; background: #101017; color: #eeeaf4; font: 14px/1.4 system-ui, sans-serif; }
  header { flex: 0 0 52px; display: flex; align-items: center; gap: 18px; padding-left: 20px; background: #1d1926; border-bottom: 1px solid #534a64; box-sizing: border-box; }
  strong { white-space: nowrap; font-size: 16px; }
  .shortcut { color: #c8bbdf; white-space: nowrap; }
  .status { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 12px; color: #dbc8a8; }
  button { cursor: pointer; border: 1px solid #6e6081; color: #f5efff; background: #30283e; font: inherit; border-radius: 5px; padding: 7px 14px; }
  button:hover { background: #50415f; }
  button:focus-visible { outline: 3px solid #dfc7ff; outline-offset: -3px; }
  .close { flex: 0 0 60px; align-self: stretch; border: 0; border-left: 1px solid #a65c68; border-radius: 0; background: #8b2d41; color: white; font-size: 27px; line-height: 1; padding: 0; }
  .close:hover { background: #b43b53; }
  iframe { width: 100%; flex: 1; min-height: 0; border: 0; background: #101017; }
  @media (max-width: 640px) { header { gap: 10px; padding-left: 12px; } .shortcut { display: none; } .reload { font-size: 12px; padding: 6px; } }
</style>

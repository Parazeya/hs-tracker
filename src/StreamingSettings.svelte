<script>
  import { onMount } from 'svelte';
  import { invoke, listen } from './bridge.js';
  import { store, save, flush } from './settings.svelte.js';
  import { t } from './say.svelte.js';

  const SOURCES = [
    ['stats', 'Statistics', 480, 380], ['notifications', 'Notifications', 960, 540],
    ['drops', 'Recent drops', 480, 300], ['map', 'Map view', 1280, 720],
  ];
  let settings = $derived(store.settings);
  let status = $state(null);
  let message = $state('');
  let port = $state('');
  let busy = $state(false);
  async function refresh() {
    try { status = await invoke('streaming_status'); }
    catch (error) { message = String(error); }
  }
  async function toggle() {
    if (busy) return;
    busy = true;
    settings.stream_enabled = !settings.stream_enabled;
    try { save(); await flush(); await refresh(); }
    catch (error) { message = String(error); }
    finally { busy = false; }
  }
  async function changePort() {
    if (!/^\d+$/.test(port) || Number(port) < 1 || Number(port) > 65535) {
      message = t('Choose a port between 1 and 65535.'); return;
    }
    settings.stream_port = Number(port);
    try { save(); await flush(); await refresh(); message = t('Update the source URLs in OBS after changing the port.'); }
    catch (error) { message = String(error); }
  }
  async function copy(source) {
    try { await invoke('copy_text', { text: `${status.base_url}/${source}` }); message = t('Copied'); }
    catch (error) { message = String(error); }
  }
  async function testOutput() {
    try { await invoke('test_stream'); message = t('Preview notification sent to OBS.'); }
    catch (error) { message = String(error); }
  }
  onMount(() => {
    refresh();
    const timer = setInterval(refresh, 2000);
    const changed = listen('stream-status-changed', event => { status = event.payload; });
    return () => { clearInterval(timer); changed.then(stop => stop()); };
  });
</script>

<section class="stream-settings">
  <h2>{t('OBS Browser Sources')}</h2>
  <p>{t('Add a Browser source in OBS for each part of Tracker you want to show. Paste its URL and set the size below. Use 30 FPS.')}</p>
  <button class="toggle" class:on={settings?.stream_enabled} onclick={toggle} disabled={busy || !settings} aria-pressed={!!settings?.stream_enabled}>
    {t(settings?.stream_enabled ? 'Browser Sources enabled' : 'Enable Browser Sources')}
  </button>
  <p class="connection" role="status">
    {#if status?.error}{status.error}
    {:else if status?.enabled}{t('Connected sources')}: {status.clients}
    {:else}{t('Enable the sources, then add their URLs in OBS.')}{/if}
  </p>
  {#if status}
    <div class="sources">
      {#each SOURCES as [source, label, width, height]}
        <div class="source"><b>{t(label)}</b><span>{width} × {height}</span>
          <code>{status.base_url}/{source}</code><button onclick={() => copy(source)}>{t('Copy URL')}</button>
        </div>
      {/each}
    </div>
  {/if}
  <div class="actions"><button disabled={!status?.enabled} onclick={testOutput}>{t('Test notification')}</button><button onclick={refresh}>{t('Refresh status')}</button></div>
  <p>{t('The map source follows the view, selection, search and language in Tracker. Different source proportions add margins to preserve that view.')}</p>
  <p>{t('Statistics and notifications keep updating while Tracker is in the tray. Sources are silent; Tracker remains the single audio source.')}</p>
  <details><summary>{t('Connection settings')}</summary>
    <label>{t('Local port')} <input type="number" min="1" max="65535" placeholder={String(status?.port ?? 17265)} bind:value={port} /></label>
    <button onclick={changePort}>{t('Apply port')}</button>
    <p>{t('Update the source URLs in OBS after changing the port.')}</p>
  </details>
  {#if message}<p role="status">{message}</p>{/if}
</section>

<style>
  .stream-settings{padding:14px;color:var(--ink-1,#e8dcc3);font-size:13px;line-height:1.5}h2{margin:0 0 12px;font-size:16px;color:var(--gold-2,#d3b67d)}p{margin:10px 0;color:var(--ink-2,#c5bba9)}button{font:inherit;cursor:pointer;color:var(--ink-1,#eadfc9);background:#281d26;border:1px solid #6f543e;padding:7px 12px;border-radius:3px}button:hover{background:#49313c}button:disabled{opacity:.4;cursor:default}.toggle.on{background:#284732;border-color:#529468}.sources{display:flex;flex-direction:column;gap:10px;margin:15px 0}.source{display:grid;grid-template-columns:1fr auto;gap:5px 10px;padding:10px;background:#100d1388;border:1px solid #5a4435}.source>b{color:var(--gold-2,#d3b67d)}.source>span{text-align:right;color:#a69a85}.source>code{overflow-wrap:anywhere;align-self:center;font:12px/1.5 ui-monospace,monospace}.actions{display:flex;gap:8px}.connection{min-height:20px}details{margin-top:16px;border-top:1px solid #5a4435;padding-top:10px}summary{cursor:pointer}input{width:100px;color:inherit;background:#120d15;border:1px solid #5a4435;padding:6px;margin:8px}label{display:inline-flex;align-items:center}button:focus-visible,input:focus-visible{outline:2px solid #e1c17d;outline-offset:2px}
</style>

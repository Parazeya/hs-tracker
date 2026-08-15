<script>
  import { invoke } from './bridge.js';
  import { art } from './skin.svelte.js';
  import appIcon from '../src-tauri/icons/128x128.png';

  let info = $state(null);
  /// null before anyone asks — the check is a button, never something the app
  /// does on its own. This is the only request the app ever makes, and it is
  /// worth keeping that true.
  let latest = $state(null);
  let checking = $state(false);
  let failed = $state('');

  $effect(() => {
    invoke('about').then((a) => (info = a)).catch(() => {});
  });

  /// "0.9.8" against "0.9.10": compared piece by piece, because a string
  /// comparison would call the second one older.
  function newer(there, here) {
    const a = String(there).replace(/^v/, '').split('.').map(Number);
    const b = String(here).replace(/^v/, '').split('.').map(Number);
    for (let i = 0; i < Math.max(a.length, b.length); i++) {
      const x = a[i] ?? 0;
      const y = b[i] ?? 0;
      if (x !== y) return x > y;
    }
    return false;
  }

  async function check() {
    checking = true;
    failed = '';
    latest = null;
    try {
      const owner = info.repo.replace('https://github.com/', '');
      const r = await fetch(`https://api.github.com/repos/${owner}/releases/latest`, {
        headers: { Accept: 'application/vnd.github+json' },
      });
      if (!r.ok) throw new Error(`GitHub answered ${r.status}`);
      const release = await r.json();
      const tag = release.tag_name ?? '';
      latest = {
        tag: tag.replace(/^v/, ''),
        url: release.html_url ?? `${info.repo}/releases`,
        newer: newer(tag, info.version),
        when: release.published_at ? new Date(release.published_at).toLocaleDateString('en-GB') : '',
      };
    } catch (e) {
      failed = String(e.message ?? e);
    }
    checking = false;
  }

  const open = (url) => invoke('open_url', { url }).catch((e) => (failed = String(e)));

  // the two addresses a streamer pastes into OBS, or nothing while it is off
  let urls = $state(null);
  let copied = $state('');
  $effect(() => {
    invoke('stream_urls').then((u) => (urls = u)).catch(() => {});
    const t = setInterval(() => invoke('stream_urls').then((u) => (urls = u)).catch(() => {}), 2000);
    return () => clearInterval(t);
  });
  function copy(text) {
    invoke('copy_text', { text }).catch(() => {});
    copied = text;
    setTimeout(() => (copied = ''), 1500);
  }
</script>

<div class="panel">
  <div class="body">
    <div class="card" style:border-image-source="url({art('chip_dark')})">
      <img class="mark" src={appIcon} alt="" />
      <div class="who">
        <div class="name">HS Tracker</div>
        <div class="ver">
          {#if info}version {info.version} · {info.platform}{:else}…{/if}
        </div>
      </div>
    </div>

    <div class="card" style:border-image-source="url({art('chip_dark')})">
      <div class="row"><span class="k">Made by</span><b>@Parazeya</b></div>
      <div class="row"><span class="k">Found in</span><b>the Hero Siege Discord</b></div>
      <div class="row">
        <span class="k">Source</span>
        {#if info}
          <button class="link" onclick={() => open(info.repo)}>{info.repo.replace('https://', '')}</button>
        {/if}
      </div>
    </div>

    <div class="card" style:border-image-source="url({art('chip_dark')})">
      <div class="head">OBS</div>
      <div class="note">
        Add a <b>Window Capture</b>, pick <b>[hs-tracker.exe]: HS Tracker — Overlay</b>
        and set the method to <b>Windows 10 (1903 and up)</b>. The overlay is
        already transparent and comes across as it looks.
      </div>
      <div class="note">
        Set <b>Window Match Priority</b> to <b>Window title must match</b>. On
        anything else OBS falls back to another window of the same type, and
        while the dashboard is up — the overlay being hidden — that is the
        dashboard.
      </div>
      {#if urls}
        <div class="note second">
          Or a <b>Browser Source</b>, which is also the answer if you play with
          the dashboard up: there is no overlay window to capture then. Size it
          {#if info}<b>{info.overlay_w} × {info.overlay_h}</b>{/if} to match.
        </div>
        <div class="row obs">
          <span class="k">Overlay</span>
          <button class="link" onclick={() => copy(urls[0])}>{urls[0]}</button>
        </div>
        <div class="row obs">
          <span class="k">Dashboard</span>
          <button class="link" onclick={() => copy(urls[1])}>{urls[1]}</button>
        </div>
        <div class="row obs">
          <span class="k">Announcement</span>
          <button class="link" onclick={() => copy(urls[2])}>{urls[2]}</button>
        </div>
        {#if copied}<div class="ok">copied</div>{/if}
      {/if}
    </div>

    <div class="card" style:border-image-source="url({art('chip_dark')})">
      <div class="head">Updates</div>
      <div class="line">
        <button class="btn" disabled={checking || !info} onclick={check}>
          {checking ? 'Asking GitHub…' : 'Check for a newer version'}
        </button>
      </div>

      {#if failed}
        <div class="bad">Could not check: {failed}</div>
      {:else if latest?.newer}
        <div class="good">
          <b>{latest.tag}</b> is out{latest.when ? ` — ${latest.when}` : ''}. You have {info.version}.
        </div>
        <div class="line">
          <button class="btn wide" onclick={() => open(latest.url)}>Open the download page</button>
        </div>
      {:else if latest}
        <div class="ok">This is the newest release ({latest.tag}).</div>
      {/if}
    </div>
  </div>
</div>

<style>
  @font-face {
    font-family: 'CookieRun Bold';
    src: url('./assets/fonts/cookierunbold.ttf') format('truetype');
  }

  .panel { height: 100%; }
  .body {
    height: 100%;
    box-sizing: border-box;
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-family: 'CookieRun Bold', sans-serif;
    font-size: 12px;
    color: var(--bone-6);
    overflow-y: auto;
  }

  .card {
    box-sizing: border-box;
    border: 6px solid transparent;
    border-image-slice: 6 fill;
    border-image-width: 6px;
    image-rendering: pixelated;
    padding: 10px 12px;
  }

  .card:first-child { display: flex; align-items: center; gap: 12px; }
  .mark { width: 44px; height: 44px; image-rendering: pixelated; }
  .name { font-size: 17px; color: var(--bone-13); }
  .ver { font-size: 11px; color: var(--bone-3); margin-top: 2px; }

  .row { display: flex; align-items: baseline; gap: 8px; padding: 2px 0; }
  .k { min-width: 74px; color: var(--bone-3); }
  .row b { color: var(--bone-11); font-weight: normal; }
  .row.obs .link { font-size: 11px; }

  .head { font-size: 13px; color: var(--gold-2); }
  .note { font-size: 11px; color: var(--bone-3); line-height: 1.5; margin-top: 3px; }
  .note b { color: var(--bone-11); font-weight: normal; }
  .note.second { margin-top: 8px; }

  .line { margin-top: 6px; }
  .btn {
    font: inherit;
    font-size: 12px;
    color: var(--bone-13);
    background: rgba(0, 0, 0, 0.35);
    border: 1px solid var(--edge-4);
    padding: 5px 14px;
    cursor: pointer;
  }
  .btn.wide { width: 100%; }
  .btn:hover:not(:disabled) { border-color: var(--gold-2); color: var(--gold-2); }
  .btn:disabled { opacity: 0.6; cursor: default; }

  .link {
    font: inherit;
    font-size: 12px;
    color: var(--gold-2);
    background: none;
    border: none;
    padding: 0;
    cursor: pointer;
    text-decoration: underline;
  }

  .good, .ok, .bad { margin-top: 8px; font-size: 11px; line-height: 1.5; }
  .good { color: var(--gold-2); }
  .ok { color: var(--bone-3); }
  .bad { color: #e06a6a; }
</style>

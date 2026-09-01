<script>
  import { t, locale } from './say.svelte.js';
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
      if (!r.ok) throw new Error(`${t('GitHub answered')} ${r.status}`);
      const release = await r.json();
      const tag = release.tag_name ?? '';
      latest = {
        tag: tag.replace(/^v/, ''),
        url: release.html_url ?? `${info.repo}/releases`,
        newer: newer(tag, info.version),
        when: release.published_at ? new Date(release.published_at).toLocaleDateString(locale()) : '',
      };
    } catch (e) {
      failed = String(e.message ?? e);
    }
    checking = false;
  }

  const open = (url) => invoke('open_url', { url }).catch((e) => (failed = String(e)));

  let logAt = $state('');
  $effect(() => {
    invoke('log_path').then((p) => (logAt = p)).catch(() => {});
  });

  let copied = $state(false);
  // On a Wayland session the clipboard reaches through XWayland or not at all;
  // saying "copied" either way sends the user to paste nothing.
  async function copy(text) {
    try {
      await invoke('copy_text', { text });
      copied = true;
      setTimeout(() => (copied = false), 1500);
    } catch (e) {
      failed = `${t('could not copy:')} ${e}`;
    }
  }
</script>

<div class="panel">
  <div class="body">
    <div class="card" style:border-image-source="url({art('chip_dark')})">
      <img class="mark" src={appIcon} alt="" />
      <div class="who">
        <div class="name">HS Tracker</div>
        <div class="ver">
          {#if info}{t('version')} {info.version} · {info.platform}{:else}…{/if}
        </div>
      </div>
    </div>

    <div class="card" style:border-image-source="url({art('chip_dark')})">
      <div class="row"><span class="k">{t("Made by")}</span><b>@Parazeya</b></div>
      <div class="row"><span class="k">{t("Found in")}</span><b>{t("the Hero Siege Discord")}</b></div>
      <div class="row">
        <span class="k">{t("Source")}</span>
        {#if info}
          <button class="link" onclick={() => open(info.repo)}>{info.repo.replace('https://', '')}</button>
        {/if}
      </div>
    </div>

    <div class="card" style:border-image-source="url({art('chip_dark')})">
      <div class="head">{t("If something goes wrong")}</div>
      <div class="note"> {t("Errors are written down as they happen — panics, and anything a panel throws. If you are asked for it, this is the file.")} </div>
      <div class="row path">
        <span class="k">{t("Log")}</span>
        <button class="link mono" onclick={() => copy(logAt)}>{logAt || '…'}</button>
      </div>
      {#if copied}<div class="ok">{t("copied")}</div>{/if}
      <div class="line">
        <button class="btn" onclick={() => invoke('show_log').catch((e) => (failed = String(e)))}> {t("Show it in the folder")} </button>
      </div>
    </div>

    <div class="card" style:border-image-source="url({art('chip_dark')})">
      <div class="head">{t("Updates")}</div>
      <div class="line">
        <button class="btn" disabled={checking || !info} onclick={check}>
          {checking ? t('Asking GitHub…') : t('Check for a newer version')}
        </button>
      </div>

      {#if failed}
        <div class="bad">{t('Could not check:')} {failed}</div>
      {:else if latest?.newer}
        <div class="good">
          <b>{latest.tag}</b> {t('is out')}{latest.when ? ` — ${latest.when}` : ''}. {t('You have')} {info.version}.
        </div>
        <div class="line">
          <button class="btn wide" onclick={() => open(latest.url)}>{t("Open the download page")}</button>
        </div>
      {:else if latest}
        <div class="ok">{t('This is the newest release')} ({latest.tag}).</div>
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
    font-family: var(--face);
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
  .row.path { align-items: center; }

  /* The game's face has no backslash of its own — the slot holds another
     glyph entirely, and a Windows path came out as C:wUserswExpertVw… Paths
     and addresses are read one character at a time and typed elsewhere, so
     they are set in the system's monospace and not in the skin. */
  .mono {
    font-family: ui-monospace, 'Cascadia Mono', Consolas, monospace;
    font-size: 11px;
    text-align: left;
    word-break: break-all;
  }

  .head { font-size: 13px; color: var(--gold-2); }
  .note { font-size: 11px; color: var(--bone-3); line-height: 1.5; margin-top: 3px; }

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

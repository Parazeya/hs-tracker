<script>
  import { t } from './say.svelte.js';
  import { invoke } from './bridge.js';
  import { art } from './skin.svelte.js';

  let items = $state([]);
  let draft = $state('');
  let copied = $state(null);
  let copyTimer;

  $effect(() => {
    invoke('get_shopping').then((list) => (items = list));
  });

  const persist = () => invoke('set_shopping', { items: $state.snapshot(items) }).catch(() => {});

  function add() {
    const text = draft.trim();
    if (!text) return;
    items.push(text);
    draft = '';
    persist();
  }

  function remove(i) {
    items.splice(i, 1);
    persist();
  }

  async function copy(i) {
    try {
      await invoke('copy_text', { text: items[i] });
      copied = i;
      clearTimeout(copyTimer);
      copyTimer = setTimeout(() => (copied = null), 900);
    } catch {}
  }

</script>

<div class="panel">
  <div class="entry">
    <input
      class="field"
      style:border-image-source="url({art('chip_dark')})"
      placeholder={t("add item…")}
      bind:value={draft}
      onkeydown={(e) => e.key === 'Enter' && add()}
    />
    <button
      class="btn"
      style:--btn="url({art('button')})"
      style:--btn-hover="url({art('button_hover')})"
      style:--btn-down="url({art('button_down')})"
      onclick={add}>{t("Add")}</button
    >
  </div>

  <div class="list">
    {#each items as it, i}
      <div class="row" style:border-image-source="url({art('chip_dark')})">
        <button class="text" class:copied={copied === i} onclick={() => copy(i)} title={t("Click to copy")}>
          {copied === i ? t('copied!') : it}
        </button>
        <button class="del" onclick={() => remove(i)} title={t("Remove")} aria-label={t("remove")}>×</button>
      </div>
    {:else}
      <div class="empty">{t("list is empty — add what you need to buy;")}<br />{t("click an entry to copy it")}</div>
    {/each}
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

  .entry {
    display: flex;
    gap: 6px;
    flex: none;
  }

  .field {
    box-sizing: border-box;
    flex: 1;
    min-width: 0;
    height: 27px;
    border: 6px solid transparent;
    border-image-slice: 6 fill;
    border-image-width: 6px;
    image-rendering: pixelated;
    background: none;
    font: inherit;
    color: var(--bone-9);
    padding: 0 4px;
    outline: none;
  }
  .field::placeholder { color: var(--edge-8); }

  .btn {
    box-sizing: border-box;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    line-height: 1;
    height: 28px;
    /* 60px is the sprite's own width and what "Add" needs; a language whose
       word is longer gets the room rather than wrapping it onto two lines —
       the art is a nine-slice and stretches. */
    min-width: 60px;
    flex: none;
    white-space: nowrap;
    font: inherit;
    font-size: 12px;
    color: var(--bone-12);
    text-shadow: 0 1px 0 var(--ground-2);
    background: var(--btn) no-repeat;
    background-size: 100% 100%;
    image-rendering: pixelated;
    border: none;
    cursor: pointer;
    padding: 0 10px 2px;
  }
  .btn:hover { background-image: var(--btn-hover); }
  .btn:active { background-image: var(--btn-down); }

  .list {
    flex: 1 1 auto;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .list::-webkit-scrollbar { width: 6px; }
  .list::-webkit-scrollbar-thumb { background: var(--dim-1); border-radius: 3px; }

  .row {
    box-sizing: border-box;
    flex: none;
    display: flex;
    align-items: center;
    border: 6px solid transparent;
    border-image-slice: 6 fill;
    border-image-width: 6px;
    image-rendering: pixelated;
    min-height: 27px;
  }

  .text {
    flex: 1;
    min-width: 0;
    text-align: left;
    font: inherit;
    color: inherit;
    background: none;
    border: none;
    cursor: pointer;
    padding: 2px 4px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .text:hover { color: var(--bone-13); }
  .text.copied { color: #00ffae; }

  .del {
    flex: none;
    width: 20px;
    font: inherit;
    font-size: 14px;
    color: var(--edge-7);
    background: none;
    border: none;
    cursor: pointer;
    padding: 0 4px 2px 0;
  }
  .del:hover { color: #ca1717; }

  .empty {
    padding: 16px 8px;
    text-align: center;
    font-size: 11px;
    color: var(--edge-8);
    line-height: 16px;
  }
</style>

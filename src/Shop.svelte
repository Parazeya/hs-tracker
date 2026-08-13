<script>
  import { invoke } from '@tauri-apps/api/core';

  import chipBg from './assets/game/chip_dark.png';
  import btnBg from './assets/game/button.png';
  import btnHoverBg from './assets/game/button_hover.png';
  import btnDownBg from './assets/game/button_down.png';

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
      style:border-image-source="url({chipBg})"
      placeholder="add item…"
      bind:value={draft}
      onkeydown={(e) => e.key === 'Enter' && add()}
    />
    <button
      class="btn"
      style:--btn="url({btnBg})"
      style:--btn-hover="url({btnHoverBg})"
      style:--btn-down="url({btnDownBg})"
      onclick={add}>Add</button
    >
  </div>

  <div class="list">
    {#each items as it, i}
      <div class="row" style:border-image-source="url({chipBg})">
        <button class="text" class:copied={copied === i} onclick={() => copy(i)} title="Click to copy">
          {copied === i ? 'copied!' : it}
        </button>
        <button class="del" onclick={() => remove(i)} title="Remove" aria-label="remove">×</button>
      </div>
    {:else}
      <div class="empty">list is empty — add what you need to buy;<br />click an entry to copy it</div>
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
    font-family: 'CookieRun Bold', sans-serif;
    font-size: 12px;
    color: #c3af75;
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
    color: #e0cc90;
    padding: 0 4px;
    outline: none;
  }
  .field::placeholder { color: #8a7a5a; }

  .btn {
    box-sizing: border-box;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    line-height: 1;
    height: 28px;
    width: 60px;
    flex: none;
    font: inherit;
    font-size: 12px;
    color: #e8d9b0;
    text-shadow: 0 1px 0 #1a0a0a;
    background: var(--btn) no-repeat;
    background-size: 100% 100%;
    image-rendering: pixelated;
    border: none;
    cursor: pointer;
    padding: 0 0 2px;
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
  .list::-webkit-scrollbar-thumb { background: #4a3a3a; border-radius: 3px; }

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
  .text:hover { color: #f0e0b0; }
  .text.copied { color: #00ffae; }

  .del {
    flex: none;
    width: 20px;
    font: inherit;
    font-size: 14px;
    color: #8a5a5a;
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
    color: #8a7a5a;
    line-height: 16px;
  }
</style>

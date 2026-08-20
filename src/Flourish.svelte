<script>
  // The window that stops the screen for a drop worth stopping it for.
  //
  // It is the game's own unique-loot pillar, its sparkle burst and the glow that
  // pools under a dropped item, tinted to the rarity. The sprites are white with
  // the shape in their alpha (tools/export_ui.py turns their brightness into it),
  // so they are used as masks and painted, which is what lets one set of frames
  // serve every rarity.
  import { appWindow, invoke, listen, native } from './bridge.js';
  import { itemName, tierLabel, typeLabel } from './items.js';
  import { art } from './skin.svelte.js';
  import { buffInfo } from './buffs.js';

  const RARITY_TINT = {
    Satanic: '#ca1717',
    Set: '#40d040',
    Heroic: '#00ffae',
    Angelic: '#f6f794',
    Unholy: '#e04a7a',
  };
  /// How the entrance and the exit are timed. They are fixed lengths rather
  /// than a share of the run: stretched to a share, a longer setting would only
  /// make the thing fade in slowly, when what the player asked for is a longer
  /// look at it. The middle is what grows.
  const IN_MS = 320;
  const OUT_MS = 600;

  let drop = $state(null);
  let playing = $state(false);
  /// bumped for every play: the effect is keyed on it, so its nodes are built
  /// afresh and the animations start from nothing. Toggling a class instead
  /// looks right and does nothing — the browser coalesces off-and-on-again
  /// within a frame into no change at all.
  let run = $state(0);
  let placing = $state(false);
  let cfg = $state(null);
  let timer = null;

  /// Drops waiting their turn. A boss can hand over three things at once, and
  /// announcing them on top of one another means seeing none of them — so they
  /// queue, and the window stays up until the last has been shown.
  let waiting = [];
  const QUEUE_CAP = 6;

  const SAMPLE = { rarity: 'Heroic', name: "Fenrir's Bloodfang", tier: 6, item_type: 3, weapon_type: 1 };

  const stopPlacing = () => invoke('place_flourish', { placing: false }).catch(() => {});

  // While it is being placed this window takes the mouse, and a window that
  // takes the mouse and cannot be dismissed is a trap: it sits over whatever is
  // underneath and swallows every click meant for it. Escape always ends it.
  $effect(() => {
    const key = (e) => {
      if (e.key === 'Escape' && placing) stopPlacing();
    };
    window.addEventListener('keydown', key);
    return () => window.removeEventListener('keydown', key);
  });

  $effect(() => {
    invoke('get_settings').then((s) => (cfg = s)).catch(() => {});
    const unsubs = [
      listen('settings-changed', (e) => (cfg = e.payload)),
      listen('flourish-play', (e) => enqueue(e.payload)),
      listen('flourish-placing', (e) => {
        placing = e.payload;
        if (placing) {
          waiting.length = 0;
          enqueue(SAMPLE);
        }
      }),
    ];
    return () => unsubs.forEach((u) => u.then((f) => f()));
  });

  function enqueue(entry) {
    if (!entry) return;
    // a stack of pending announcements longer than this is a wall of light
    // nobody reads; the counters have them all either way
    if (waiting.length >= QUEUE_CAP) waiting.shift();
    waiting.push(entry);
    if (!playing) advance();
  }

  function advance() {
    clearTimeout(timer);
    const next = waiting.shift();
    if (!next) {
      playing = false;
      drop = null;
      // the window knows how long it takes, so the window says when to hide.
      // Served to a browser there is no window to hide, and nothing to say.
      if (!placing && native) invoke('flourish_done').catch(() => {});
      return;
    }
    drop = next;
    playing = true;
    run += 1;
    timer = setTimeout(() => {
      // While it is being placed the sample loops, so the size and the shading
      // can be judged without farming for an hour. Through the queue and then
      // `advance` directly: `enqueue` only starts anything when nothing is
      // playing, and nothing ever cleared `playing` here — so the promised
      // loop played once and the box sat empty for the rest of the session.
      if (placing) waiting.push(SAMPLE);
      advance();
    }, runMs);
  }

  let tint = $derived(RARITY_TINT[drop?.rarity] ?? '#f0e0b0');
  let label = $derived.by(() => {
    if (!drop) return '';
    if (drop.name) return drop.name;
    const known = itemName(drop.item_type, drop.item_id, drop.weapon_type);
    return known ?? typeLabel(drop.item_type, drop.weapon_type);
  });
  let runMs = $derived(Math.round(Math.min(12, Math.max(2, cfg?.flourish_secs ?? 6)) * 1000));
  let scale = $derived(Math.min(2, Math.max(0.5, cfg?.flourish_scale ?? 1)));
  let shade = $derived(Math.min(1, Math.max(0, cfg?.flourish_shade ?? 0.55)));
</script>

<div
  class="stage"
  class:playing
  class:placing
  style:--tint={tint}
  style:--scale={scale}
  style:--in="{IN_MS}ms"
  style:--out="{OUT_MS}ms"
  style:--hold="{Math.max(0, runMs - OUT_MS)}ms"
  style:--shade={shade}
  style:--sparks="url({art('fx_sparks')})"
  style:--glow="url({art('fx_glow')})"
>
  {#key run}
    <div class="fx" class:playing>
      <!-- The shading is a pool of shadow rather than a panel: the window is
           transparent, so a solid background would be a black box sitting on
           the game. It darkens the middle and fades to nothing at the edges. -->
      <div class="shade"></div>
      <div class="glow"></div>
      <div class="sparks left"></div>
      <div class="sparks right"></div>
      <div class="sparks over"></div>
      {#if drop}
        <div class="caption">
          <div class="caption-head">
            <span class="rar">{drop.kind === 'zone' ? 'Satanic Zone' : drop.rarity}</span>
            <span class="name">{label}</span>
            {#if drop.tier > 0}<span class="grade">{tierLabel(drop.tier)}</span>{/if}
          </div>
          {#if drop.buffs && Array.isArray(drop.buffs) && drop.buffs.length > 0}
            <div class="zone-buffs">
              {#each drop.buffs as bId}
                {@const b = buffInfo(bId)}
                <div class="zone-buff-pill" title={b ? `${b.name} : ${b.desc}` : ''}>
                  {#if b?.icon}<img class="buff-icon" src={b.icon} alt="" />{/if}
                  <span class="buff-text">{b?.name ?? `Buff ${bId}`}</span>
                </div>
              {/each}
            </div>
          {/if}
        </div>
      {/if}
    </div>
  {/key}

  {#if placing}
    <!-- while it is being placed the window takes the mouse, so it can be
         dragged, and says what it is -->
    <div class="place" data-tauri-drag-region>
      <div class="hint" data-tauri-drag-region>Drag this box where you want drops announced</div>
      <button class="done" onclick={stopPlacing}>Done — or press Esc</button>
    </div>
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
  }

  .stage {
    position: relative;
    width: 100vw;
    height: 100vh;
    font-family: 'CookieRun Bold', sans-serif;
    overflow: hidden;
  }

  .shade {
    position: absolute;
    inset: 0;
    background: radial-gradient(
      ellipse 46% 52% at 50% 50%,
      rgba(0, 0, 0, var(--shade)) 0%,
      rgba(0, 0, 0, calc(var(--shade) * 0.55)) 45%,
      rgba(0, 0, 0, 0) 72%
    );
    opacity: 0;
  }
  .fx.playing .shade {
    animation: appear var(--in) ease-out forwards,
               vanish var(--out) ease-in var(--hold) forwards;
  }
  @keyframes appear { from { opacity: 0 } to { opacity: 1 } }
  /* See main.js. Fading the shade in means twenty paints of a half-transparent
     black, and on a desktop that never clears the surface those add up: the
     soft pool arrives as a hard blob. There it is simply there, and simply
     gone — two paints, and the gradient keeps the shape it was given. */
  :global(html[data-os='linux']) .fx.playing .shade {
    animation: none;
    opacity: 1;
  }
  :global(html[data-os='linux']) .fx.playing .glow {
    animation: swell var(--in) ease-out forwards,
               vanish var(--out) ease-in var(--hold) forwards,
               glowframes 1s steps(15) infinite;
  }
  @keyframes vanish { from { opacity: 1 } to { opacity: 0 } }

  /* Everything centres on the name: the glow behind it, the sparks around it.
     The whole group scales together from its middle. */
  .fx {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    transform: scale(var(--scale));
  }

  .sparks, .glow {
    position: absolute;
    opacity: 0;
    background: var(--tint);
    -webkit-mask-repeat: no-repeat;
    mask-repeat: no-repeat;
    pointer-events: none;
  }

  /* the pool of light the game puts under a dropped item, stretched wide enough
     to sit behind a name rather than under a sprite */
  .glow {
    width: 340px;
    height: 120px;
    -webkit-mask-image: var(--glow);
    mask-image: var(--glow);
    -webkit-mask-size: 5100px 120px;
    mask-size: 5100px 120px;
  }
  .fx.playing .glow {
    animation: swell var(--in) ease-out forwards,
               vanish var(--out) ease-in var(--hold) forwards,
               glowframes 1s steps(15) infinite;
  }
  @keyframes swell {
    from { opacity: 0; transform: scale(0.7) }
    to { opacity: 0.5; transform: scale(1) }
  }
  @keyframes glowframes { to { -webkit-mask-position: -5100px 0; mask-position: -5100px 0 } }

  /* three bursts around the name rather than one on top of it */
  .sparks {
    width: 96px;
    height: 96px;
    -webkit-mask-image: var(--sparks);
    mask-image: var(--sparks);
    -webkit-mask-size: 1344px 96px;
    mask-size: 1344px 96px;
  }
  .sparks.left { margin-right: 210px; margin-top: -14px }
  .sparks.right { margin-left: 210px; margin-top: 18px }
  .sparks.over { margin-bottom: 74px; width: 72px; height: 72px }
  .fx.playing .sparks {
    animation: pop var(--in) ease-out forwards,
               vanish var(--out) ease-in var(--hold) forwards,
               /* the burst keeps going for as long as the thing is up */
               sparkframes 700ms steps(14) infinite;
  }
  .fx.playing .sparks.right { animation-delay: 140ms, var(--hold), 140ms }
  .fx.playing .sparks.over { animation-delay: 280ms, var(--hold), 280ms }
  @keyframes pop {
    from { opacity: 0; transform: scale(0.6) }
    to { opacity: 1; transform: scale(1) }
  }
  @keyframes sparkframes { to { -webkit-mask-position: -1344px 0; mask-position: -1344px 0 } }

  .caption {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    font-size: 19px;
    white-space: nowrap;
    text-shadow: 0 2px 0 #000, 0 0 12px #000, 0 0 24px var(--tint);
    opacity: 0;
  }
  .caption-head {
    display: flex;
    align-items: baseline;
    gap: 8px;
  }
  .zone-buffs {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    flex-wrap: wrap;
    max-width: 90vw;
  }
  .zone-buff-pill {
    display: flex;
    align-items: center;
    gap: 5px;
    background: rgba(10, 8, 8, 0.85);
    border: 1px solid var(--tint);
    border-radius: 4px;
    padding: 3px 8px;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.6);
  }
  .zone-buff-pill .buff-icon {
    width: 18px;
    height: 18px;
    flex: none;
  }
  .zone-buff-pill .buff-text {
    color: #f4e6bb;
    font-size: 13px;
    letter-spacing: 0.02em;
    text-shadow: 0 1px 2px #000;
  }
  .fx.playing .caption {
    animation: rise var(--in) ease-out forwards,
               vanish var(--out) ease-in var(--hold) forwards;
  }
  @keyframes rise {
    from { opacity: 0; transform: translateY(8px) scale(0.94) }
    to { opacity: 1; transform: translateY(0) scale(1) }
  }
  .rar {
    color: var(--tint);
    font-size: 12px;
    letter-spacing: 0.12em;
    text-transform: uppercase;
  }
  .name { color: #f4e6bb }
  .grade {
    color: var(--tint);
    font-size: 12px;
    border: 1px solid var(--tint);
    padding: 0 4px;
  }

  /* only while it is being parked */
  /* It has to be unmistakable. Transparent and outlined in a thin dash, it was
     a window nobody could see grabbing clicks nobody could explain. */
  /* No fill: it is drawn after .fx with no z-index, so a full-bleed scrim
     painted over the very sample the player is here to judge. The dashed
     border and the button are enough to say what this box is. */
  .place {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    border: 2px dashed rgba(232, 216, 168, 0.75);
    box-sizing: border-box;
    cursor: move;
  }
  .hint {
    font-size: 13px;
    color: #e8d8a8;
    text-shadow: 0 1px 0 #000;
  }
  .done {
    font: inherit;
    font-size: 13px;
    color: #e8d8a8;
    background: rgba(0, 0, 0, 0.85);
    border: 1px solid #8a7a5a;
    padding: 6px 20px;
    cursor: pointer;
  }
  .done:hover { border-color: #e8c860; }
</style>

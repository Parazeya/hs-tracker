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
  import { buffInfo, zoneName } from './buffs.js';
  import { art, css } from './skin.svelte.js';

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
  /// The other thing this window draws, so a position can be judged against
  /// both before it is committed to. The zone announcement is the wider of the
  /// two, and picking a spot that only suits the drop is how it ends up half
  /// off the screen an hour later.
  const ZONE_SAMPLE = { kind: 'zone', zone: 'Satanic_5_5', buffs: [2, 3, 14, 21, 24], debuffs: [3, 9] };
  /// More than this and the plate is a wall of names nobody reads inside six
  /// seconds; the rest are counted instead.
  const BUFFS_SHOWN = 5;

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
    if (waiting.length >= QUEUE_CAP) {
      // Except the rotation, which is not evictable by a drop. It happens once
      // an hour and a boss hands over three things at once, so first-in-first-out
      // spends the rare announcement to make room for the common one.
      const i = waiting.findIndex((e) => e.kind !== 'zone');
      waiting.splice(i < 0 ? 0 : i, 1);
    }
    waiting.push(entry);
    if (!playing) advance();
  }

  function advance() {
    clearTimeout(timer);
    const next = waiting.shift();
    if (!next) {
      playing = false;
      drop = null;
      // the window knows how long it takes, so the window says when to hide
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
      // alternating, so both layouts are seen in the one loop
      if (placing) waiting.push(next.kind === 'zone' ? SAMPLE : ZONE_SAMPLE);
      advance();
    }, runMs);
  }

  let isZone = $derived(drop?.kind === 'zone');
  /// What the plate lists, capped, with the overflow kept as a count rather
  /// than dropped silently.
  let zbuffs = $derived.by(() => {
    const ids = drop?.buffs ?? [];
    const shown = ids.slice(0, BUFFS_SHOWN).map((id) => ({ id, ...buffInfo(id) }));
    if (ids.length > BUFFS_SHOWN) shown.push({ id: -1, more: ids.length - BUFFS_SHOWN });
    return shown;
  });

  let tint = $derived(RARITY_TINT[drop?.rarity] ?? '#f0e0b0');
  let label = $derived.by(() => {
    if (!drop || drop.kind === 'zone') return '';
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
  style:--sparks={css('fx_sparks')}
  style:--glow={css('fx_glow')}
  style:--plate={css('header')}
  style:--chipart={css('chip_dark')}
>
  {#key run}
    {#if isZone}
      <!-- A drop is a column of light standing on a pool; a rotation is a band
           that splits open across the window. Not one node in common with the
           branch below, which is what makes them impossible to confuse from the
           other side of the room — the point of the window. -->
      <div class="zfx" class:playing>
        <div class="rift">
          <div class="band"><div class="sweep"></div></div>
          <div class="zbody">
            <div class="zkind">
              <img src={art('satanic_star')} alt="" />
              <span class="txt">Satanic Zone</span>
              <img src={art('satanic_star')} alt="" />
              {#if drop.debuffs?.length}
                <!-- the buffs are the decision, the curses are the small print:
                     spelling them out doubles the height for something nobody
                     picks a zone by -->
                <span class="zcurse">{drop.debuffs.length} curses</span>
              {/if}
            </div>
            <div class="zplate"><span class="zname">{zoneName(drop.zone)}</span></div>
            {#if zbuffs.length}
              <div class="zbuffs">
                <!-- keyed by position: the ids come from the packet and
                     nothing dedupes them, and a repeat would throw
                     `each_key_duplicate` in a window nobody can see fail -->
                {#each zbuffs as b, i (i)}
                  <div class="zbuff" class:more={b.more}>
                    {#if b.more}
                      <span class="bname">+{b.more} more</span>
                    {:else}
                      <img src={b.icon} alt="" />
                      <span class="bname">{b.name}</span>
                    {/if}
                  </div>
                {/each}
              </div>
            {:else}
              <div class="znone">no buffs this rotation</div>
            {/if}
          </div>
        </div>
      </div>
    {:else}
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
          <span class="rar">{drop.rarity}</span>
          <span class="name">{label}</span>
          {#if drop.tier > 0}<span class="grade">{tierLabel(drop.tier)}</span>{/if}
        </div>
      {/if}
    </div>
    {/if}
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

  /* The pool the announcement stands in, and the only part of it the player has
     a slider for — so what that slider does has to be worth doing. The alpha is
     its, the geometry is not, and the geometry is what the setting kept running
     into: an ellipse 46% wide that reached nothing at 72% of its radius stopped
     186px from the middle of a 560px window, while a long name runs to 200. The
     outer third of every long drop was set on the bare game at any setting, and
     turning the slider to the top darkened a blob between the words. Wider, and
     flatter across the middle, so the name is backed end to end.
     Not wider than the window, though: `.stage` clips, and a clipped gradient
     is a straight edge of shadow with a corner on it. Nothing at 49% of the
     width and 45% of the height leaves a margin either side at every size. */
  .shade {
    position: absolute;
    inset: 0;
    /* And it must not take `.fx`'s scale. The window itself already grows with
       that setting — FLOURISH_W * scale, in lib.rs — so scaling the gradient
       inside it applies the same number twice: at 200% the pool was magnified
       to twice the window and clipped at both sides while still two thirds
       opaque, which is the rectangle this comment exists to prevent. */
    transform: scale(calc(1 / var(--scale)));
    background: radial-gradient(
      ellipse 60% 55% at 50% 50%,
      rgba(0, 0, 0, var(--shade)) 0%,
      rgba(0, 0, 0, calc(var(--shade) * 0.72)) 55%,
      rgba(0, 0, 0, 0) 82%
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
    align-items: baseline;
    gap: 8px;
    font-size: 19px;
    white-space: nowrap;
    text-shadow: 0 2px 0 #000, 0 0 12px #000, 0 0 24px var(--tint);
    opacity: 0;
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

  /* ── The satanic zone. ───────────────────────────────────────────────────
     A drop is a column of light; a rotation is a band that opens across the
     window. The axis, the edges and the sprites all differ on purpose: the two
     never share the screen, so the only thing telling them apart is the shape
     each leaves in the corner of the eye.

     Authored at 560x220 in fixed pixels. Not one percentage width in here —
     .zfx is inset:0 and then scaled, so `width: 100%` would be 1120px before
     the scale and 2240 after it. */
  .zfx {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    transform: scale(var(--scale));

    --ember: #ff3a2e;
    --ember-lit: #ffb08a;
    /* the token the panel's zone chip turns its name to when the zone moves.
       It carries a meaning rather than a season, so both skins agree on it. */
    --zone-ink: var(--rar-satanic, #ff6a6a);
  }

  /* Height comes from what is in it — two buffs is a thinner band than five —
     and is capped, so a zone carrying more than the game has ever sent still
     cannot push past the window. */
  .rift {
    position: relative;
    box-sizing: border-box;
    width: 560px;
    max-height: 220px;
    padding: 10px 24px 12px;
    display: flex;
    flex-direction: column;
    align-items: center;
    overflow: hidden;
  }

  /* The slab. Its black is the player's own shade setting, so that slider still
     means something here. Masked at both ends: the window is 560 wide, and a
     bar that stops dead at the edge reads as a bug rather than as a band
     crossing the screen. */
  .band {
    position: absolute;
    inset: 0;
    transform-origin: 50% 50%;
    opacity: 0;
    overflow: hidden;
    background:
      radial-gradient(
        ellipse 70% 130% at 50% 50%,
        rgba(255, 58, 46, calc(var(--shade) * 0.2)) 0%,
        rgba(255, 58, 46, 0) 70%
      ),
      linear-gradient(
        180deg,
        rgba(0, 0, 0, 0) 0%,
        rgba(0, 0, 0, calc(var(--shade) * 1.35)) 14%,
        rgba(0, 0, 0, calc(var(--shade) * 1.55)) 50%,
        rgba(0, 0, 0, calc(var(--shade) * 1.35)) 86%,
        rgba(0, 0, 0, 0) 100%
      );
    -webkit-mask-image: linear-gradient(90deg, transparent 0, #000 10%, #000 90%, transparent 100%);
    mask-image: linear-gradient(90deg, transparent 0, #000 10%, #000 90%, transparent 100%);
  }
  /* The two lips, held 3px inside the slab so the bloom pools inward instead of
     being cut off by the clip. */
  .band::before,
  .band::after {
    content: '';
    position: absolute;
    left: 0;
    right: 0;
    height: 2px;
    background: var(--ember);
    box-shadow: 0 0 10px 1px var(--ember), 0 0 26px rgba(255, 58, 46, 0.55);
    animation: lip-breathe 2.4s ease-in-out infinite alternate;
  }
  .band::before { top: 3px }
  .band::after { bottom: 3px }

  .zfx.playing .band {
    animation: rift-open var(--in) cubic-bezier(0.16, 1, 0.3, 1) forwards,
               rift-shut var(--out) ease-in var(--hold) forwards;
  }
  /* It arrives as the hairline the two lips make when they are together, and it
     leaves the same way. Nothing else in the app opens like this. */
  @keyframes rift-open {
    from { opacity: 0; transform: scaleY(0.06) }
    60% { opacity: 1 }
    to { opacity: 1; transform: scaleY(1) }
  }
  @keyframes rift-shut {
    from { opacity: 1; transform: scaleY(1) }
    to { opacity: 0; transform: scaleY(0.06) }
  }
  @keyframes lip-breathe {
    from { opacity: 0.75 }
    to { opacity: 1 }
  }

  /* The sweep the panel's zone chip already gets when the zone moves, borrowed
     so the two say the same thing. Slower and a third of the brightness: there
     it crosses 240px for three seconds and is meant to nag, here it crosses 560
     for six and is meant to be lived with. */
  .sweep {
    position: absolute;
    inset: 0;
    pointer-events: none;
    background: linear-gradient(100deg, transparent 30%, rgba(255, 255, 255, 0.14) 50%, transparent 70%);
    animation: rift-sweep 2.6s linear infinite;
  }
  @keyframes rift-sweep {
    from { transform: translateX(-100%) }
    to { transform: translateX(100%) }
  }

  /* A sibling of the slab, not a child: the slab scales on its way in, and a
     scaling parent would squash the text with it. One fade takes the column out
     together at the end. */
  .zbody {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: center;
  }
  .zfx.playing .zbody {
    animation: vanish var(--out) ease-in var(--hold) forwards;
  }

  .zkind {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 23px;
    opacity: 0;
  }
  /* native 23x23 and drawn at 23: this is pixel art, and a fractional size
     turns the pentagram to mush */
  .zkind img {
    width: 23px;
    height: 23px;
    image-rendering: pixelated;
    filter: drop-shadow(0 0 6px var(--ember));
  }
  .zkind .txt {
    font-size: 11px;
    letter-spacing: 0.34em;
    /* letter-spacing hangs a gap off the last letter, which walks the whole
       line half a space left of centre */
    margin-right: -0.34em;
    text-transform: uppercase;
    color: var(--ember-lit);
    text-shadow: 0 2px 0 #000, 0 0 14px var(--ember);
  }
  .zkind .zcurse {
    font-size: 10px;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    color: #9a6a6a;
    text-shadow: 0 1px 0 #000;
  }
  .zfx.playing .zkind {
    animation: fall-in var(--in) ease-out 60ms forwards;
  }
  /* down from above — the drop's caption rises from below, and that alone reads
     before either has been focused on */
  @keyframes fall-in {
    from { opacity: 0; transform: translateY(-10px) }
    to { opacity: 1; transform: translateY(0) }
  }

  /* The panel's own zone plate at twice the width. The player already knows
     this shape means "zone"; nothing else in this window wears it. */
  .zplate {
    box-sizing: border-box;
    width: 384px;
    height: 40px;
    margin-top: 6px;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0 18px;
    background-image: var(--plate);
    background-size: 100% 100%;
    background-repeat: no-repeat;
    image-rendering: pixelated;
    transform-origin: 50% 50%;
    opacity: 0;
  }
  .zname {
    font-size: 19px;
    line-height: 1;
    color: var(--zone-ink);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    text-shadow: 0 2px 0 #000, 0 0 16px rgba(255, 58, 46, 0.8);
  }
  .zfx.playing .zplate {
    animation: plate-in var(--in) cubic-bezier(0.2, 0.9, 0.2, 1) 120ms forwards;
  }
  /* widens rather than swells: the drop's glow grows from a point in every
     direction, this one runs out along the band */
  @keyframes plate-in {
    from { opacity: 0; transform: scaleX(0.66) }
    to { opacity: 1; transform: scaleX(1) }
  }

  /* Two fixed columns, so five buffs are three rows and the longest name in the
     table still fits without an ellipsis. Fixed rather than content-sized: a
     ragged pair of edges is a caption, a squared-off pair is a sheet, and a
     sheet is what is being read. */
  .zbuffs {
    display: grid;
    grid-template-columns: repeat(2, 210px);
    gap: 6px 14px;
    justify-content: center;
    margin-top: 10px;
  }
  .zbuff {
    box-sizing: border-box;
    min-width: 0;
    height: 33px;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0 8px;
    border: 6px solid transparent;
    border-image-source: var(--chipart);
    border-image-slice: 6 fill;
    border-image-width: 6px;
    image-rendering: pixelated;
    white-space: nowrap;
    opacity: 0;
  }
  /* an odd one out sits under the middle rather than hanging off the left */
  .zbuff:last-child:nth-child(odd) {
    grid-column: 1 / -1;
    justify-self: center;
    width: max-content;
  }
  .zbuff img {
    width: 21px;
    height: 21px;
    flex: none;
    image-rendering: pixelated;
  }
  .zbuff .bname {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    font-size: 12px;
    color: #f4e6bb;
    text-shadow: 0 1px 0 #000;
  }
  .zbuff.more .bname { color: var(--ember-lit) }
  .znone {
    margin-top: 10px;
    height: 33px;
    display: flex;
    align-items: center;
    font-size: 12px;
    color: #9a6a6a;
    text-shadow: 0 1px 0 #000;
    opacity: 0;
  }
  .zfx.playing .zbuff,
  .zfx.playing .znone {
    animation: chip-in 260ms ease-out 200ms forwards;
  }
  /* one after another, so the list is read as a list. The last starts at 400ms
     and has settled by 660 — the shortest hold this window allows is 1400, so
     it never collides with the fade out. */
  .zfx.playing .zbuff:nth-child(2) { animation-delay: 240ms }
  .zfx.playing .zbuff:nth-child(3) { animation-delay: 280ms }
  .zfx.playing .zbuff:nth-child(4) { animation-delay: 320ms }
  .zfx.playing .zbuff:nth-child(5) { animation-delay: 360ms }
  .zfx.playing .zbuff:nth-child(6) { animation-delay: 400ms }
  @keyframes chip-in {
    from { opacity: 0; transform: translateY(6px) }
    to { opacity: 1; transform: translateY(0) }
  }

  /* See main.js, and the shade above. Twenty paints of a half-transparent black
     on a desktop that never clears its surface arrive as a hard blob, so the
     band is simply there and simply gone. The text keeps its fades: it is
     small, and the caption already does the same. */
  :global(html[data-os='linux']) .zfx.playing .band {
    animation: none;
    opacity: 1;
    transform: none;
  }

  @media (prefers-reduced-motion: reduce) {
    .zfx.playing .band {
      animation: appear var(--in) ease-out forwards,
                 vanish var(--out) ease-in var(--hold) forwards;
    }
    .band::before,
    .band::after { animation: none; opacity: 1 }
    .sweep { animation: none; opacity: 0.1 }
    .zfx.playing .zkind,
    .zfx.playing .zplate,
    .zfx.playing .zbuff,
    .zfx.playing .znone {
      animation: appear var(--in) ease-out forwards;
    }
  }
</style>

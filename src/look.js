// How a drop's name is painted on the announcement pillar.
//
// Two readers, one answer: the pillar itself and the preview on the Watchlist
// tab. They have to agree exactly or the preview is a lie, so the resolving and
// the CSS both live here rather than once in each window.
//
// What is stored is small on purpose — an effect, two colours and a number —
// because it is written into settings.json against an item's name and a player
// may well have a hundred of them. See `Look` in src-tauri/src/lib.rs.

/// The colour each rarity paints with. Here rather than in the pillar's own
/// file because the Watchlist has to show the same starting point the pillar
/// would have used, and two copies of a colour table drift.
export const RARITY_TINT = {
  Satanic: '#ca1717',
  Set: '#40d040',
  Heroic: '#00ffae',
  Angelic: '#f6f794',
  Unholy: '#e04a7a',
};

/// What an item wears when nobody has said otherwise: the colour its rarity
/// would have given it anyway.
export const DEFAULT_LOOK = { effect: 'rarity', color: '', color2: '', glow: 100 };

/// The effects, in the order the picker offers them.
export const EFFECTS = [
  ['rarity', 'Rarity colour'],
  ['solid', 'One colour'],
  ['gradient', 'Two-colour gradient'],
];

/// The glow, as a percentage of what the pillar draws by default.
///
/// Past two hundred it stops being a light on a transparent window and becomes
/// a fog with a word in it; at zero the name is drawn flat, which is what
/// somebody streaming at a low bitrate wants.
export const GLOW_MAX = 200;

/// What the player set for this item, filled in from the defaults.
///
/// Keyed by the lowercased English name — the same key a watchlist stores, so
/// the two cannot drift — and deliberately not by the list: the same item on
/// two lists is the same find, and having it come up green on one and gold on
/// the other would say something about the drop that is not true.
export function lookFor(looks, name) {
  const set = looks?.[String(name ?? '').trim().toLowerCase()];
  return set ? { ...DEFAULT_LOOK, ...set } : { ...DEFAULT_LOOK };
}

/// The same, for a drop as it arrives off the wire.
///
/// Two items can wear one name and the seven Essence Vaults share theirs across
/// every rarity, so a watchlist stores those as "Name (Rarity)" and the packet
/// carries the bare name and the rarity apart. The qualified key is tried first
/// and the bare one second, which is the order `listed_sound` uses in the
/// engine for the same reason — an appearance set on the Angelic vault must not
/// end up on the Satanic one.
export function lookForDrop(looks, name, rarity) {
  const bare = String(name ?? '').trim();
  if (rarity) {
    const qualified = looks?.[`${bare} (${rarity})`.toLowerCase()];
    if (qualified) return { ...DEFAULT_LOOK, ...qualified };
  }
  return lookFor(looks, bare);
}

/// Whether this is anything other than what the rarity would have done. The
/// row on the Watchlist says so, so a player can see which of a hundred items
/// they have dressed without opening each one.
export function isCustom(look) {
  const it = { ...DEFAULT_LOOK, ...(look ?? {}) };
  return it.effect !== DEFAULT_LOOK.effect || it.glow !== DEFAULT_LOOK.glow;
}

const hex = (c) => (/^#[0-9a-fA-F]{6}$/.test(String(c ?? '')) ? c : null);

/// The CSS for the name itself, as an inline style string.
///
/// A gradient cannot use `text-shadow`: the text is painted transparent so the
/// background shows through the glyphs, and a text shadow under transparent
/// text is all anyone would see. `drop-shadow` filters the rendered result
/// instead, which is the same light and works for both.
export function nameStyle(look, tint) {
  const it = { ...DEFAULT_LOOK, ...(look ?? {}) };
  const glow = Math.max(0, Math.min(GLOW_MAX, Number(it.glow) || 0)) / 100;
  const one = hex(it.color) || tint;
  const two = hex(it.color2) || one;
  // The two black layers are the reading: a hard drop for the edge and a soft
  // one to lift the word off whatever is behind the window. Only the coloured
  // layers answer to the slider, because those are the decoration.
  //
  // Two of them, tight and wide. One wide blur alone spreads the colour so thin
  // that moving the slider looked like it did nothing — the light was there and
  // nobody could see it change. The tight layer is what reads as a glow at all.
  const lit = glow > 0
    ? `, 0 0 ${Math.round(7 * glow)}px ${one}, 0 0 ${Math.round(20 * glow)}px ${one}`
    : '';
  if (it.effect === 'gradient') {
    const shadow = glow > 0
      ? ` drop-shadow(0 0 ${Math.round(4 * glow)}px ${one}) drop-shadow(0 0 ${Math.round(12 * glow)}px ${one})`
      : '';
    return (
      `background-image:linear-gradient(100deg,${one},${two});` +
      '-webkit-background-clip:text;background-clip:text;color:transparent;' +
      `filter:drop-shadow(0 2px 0 #000) drop-shadow(0 0 6px #000)${shadow}`
    );
  }
  const colour = it.effect === 'solid' ? one : '#f4e6bb';
  return `color:${colour};text-shadow:0 2px 0 #000, 0 0 12px #000${lit}`;
}

/// The colour the rest of the caption keys off — the rules beside the rarity
/// word, the grade's box. It follows the item's own colour once one is set, so
/// a dressed item is dressed all through rather than in one patch.
export function accentOf(look, tint) {
  const it = { ...DEFAULT_LOOK, ...(look ?? {}) };
  return (it.effect !== 'rarity' && hex(it.color)) || tint;
}

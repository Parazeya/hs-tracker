// Which set of art the windows are wearing.
//
// The palette in theme.css only reaches what CSS paints. The panels, chips and
// buttons are PNGs, and a season has its own copies of them under
// assets/game/<season>/ (see tools/gen_skin.py). Both sets are bundled, and this
// is what decides which one a component asks for.
//
// One skin is not a season at all. `modern` wears no game art: its chrome is
// drawn by modern.css out of borders and radii, and the handful of sprites that
// are genuinely icons — a lock, a tick, a close cross — are line SVGs under
// assets/modern/ instead. So `art` has two jobs: name a file, and say when
// there is no file to name, which is what tells a component to let CSS draw.
//
// It is a rune rather than a plain variable so that every `art(...)` in a
// component re-runs when the skin changes: switching themes in Settings repaints
// the windows without a reload.

const GAME = import.meta.glob('./assets/game/**/*.png', { eager: true, import: 'default' });
const MODERN = import.meta.glob('./assets/modern/*.svg', { eager: true, import: 'default' });

let skin = $state('default');

export function wearSkin(name) {
  skin = name && name !== 'default' ? name : 'default';
}

/// Whether the chrome is CSS rather than sprites. Read it to drop a wrapper
/// that only exists to hold art — never to choose a colour, which is what the
/// tokens are for.
export function flat() {
  return skin === 'modern';
}

/// A sprite by name, without the folder or the extension.
///
/// A season that has no copy of one falls back to the original, so a
/// half-finished skin still draws. `modern` does NOT fall back: a pixel-art
/// panel behind a flat one would be the whole point missed, so a name it has no
/// SVG for answers null, and the caller draws nothing.
export function art(name) {
  if (skin === 'modern') return MODERN[`./assets/modern/${name}.svg`] ?? null;
  return GAME[`./assets/game/${skin}/${name}.png`] ?? GAME[`./assets/game/${name}.png`];
}

/// The same, as a CSS image value.
///
/// Components set their art through `style:` directives, and a name with no
/// file behind it used to interpolate to the string `url(null)` — which is an
/// invalid value the webview drops, leaving whatever the stylesheet had said
/// before it. `none` is a value CSS has a meaning for, so the declaration lands
/// and the property really is cleared.
export function css(name) {
  const url = art(name);
  return url ? `url(${url})` : 'none';
}

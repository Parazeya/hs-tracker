// Which set of the game's sprites the windows are wearing.
//
// The palette in theme.css only reaches what CSS paints. The panels, chips and
// buttons are PNGs, and a season has its own copies of them under
// assets/game/<season>/ (see tools/gen_skin.py). Both sets are bundled, and this
// is what decides which one a component asks for.
//
// It is a rune rather than a plain variable so that every `art(...)` in a
// component re-runs when the skin changes: switching themes in Settings repaints
// the windows without a reload.

const FILES = import.meta.glob('./assets/game/**/*.png', { eager: true, import: 'default' });

let skin = $state('default');

export function wearSkin(name) {
  skin = name && name !== 'default' ? name : 'default';
}

/// A sprite by name, without the folder or the extension. A season that has no
/// copy of one falls back to the original, so a half-finished skin still draws.
export function art(name) {
  return FILES[`./assets/game/${skin}/${name}.png`] ?? FILES[`./assets/game/${name}.png`];
}

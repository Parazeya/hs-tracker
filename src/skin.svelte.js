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

/// The plain skin is not a set of sprites, so it is the one skin `art` answers
/// for itself.
///
/// A season is the same nine-slice PNGs in other colours; plain is the absence
/// of them — a real border, a radius and a flat ground, which recolouring a
/// stretched pixel frame cannot give. So the art has to stop arriving, and it
/// stops here rather than at a hundred and thirty call sites.
///
/// Two answers, because callers use the result two ways. A `border-image` takes
/// `none` and plain.css draws the border instead. A checkbox is an `<img>` in
/// dozens of places, so it gets a drawing rather than a rule — `src="none"`
/// would be that many broken images — sized to the 18px the components give it.
///
/// The checkbox is shadcn's: a hairline until ticked, then the primary colour
/// with a dark foreground on it, white being unreadable at that contrast. An SVG
/// in a `src` cannot read a custom property, so the colours are spelled out
/// here; as markup rather than base64, because a colour nobody can read is a
/// colour that drifts from the palette.
const NOTHING = 'none';

const svg = (body) =>
  'data:image/svg+xml,' +
  encodeURIComponent(
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 18 18" width="18" height="18">${body}</svg>`,
  );

const PLATE = '<rect x="1.5" y="1.5" width="15" height="15" rx="3"';
const TICK =
  '<path d="M5 9.2 L7.7 11.9 L13 6.3" fill="none" stroke-width="2"' +
  ' stroke-linecap="round" stroke-linejoin="round" stroke="';
/// One drawing, two grounds. The plate has to match the surface it sits on and
/// the tick has to be legible on the accent, and neither answer survives being
/// carried across: #a78bfa cannot be read on paper and #7c3aed cannot take a
/// dark tick.
const PAIR = {
  plain: [
    svg(`${PLATE} fill="#18181b" stroke="#52525b" stroke-width="1"/>`),
    svg(`${PLATE} fill="#a78bfa" stroke="#a78bfa" stroke-width="1"/>${TICK}#18181b"/>`),
  ],
  plainlight: [
    svg(`${PLATE} fill="#ffffff" stroke="#a1a1aa" stroke-width="1"/>`),
    svg(`${PLATE} fill="#7c3aed" stroke="#7c3aed" stroke-width="1"/>${TICK}#ffffff"/>`),
  ],
};

/// What the plain skin draws itself. The rest — the coin, the frost, the
/// sparks, the satanic star — is the game's own iconography rather than the
/// window's frame, and it belongs on any ground.
const CHROME = new Set(['panel', 'chip', 'chip_dark', 'button', 'button_hover', 'button_down',
  'header', 'backdrop']);
/// Both names of the plain skin draw their own chrome; a season does not.
const PLAIN = new Set(['plain', 'plainlight']);

/// A sprite by name, without the folder or the extension. A season that has no
/// copy of one falls back to the original, so a half-finished skin still draws.
export function art(name) {
  if (PLAIN.has(skin)) {
    if (CHROME.has(name)) return NOTHING;
    if (name === 'check_off') return PAIR[skin][0];
    if (name === 'check_on') return PAIR[skin][1];
  }
  return FILES[`./assets/game/${skin}/${name}.png`] ?? FILES[`./assets/game/${name}.png`];
}

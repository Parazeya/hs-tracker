// Which set of art the windows are wearing.
//
// The palette in theme.css only reaches what CSS paints. The panels, chips and
// buttons are PNGs, and a season has its own copies of them under
// assets/game/<season>/ (see tools/gen_skin.py). Both sets are bundled, and this
// is what decides which one a component asks for.
//
// Three skins are not seasons at all, and they arrived here from two
// directions: `modern` is this fork's, `plain` and `plainlight` are upstream's,
// and both answer the same question — what a window looks like with no game art
// on it — by different means. They are kept apart rather than folded together,
// because each one's components were written against its own mechanism:
//
//   modern            no file to name. Its chrome is drawn by modern.css out of
//                     borders and radii, and the sprites that are genuinely
//                     icons — a lock, a tick, a close cross — are line SVGs
//                     under assets/modern/. `flat()` is how a component knows
//                     to drop a wrapper that exists only to hold art.
//   plain, plainlight the chrome names answer `none` and plain.css draws the
//                     border instead, so the wrapper stays and the art stops
//                     arriving. `flat()` is false for them: nothing should be
//                     removed from markup plain.css is styling.
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

/// What the plain skins draw for themselves.
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

/// Whether the chrome is CSS rather than sprites *and the markup holding it can
/// go*. Read it to drop a wrapper that only exists to hold art — never to
/// choose a colour, which is what the tokens are for.
///
/// True for `modern` only. The plain skins reach the same look by keeping the
/// wrapper and styling it, so removing one would take plain.css's border with
/// it.
export function flat() {
  return skin === 'modern';
}

/// A sprite by name, without the folder or the extension.
///
/// A season that has no copy of one falls back to the original, so a
/// half-finished skin still draws. The flat skins do NOT fall back: a pixel-art
/// panel behind a flat one would be the whole point missed. `modern` answers
/// null for a name it has no SVG for and the caller draws nothing; plain
/// answers `none` for the chrome and a drawing for the checkbox.
export function art(name) {
  if (skin === 'modern') return MODERN[`./assets/modern/${name}.svg`] ?? null;
  if (PLAIN.has(skin)) {
    if (CHROME.has(name)) return NOTHING;
    if (name === 'check_off') return PAIR[skin][0];
    if (name === 'check_on') return PAIR[skin][1];
  }
  return GAME[`./assets/game/${skin}/${name}.png`] ?? GAME[`./assets/game/${name}.png`];
}

/// The same, as a CSS image value.
///
/// Components set their art through `style:` directives, and a name with no
/// file behind it used to interpolate to the string `url(null)` — which is an
/// invalid value the webview drops, leaving whatever the stylesheet had said
/// before it. `none` is a value CSS has a meaning for, so the declaration lands
/// and the property really is cleared.
///
/// It is the one accessor a `style:` should use, because it is where the three
/// answers `art` can give — a URL, null from modern, `none` from plain — become
/// the two CSS has words for.
export function css(name) {
  const url = art(name);
  return url && url !== NOTHING ? `url(${url})` : 'none';
}

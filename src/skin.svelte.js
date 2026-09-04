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

import { remember } from './bridge.js';

const FILES = import.meta.glob('./assets/game/**/*.png', { eager: true, import: 'default' });

let skin = $state('default');
/// The accent the plain skins wear, as `#rrggbb`, or empty for the one the
/// palette ships with. A rune for the same reason `skin` is: the checkbox is a
/// drawing rather than a rule, and it has to be redrawn when this moves.
let accent = $state('');

export function wearSkin(name) {
  skin = name && name !== 'default' ? name : 'default';
}

/**
 * Wear a whole theme: the palette, the shape and the sprites.
 *
 * All three move together, so they are set together, and here rather than in
 * main.js because the picker in Settings has to be able to do it too. Every
 * window still follows the backend's `settings-changed`; what this adds is the
 * window the picker is in, which used to wait for that event like the others.
 *
 * Waiting was the whole of it: a save that cannot reach the disk emits nothing,
 * and the theme then never changed anywhere — the picker moved, the colours did
 * not, and nothing on the screen said why.
 */
/// One notch darker, for the second half of the accent pair.
///
/// Every palette carries two: the lighter one is the text and the darker one
/// the edge under it. One colour for both flattens the difference the skins
/// were drawn with, and asking the player for two colours to answer one
/// question is not a setting, it is homework.
function darker(hex) {
  const m = /^#?([0-9a-f]{6})$/i.exec(hex ?? '');
  if (!m) return hex;
  const n = parseInt(m[1], 16);
  const cut = (v) => Math.max(0, Math.round(v * 0.88));
  return `#${[(n >> 16) & 255, (n >> 8) & 255, n & 255]
    .map((v) => cut(v).toString(16).padStart(2, '0'))
    .join('')}`;
}

export function wearTheme(name, tint) {
  const root = document.documentElement;
  if (name && name !== 'default') root.setAttribute('data-theme', name);
  else root.removeAttribute('data-theme');
  // The palette rides on data-theme; the SHAPE rides on this. Only the plain
  // skin has one, because only it replaces the art with rules — a season is the
  // same nine-slice PNGs in other colours and needs nothing here. There are two
  // plain palettes and one plain shape, so both wear the same skin name.
  if (name === 'plain' || name === 'plainlight') root.setAttribute('data-skin', 'plain');
  else root.removeAttribute('data-skin');
  remember('theme', name ?? 'default');
  // The accent is the plain skins' alone: a season is the game's own art in the
  // game's own colours, and recolouring the text over it would leave the two
  // halves disagreeing.
  const plain = name === 'plain' || name === 'plainlight';
  accent = plain && /^#[0-9a-f]{6}$/i.test(tint ?? '') ? tint : '';
  if (accent) {
    root.style.setProperty('--gold-2', accent);
    root.style.setProperty('--gold-1', darker(accent));
  } else {
    root.style.removeProperty('--gold-2');
    root.style.removeProperty('--gold-1');
  }
  // What was asked for, not what was applied: a season clears the accent
  // while it is worn, and the choice has to survive being switched away from.
  remember('accent', tint ?? '');
  wearSkin(name);
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
/// carried across: a light accent cannot be read on paper and a dark one cannot
/// take a dark tick.
///
/// Built when asked rather than once, because the accent moves now: the ticked
/// plate is the accent, so the drawing has to be made again when it changes.
const WORN = { plain: '#a78bfa', plainlight: '#7c3aed' };
const pair = (which) => {
  const on = accent || WORN[which];
  return which === 'plain'
    ? [
        svg(`${PLATE} fill="#18181b" stroke="#52525b" stroke-width="1"/>`),
        svg(`${PLATE} fill="${on}" stroke="${on}" stroke-width="1"/>${TICK}#18181b"/>`),
      ]
    : [
        svg(`${PLATE} fill="#ffffff" stroke="#a1a1aa" stroke-width="1"/>`),
        svg(`${PLATE} fill="${on}" stroke="${on}" stroke-width="1"/>${TICK}#ffffff"/>`),
      ];
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
    if (name === 'check_off') return pair(skin)[0];
    if (name === 'check_on') return pair(skin)[1];
  }
  return FILES[`./assets/game/${skin}/${name}.png`] ?? FILES[`./assets/game/${name}.png`];
}

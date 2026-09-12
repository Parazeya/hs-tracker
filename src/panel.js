// What the overlay panel can show, and where it shows it by default.
//
// Two readers: App.svelte draws the cells and Settings.svelte arranges them.
// The list was written out in both and in lib.rs besides, and three copies of
// an order is a reading added to two of them.
//
// The grid itself is not in here because it is not a choice: every row is
// 140 + 124 + 124 with two 8px gaps, the same column boundaries all the way
// down, and the size of a cell comes from its position and never from what is
// put in it. See `.row` in App.svelte for what depends on that.

/// Every reading a cell can hold: the id a settings file stores, and the
/// English the picker shows. The order is also the layout the panel ships
/// with — the first three are the first row, and so on.
export const READINGS = [
  ['time', 'Run time'],
  ['mail', 'Mail'],
  ['boss', 'Bosses'],
  ['rare', 'Angelic | Unholy'],
  ['sat', 'Satanic'],
  ['heroset', 'Heroic | Set'],
  ['gold', 'Gold'],
  ['goldh', 'Gold per hour'],
  ['kills', 'Kills'],
  ['xp', 'Experience'],
  ['xph', 'Experience per hour'],
  ['ss', 'SS drops'],
];

/// The layout as it ships. `OVERLAY_SLOTS` in src-tauri/src/lib.rs holds the
/// same order for one reason only: the window has to guess its own height for
/// the frame before the page has measured itself. A copy that drifted would
/// cost that one frame and nothing after it.
export const DEFAULT_SLOTS = READINGS.map(([id]) => id);

/// The cells as the player left them, padded to whole rows.
///
/// Padded rather than refused: a file holding a short or odd-length list —
/// hand-edited, or written by a version with fewer readings — should leave the
/// panel one row short of what it expects, not missing a column.
export function slotsOf(stored) {
  const held = (stored?.length ? stored : DEFAULT_SLOTS).slice();
  while (held.length % 3) held.push('');
  return held;
}

/// Every id this version can draw. A layout may name one it cannot — a file
/// written by a newer version, read after going back a release — and that cell
/// is a gap here rather than a row of nothing. The stored layout is untouched,
/// so the reading comes back when the newer version does.
const KNOWN = new Set(DEFAULT_SLOTS);

/// The same, cut into rows of three, with the empty rows dropped. Emptying a
/// row is how a reading is removed rather than merely blanked, and the window
/// shrinks by that row.
export function rowsOf(stored) {
  const flat = slotsOf(stored).map((cell) => (KNOWN.has(cell) ? cell : ''));
  const out = [];
  for (let i = 0; i < flat.length; i += 3) {
    const row = flat.slice(i, i + 3);
    if (row.some((cell) => cell)) out.push(row);
  }
  return out;
}

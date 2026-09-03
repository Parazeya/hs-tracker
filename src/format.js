// The numbers, said the same way everywhere.
//
// One copy on purpose. Four panels and the run card all format the same figures,
// and a second copy drifts: the card once said `2.4M` where the panel it was
// copied from said `2.40kk`, on the one artefact that leaves the app.
//
// Hero Siege says kk and kkk rather than M and B, which is why these are not
// the SI suffixes a general-purpose helper would reach for.
//
// The words go through the language runtime — `Hell 3` and `1h 24m` are read in
// every panel — so a formatter answering in English would be the last English
// left in a translated window.

import { t, locale } from './say.svelte.js';

/** 1234 -> "1,234"; 12345 -> "12.3k"; 1234567 -> "1.23kk". */
export function fmt(n) {
  const v = n ?? 0;
  const abs = Math.abs(v);
  if (abs >= 1e9) return `${(v / 1e9).toFixed(2)}kkk`;
  if (abs >= 1e6) return `${(v / 1e6).toFixed(2)}kk`;
  // below ten thousand the digits still fit, and reading them exactly is
  // worth more than the two characters saved
  if (abs >= 10_000) return `${(v / 1e3).toFixed(1)}k`;
  return v.toLocaleString(locale());
}

/** Seconds as a running clock: 3661 -> "1:01:01". Hours never wrap. */
export function clock(secs) {
  const s = Math.max(0, Math.floor(secs ?? 0));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  return `${h}:${String(m).padStart(2, '0')}:${String(s % 60).padStart(2, '0')}`;
}

/** Seconds as a length of time, for reading rather than watching: "1h 24m". */
export function span(secs) {
  const s = Math.max(0, Math.floor(secs ?? 0));
  // Rounding the remainder gives "60m" for anything from 59m30s: round the
  // whole thing into minutes first, then split, and an hour stays an hour.
  const mins = Math.round(s / 60);
  const h = Math.floor(mins / 60);
  const m = mins % 60;
  if (h && m) return `${h}${t('h')} ${m}${t('m')}`;
  if (h) return `${h}${t('h')}`;
  return `${m}${t('m')}`;
}

/// The colour each rarity is drawn in. The same five classes are declared in
/// four components' styles; this is the one place that says which is which.
export const RARITY_CLASS = {
  Satanic: 'c-sat',
  Set: 'c-set',
  Heroic: 'c-her',
  Angelic: 'c-ang',
  Unholy: 'c-unh',
};

/// How the ten rarities rank against each other, in the order the engine keeps
/// them (see RARITIES in stats.rs). Beside the classes above for the reason
/// those are here at all: the run card sorts by this, and a second private copy
/// is exactly how the card's number formatting drifted from the panel's.
export const RARITY_RANK = {
  Unholy: 10,
  Heroic: 9,
  Blessed: 8,
  Angelic: 7,
  Satanic: 6,
  Mythic: 5,
  Set: 4,
  Rare: 3,
  Superior: 2,
  Common: 1,
};

/// The five the app counts, in the order every panel lists them.
export const RARITIES = ['Satanic', 'Set', 'Heroic', 'Angelic', 'Unholy'];

/// What the game calls the difficulty a character is on.
///
/// Falls back to `D<n>` rather than guessing. Season 10 (21 August 2026) retires
/// Nightmare and splits Hell into five grades, so these numbers will come to
/// mean different names — which is deliberately not written in yet: the packets
/// carry a season number whose relation to the season's public name is not
/// established (a character playing season 9 reports 10), and a wrong name looks
/// like the app understood where `D4` does not.
export function difficulty(n, hellSub = 0) {
  if (n == null) return null;
  const name = ['Normal', 'Nightmare', 'Hell', 'Inferno'][n] ?? `D${n}`;
  // Hell is five difficulties wearing one name, and the game says which in a
  // field of its own. Only shown on Hell: it carries a value on characters who
  // are not there, and reading it out then would be inventing a fact.
  return name === 'Hell' && hellSub >= 1 && hellSub <= 5 ? `${t('Hell')} ${hellSub}` : t(name);
}

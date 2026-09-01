// What the game calls things, in the language the reader picked.
//
// `items.js` is English and stays English: it is what the app loads first, what
// every lookup by name matches against, and what everything falls back to. The
// other ten ride in `src/lang/<code>.json`, written by tools/gen_items.py from
// the game's own translation files, and one of them is fetched when it is
// chosen. Ten alphabets in the bundle would be 600 KB nobody reading English
// needs.
//
// Two rules hold this together.
//
// A name is *displayed* through here and *matched* in English. A watchlist is a
// list of names a user built, a sound is keyed by one, and the parser reads the
// name the game announced: none of those may move when the reader changes
// language, or every list they own empties itself. So `items.js` is still what
// answers `RARITY_BY_NAME` and what a filter stores.
//
// And nothing here throws for a missing language. A file that failed to fetch,
// a name the game never translated, a season that added an item after the
// tables were built — all three land on English, which is always there.

import {
  ITEMS,
  itemName as englishItem,
  typeLabel as englishType,
  zoneLabel as englishZone,
} from './items.js';
import { invoke } from './bridge.js';

const FILES = import.meta.glob('./lang/*.json');

/// The ten the game ships. There is no Turkish: the community has a channel for
/// it and the game has no column, so the picker cannot offer one.
export const LANGUAGES = [
  ['en', 'English'],
  ['de', 'Deutsch'],
  ['sp', 'Español'],
  ['fi', 'Suomi'],
  ['fr', 'Français'],
  ['pl', 'Polski'],
  ['pt', 'Português'],
  ['ru', 'Русский'],
  ['zh', '中文'],
  ['ja', '日本語'],
  ['ko', '한국어'],
];

let lang = $state('en');
let said = $state(null);

/// Wear a language. Returns once its names are in, so a caller that wants to
/// paint after the switch can wait; nothing has to.
export async function speak(code) {
  const want = LANGUAGES.some(([c]) => c === code) ? code : 'en';
  lang = want;
  if (want === 'en') {
    said = null;
    tell();
    return;
  }
  const load = FILES[`./lang/${want}.json`];
  if (!load) {
    said = null;
    tell();
    return;
  }
  try {
    const file = await load();
    // the import may resolve after a second switch; the last one asked wins
    if (lang === want) said = file.default ?? file;
  } catch {
    said = null;
  }
  tell();
}

/// Hand the vocabulary to the backend.
///
/// The tray menu, the file dialogs, the errors Rust returns and the Discord
/// card are all words this app coined, and they are printed on the far side of
/// the bridge where none of this is in scope. Rather than build the catalogue
/// twice — once for the page and once into the binary — the page sends the one
/// it has, and Rust reads a line out of it. Rooms go over under a prefix
/// because their keys are the game's, not English.
function tell() {
  const words = { ...(said?.ui ?? {}) };
  for (const [key, name] of Object.entries(said?.rooms ?? {})) words[`room:${key}`] = name;
  // Not on the page: this is a desktop app talking to its own backend, and a
  // failure here means the backend is not up yet, which the next switch fixes.
  invoke('set_words', { words }).catch(() => {});
}

export function language() {
  return lang;
}

/// The tag Intl wants, for the dates and the thousands separators. The game's
/// codes are close to BCP-47 but not it: `sp` is Spanish, and English stays
/// en-GB because that is the order and the clock the panels were written for.
const INTL = { en: 'en-GB', sp: 'es', zh: 'zh-CN' };
export function locale() {
  return INTL[lang] ?? lang;
}

/// The name of an item, by the identity the packet carries.
export function itemName(type, id, weaponType) {
  const key = `${type}:${id}:${weaponType}`;
  return said?.items?.[key] || englishItem(type, id, weaponType);
}

/// The identity an English name belongs to, built once and only if asked.
///
/// A saved run keeps `{name, rarity, tier}` and no identity, and a watchlist is
/// a list of names the user chose: both have the name and nothing else, so the
/// only way to the other ten languages is back through the English table.
///
/// Eleven names belong to two items each — the game calls both a Set gun and a
/// Heroic orb "Angel" — and the first one wins here. That is a display choice
/// and not a claim: a rarity or a grade is never read this way, only a word to
/// print, and the two items called "Angel" are called one thing in German too.
let backwards = null;
function identityOf(englishName) {
  if (!backwards) {
    backwards = new Map();
    for (const key in ITEMS) {
      const lower = ITEMS[key].toLowerCase();
      if (!backwards.has(lower)) backwards.set(lower, key);
    }
  }
  return backwards.get(englishName?.toLowerCase?.() ?? '');
}

/// The name of an item this app already holds in English — a watchlist row, a
/// find in a saved run. Pass the identity when the caller has one; it is exact,
/// and the lookup by name is the fallback for the callers that do not.
export function nameOf(englishName, type, id, weaponType) {
  if (!englishName) return englishName;
  // All three or none: a caller that knows the type but not the id was building
  // "2:undefined:0", missing, and falling back to the English — with the
  // translation sitting in the file the whole time.
  const known = type != null && id != null && weaponType != null;
  const key = known ? `${type}:${id}:${weaponType}` : identityOf(englishName);
  return (key && said?.items?.[key]) || englishName;
}

export function typeLabel(type, weaponType) {
  if (said) {
    const from = type === 3 && weaponType > 0 ? said.weapons : said.types;
    const key = type === 3 && weaponType > 0 ? weaponType : type;
    const word = from?.[key];
    if (word) return word;
  }
  return englishType(type, weaponType);
}


export function zoneLabel(room) {
  return said?.rooms?.[room] || englishZone(room);
}

/// The satanic zone, which the server names by act and index — "SZ_9_2" — and
/// not by the room key the game's own translation file is written against.
///
/// `buffs.js` turns that into "Act 9 : Shipwreck Cove" from a table of its own,
/// which is English and only English. The same pair of numbers is a room key
/// the game has a name for in all eleven languages, so the numbers are what is
/// carried across and the words come from the game.
/// A Satanic Zone place, by the act and the index its announcement carries.
/// Empty when the language does not name it, so the caller can fall back to the
/// English table it already holds.
/// The same lookup with the numbers put back in: say('{n} ticked', { n: 4 }).
/// A sentence chopped into wrapped fragments comes out in English word order
/// whatever the language, so anything with a count in the middle is one key.
export function say(english, vars) {
  let out = t(english);
  for (const [name, value] of Object.entries(vars ?? {})) {
    out = out.split(`{${name}}`).join(value);
  }
  return out;
}

/// Where a chase item drops, in the reader's language.
///
/// "Act IX Zone 4-5", "Sheeponia (Inferno Only)", "Uber Damien" — one grammar
/// with four pieces, and the whole string is never looked up: the act numeral
/// and the zone range are numbers and stay as they are, and each word around
/// them is asked for on its own. A piece the tables cannot answer for comes
/// back English, which leaves the sentence readable rather than blank.
const QUALIFIER = /\s*\((Inferno Only|Inferno Difficulty|Inferno)\)$/;
const ACT_PLACE = /^Act ([IVX]+(?: & [IVX]+)?) (.+)$/;

export function placeLabel(place) {
  const raw = String(place ?? '').trim();
  if (!raw) return '';
  const qualifier = raw.match(QUALIFIER);
  const bare = qualifier ? raw.slice(0, qualifier.index) : raw;
  const act = bare.match(ACT_PLACE);
  let said = t(bare);
  if (act) {
    const zone = act[2].match(/^Zone (.+)$/);
    said = `${t('Act')} ${act[1]} ${zone ? `${t('Zone')} ${zone[1]}` : t(act[2])}`;
  }
  return qualifier ? `${said} (${t(qualifier[1])})` : said;
}

/// The same list a drop row prints, joined the way it was written.
export function placeList(where, join = ' · ') {
  return (where ?? []).map(placeLabel).join(join);
}

export function actZone(act, idx) {
  return said?.acts?.[`${act}_${idx}`] || '';
}

export function satanicZoneName(raw, english) {
  const m = String(raw ?? '').match(/(\d+)[_-](\d+)\s*$/);
  if (!m) return english;
  const act = Number(m[1]);
  const key = `Act_${String(act).padStart(2, '0')}_${String(Number(m[2])).padStart(2, '0')}`;
  const named = said?.rooms?.[key];
  return named ? `${t('Act')} ${act} : ${named}` : english;
}

/// What this app says itself, as against what the game names.
///
/// Keyed by the English, which is also the fallback: a string with no entry in
/// the chosen language reads as it always did, so a half-translated language is
/// a mixed window rather than a broken one. The catalogue is `tools/said.py`
/// and it rides in the same file as the game's own words — a reader of another
/// language is fetching that file anyway, and an English reader fetches none.
export function t(english) {
  return said?.ui?.[english] || english;
}

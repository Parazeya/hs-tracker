// Where things are on THIS machine, and nowhere in the repository.
//
// The build and item scripts need three paths that differ per developer: the
// game, the extractor beside this checkout, and a Visual Studio that can
// actually compile. Hard-coding them published one person's drive letters and
// left everyone else editing scripts to build.
//
// Order: the environment wins, then .env beside the checkout, then a guess that
// is right often enough to be worth making. Nothing here fails on a missing
// value — a script that needs one says so itself, with the name to set.
//
// Copy .env.example to .env and edit it. .env is git-ignored; .env.example is
// the documentation.

import { existsSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join, resolve } from 'node:path';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');

/// A deliberately small reader: KEY=value, # comments, no quoting rules to
/// learn and no dependency to install. Anything already in the environment
/// wins, so a one-off override does not mean editing a file.
function dotenv() {
  const at = join(root, '.env');
  if (!existsSync(at)) return {};
  const out = {};
  for (const line of readFileSync(at, 'utf8').split(/\r?\n/)) {
    const text = line.trim();
    if (!text || text.startsWith('#')) continue;
    const eq = text.indexOf('=');
    if (eq < 1) continue;
    out[text.slice(0, eq).trim()] = text.slice(eq + 1).trim().replace(/^["']|["']$/g, '');
  }
  return out;
}

const file = dotenv();
const read = (name) => process.env[name] || file[name] || '';

/// The first candidate that exists, or the first one at all so an error message
/// has something to name.
const firstThatExists = (...candidates) => candidates.find(existsSync) ?? candidates[0];

export const REPO = root;

/// The game's bin folder — where data.win, Hero_Siege.exe and the translation
/// tables live.
export const GAME =
  read('HERO_SIEGE_BIN') ||
  firstThatExists(
    'C:/Program Files (x86)/Steam/steamapps/common/HeroSiege/bin',
    'D:/Steam/steamapps/common/HeroSiege/bin',
  );

/// The extractor that reads the game. It is a separate tool in a separate
/// folder, on purpose: it knows about Hero Siege, not about this app.
export const EXTRACTOR =
  read('HS_EXTRACTOR') ||
  resolve(root, '..', 'HeroSiege Extractor');

/// A Visual Studio whose CRT headers are actually installed. cargo finds a
/// compiler by looking for cl.exe, not by checking that it works, so a broken
/// installation is found first and every build dies in vswhom-sys.
export const VCVARS =
  read('VCVARS') ||
  firstThatExists(
    'C:/Program Files/Microsoft Visual Studio/2022/BuildTools/VC/Auxiliary/Build/vcvars64.bat',
    'C:/Program Files/Microsoft Visual Studio/2022/Community/VC/Auxiliary/Build/vcvars64.bat',
    'C:/Program Files (x86)/Microsoft Visual Studio/2019/BuildTools/VC/Auxiliary/Build/vcvars64.bat',
  );

/// The private half of the update signing key, and its password if it has one.
///
/// Never in the repository, and never printed. The app only installs a package
/// signed by this key — `plugins.updater.pubkey` in tauri.conf.json is its
/// public half — so losing it means no machine already running this app can
/// ever be updated again, only reinstalled by hand. Back it up somewhere that
/// is not this laptop.
///
/// `tauri build` reads these as TAURI_SIGNING_PRIVATE_KEY(_PASSWORD); the
/// build script puts them in the child's environment so they never reach a
/// command line, where Windows would record them in the console history.
export const SIGNING_KEY_PATH =
  read('TAURI_SIGNING_PRIVATE_KEY_PATH') ||
  firstThatExists(join(process.env.USERPROFILE || process.env.HOME || '', '.tauri', 'hs-tracker.key'));

export const SIGNING_KEY_PASSWORD = read('TAURI_SIGNING_PRIVATE_KEY_PASSWORD');

/// Say which one is missing and what to set, rather than failing on a path the
/// reader has never heard of.
export function require(name, value, what) {
  if (value && existsSync(value)) return value;
  console.error(
    `\n  Cannot find ${what}.\n` +
      `    looked at: ${value || '(nothing)'}\n` +
      `    set ${name} in ${join(root, '.env')} or in the environment.\n` +
      `    See .env.example.\n`,
  );
  process.exit(1);
}

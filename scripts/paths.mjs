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
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { dirname, isAbsolute, join, resolve } from 'node:path';

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
const rooted = (value) => value ? (isAbsolute(value) ? value : resolve(root, value)) : '';

export function steamLibraries({ platform = process.platform, env = process.env, registry } = {}) {
  const personal = env.USERPROFILE || env.HOME || '';
  const candidates = platform === 'win32'
    ? [join(env['ProgramFiles(x86)'] || 'C:/Program Files (x86)', 'Steam'), join(env.ProgramFiles || 'C:/Program Files', 'Steam')]
    : platform === 'darwin'
      ? [join(personal, 'Library', 'Application Support', 'Steam')]
      : [join(personal, '.local', 'share', 'Steam'), join(personal, '.steam', 'steam')];
  if (platform === 'win32') {
    try {
      const value = registry ?? execFileSync('reg.exe', ['query', 'HKCU\\Software\\Valve\\Steam', '/v', 'SteamPath'], { encoding: 'utf8', windowsHide: true, stdio: ['ignore', 'pipe', 'ignore'] });
      const match = value.match(/SteamPath\s+REG_SZ\s+(.+)/i);
      if (match) candidates.unshift(match[1].trim());
    } catch { /* Steam is optional; a prebuilt bundle needs no installation. */ }
  }
  const all = new Set(candidates);
  for (const steam of candidates) {
    const path = join(steam, 'steamapps', 'libraryfolders.vdf');
    if (!existsSync(path)) continue;
    for (const match of readFileSync(path, 'utf8').matchAll(/"path"\s+"((?:\\.|[^"\\])*)"/g)) {
      all.add(match[1].replace(/\\\\/g, '\\'));
    }
  }
  return [...all];
}

export function findGame(libraries = steamLibraries()) {
  const candidates = libraries.map((library) => {
    const manifest = join(library, 'steamapps', 'appmanifest_269210.acf');
    const folder = existsSync(manifest)
      ? readFileSync(manifest, 'utf8').match(/"installdir"\s+"([^"]+)"/)?.[1] || 'HeroSiege'
      : 'HeroSiege';
    return join(library, 'steamapps', 'common', folder, 'bin');
  });
  return candidates.find((candidate) => existsSync(join(candidate, 'data.win'))) || candidates[0] || '';
}

function configuredPath(names, fallback) {
  const explicit = names.map((name) => process.env[name]).find(Boolean);
  if (explicit) return rooted(explicit);
  const saved = names.map((name) => file[name]).find(Boolean);
  if (saved && existsSync(rooted(saved))) return rooted(saved);
  if (saved) console.warn(`Saved ${names[0]} path is unavailable; using local discovery. An environment override remains authoritative.`);
  return fallback();
}

/// The first candidate that exists, or the first one at all so an error message
/// has something to name.
const firstThatExists = (...candidates) => candidates.find(existsSync) ?? candidates[0];

export const REPO = root;

/// The game's bin folder — where data.win, Hero_Siege.exe and the translation
/// tables live.
export const GAME =
  configuredPath(['HERO_SIEGE_BIN', 'HERO_SIEGE'], findGame);

/// The extractor that reads the game. It is a separate tool in a separate
/// folder, on purpose: it knows about Hero Siege, not about this app.
export const EXTRACTOR =
  configuredPath(['HS_EXTRACTOR'], () => resolve(root, '..', 'HeroSiege Extractor'));

export const EXTRACTOR_BIN = rooted(read('HS_EXTRACTOR_BIN')) ||
  join(EXTRACTOR, 'target', 'release', process.platform === 'win32' ? 'hse-extractor.exe' : 'hse-extractor');

export const BUNDLE = rooted(read('HS_DATA_BUNDLE'));
const python = read('PYTHON');
const localPython = join(EXTRACTOR, '.venv', ...(process.platform === 'win32' ? ['Scripts', 'python.exe'] : ['bin', 'python']));
export const PYTHON = python && /[\\/]/.test(python) ? rooted(python) : python || (existsSync(localPython) ? localPython : 'python');

function findVCVars() {
  const installer = join(process.env['ProgramFiles(x86)'] || 'C:/Program Files (x86)', 'Microsoft Visual Studio', 'Installer', 'vswhere.exe');
  if (existsSync(installer)) {
    try {
      const installs = JSON.parse(execFileSync(installer, ['-all', '-products', '*', '-format', 'json'], { encoding: 'utf8', windowsHide: true }));
      for (const install of installs) {
        const candidate = join(install.installationPath, 'VC', 'Auxiliary', 'Build', 'vcvars64.bat');
        if (existsSync(candidate)) return candidate;
      }
    } catch { /* Fall back to the conventional locations below. */ }
  }
  return firstThatExists(
    'C:/Program Files/Microsoft Visual Studio/2022/BuildTools/VC/Auxiliary/Build/vcvars64.bat',
    'C:/Program Files/Microsoft Visual Studio/2022/Community/VC/Auxiliary/Build/vcvars64.bat',
    'C:/Program Files (x86)/Microsoft Visual Studio/2019/BuildTools/VC/Auxiliary/Build/vcvars64.bat',
  );
}

/// The certificate the Windows build signs with, by its SHA-1 thumbprint, and
/// empty when there is none — an unsigned build is the ordinary case and must
/// keep working.
///
/// Read here rather than straight off `process.env` so that .env is where it
/// can go, beside the game path and the toolchain. That is the first place
/// anybody looks, and this file is the only thing that reads .env: nothing
/// puts those keys into the environment.
///
/// Not in tauri.conf.json: a thumbprint committed to a public repository names
/// a certificate for anyone who cares to go looking for it.
export const SIGN_THUMBPRINT = read('HS_SIGN_THUMBPRINT').replace(/[^0-9a-fA-F]/g, '');

/// Timestamping is not optional. Without it every installer already cut stops
/// verifying on the day the certificate expires, rather than staying good for
/// what it was signed with while it was valid.
export const SIGN_TIMESTAMP = read('HS_SIGN_TIMESTAMP') || 'http://timestamp.digicert.com';

/// A Visual Studio whose CRT headers are actually installed. cargo finds a
/// compiler by looking for cl.exe, not by checking that it works, so a broken
/// installation is found first and every build dies in vswhom-sys.
export const VCVARS = configuredPath(['VCVARS'], findVCVars);

/// The private half of the key an update is signed with. It lives outside the
/// checkout on purpose: the app installs nothing this key did not sign, so a
/// copy in the repository would hand that power to everyone who clones it.
///
/// `tauri signer generate -w ~/.tauri/hs-tracker.key` writes it, and the public
/// half beside it belongs in tauri.conf.json.
export const SIGNING_KEY =
  read('TAURI_SIGNING_PRIVATE_KEY_PATH') ||
  join(process.env.USERPROFILE || process.env.HOME || '.', '.tauri', 'hs-tracker.key');

/// tauri reads the key from the environment rather than from a path, so put it
/// there. False if there is no key to load — the caller decides what that
/// means, and for a release build it means stopping.
export function loadSigningKey() {
  if (process.env.TAURI_SIGNING_PRIVATE_KEY) return true;
  if (!existsSync(SIGNING_KEY)) return false;
  process.env.TAURI_SIGNING_PRIVATE_KEY = readFileSync(SIGNING_KEY, 'utf8').trim();
  // Empty is a password, and one tauri accepts; unset is a prompt nobody is
  // there to answer.
  process.env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD = read('TAURI_SIGNING_PRIVATE_KEY_PASSWORD');
  return true;
}

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

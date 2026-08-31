// `npm run all` — every artifact of a release, built on this machine.
//
// The CI path keeps failing on something that has nothing to do with the code:
// the runner points apt at a mirror that goes dark, and a release triggered by
// a tag runs the workflow AS IT WAS AT THAT TAG, so fixing the workflow does
// not fix the run — the tag has to move. Building here sidesteps all of it, and
// the Linux half is containerised anyway, so it is the same package either way.
//
//   npm run all                 # everything this machine can build
//   npm run all -- --linux      # Linux only
//   npm run all -- --windows    # Windows only
//
// Everything means the Windows installer, the .deb, the .rpm and the AppImage.
// All three Linux packages come out of the one container: tauri writes the RPM
// itself rather than calling rpmbuild, so the image needs nothing Fedora has.
// CI still uses a Fedora container for the .rpm, which is the more careful
// place to record dependency names — if the two ever disagree, trust that one.
//
// Windows is built directly because that is the host; Linux goes through the
// image `npm run deb` already uses.

import { execFileSync, execSync } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, readFileSync, readdirSync, statSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join, resolve } from 'node:path';
import { SIGNING_KEY_PASSWORD, SIGNING_KEY_PATH, VCVARS } from './paths.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
// only this build's artifacts are collected; older ones stay where they are
const version = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8')).version;
const args = process.argv.slice(2);
const has = (f) => args.includes(f);
const only = has('--linux') || has('--windows');
const wantWindows = !only || has('--windows');
const wantLinux = !only || has('--linux');

const run = (file, argv) => {
  console.log(`\n  $ ${file} ${argv.join(' ')}\n`);
  execFileSync(file, argv, { cwd: root, stdio: 'inherit', shell: process.platform === 'win32' });
};

// Newest first, so what this run produced is at the top rather than buried
// under every release that came before it.
// Everything a release needs, in one place.
//
// The pieces are built in three different directories by two different
// toolchains, and the Linux one now clears its bundle directory before each
// run — which is right, it was shipping a .deb from a fortnight ago, but it
// means a package is only on disk until the next bundle. So each one is
// copied here as soon as it exists rather than gathered at the end.
const RELEASE = join(root, 'release');

function collect(dir, exts) {
  if (!existsSync(dir)) return;
  mkdirSync(RELEASE, { recursive: true });
  const walk = (at) => {
    for (const name of readdirSync(at)) {
      const path = join(at, name);
      if (statSync(path).isDirectory()) walk(path);
      else if (exts.some((e) => name.endsWith(e)) && name.includes(version)) {
        copyFileSync(path, join(RELEASE, name));
      }
    }
  };
  walk(dir);
}

function show(dir, exts) {
  if (!existsSync(dir)) return;
  const found = [];
  const walk = (at) => {
    for (const name of readdirSync(at)) {
      const path = join(at, name);
      if (statSync(path).isDirectory()) walk(path);
      else if (exts.some((e) => name.endsWith(e))) found.push([statSync(path).mtimeMs, path]);
    }
  };
  walk(dir);
  for (const [, path] of found.sort((a, b) => b[0] - a[0]).slice(0, 8)) {
    console.log(`    ${path.slice(root.length + 1)}`);
  }
}

// The signing key, for the update manifest.
//
// `createUpdaterArtifacts` is on, so every bundle writes a .sig beside the
// installer — and cannot, without this. Passed through the environment rather
// than the command line: an argument would be in the console history and in
// the process list, and this one opens every install of the app to whoever
// reads it.
//
// A build with no key is allowed and says so. It produces an installer people
// can run, and no update anybody already running the app will accept, which is
// worth one line of warning rather than a refusal — there are reasons to want
// an unsigned local build.
function signing() {
  if (!SIGNING_KEY_PATH || !existsSync(SIGNING_KEY_PATH)) {
    console.log('  signing:   none — this build carries no update signature');
    console.log('             see DEVELOPING.md, "Updates"');
    return {};
  }
  console.log(`  signing:   ${SIGNING_KEY_PATH}`);
  return {
    TAURI_SIGNING_PRIVATE_KEY: readFileSync(SIGNING_KEY_PATH, 'utf8').trim(),
    TAURI_SIGNING_PRIVATE_KEY_PASSWORD: SIGNING_KEY_PASSWORD ?? '',
  };
}

if (wantWindows) {
  console.log('== Windows ==');
  const env = { ...process.env, ...signing() };
  if (!VCVARS) {
    console.log('\n  $ npx tauri build\n');
    execFileSync('npx', ['tauri', 'build'], { cwd: root, stdio: 'inherit', shell: process.platform === 'win32', env });
  } else {
    console.log(`  toolchain: ${VCVARS}`);
    execSync(`call "${VCVARS.replace(/\//g, "\\")}" >nul && npx tauri build`, {
      cwd: root,
      stdio: 'inherit',
      shell: 'cmd.exe',
      env,
    });
  }
}

if (wantLinux) {
  console.log('\n══ Linux ══');
  // Two runs, not one, and in this order. The AppImage bundler downloads
  // linuxdeploy and its plugins at bundle time and fails often enough to plan
  // around — and asked for all three at once, tauri stops at the first failure,
  // so a dead linuxdeploy took the .rpm with it and left a run with one package
  // out of three. The same lesson the release workflow already learned about
  // the .deb.
  run('node', ['scripts/build-linux.mjs', '--rpm']);
  collect(join(root, 'dist-linux'), ['.deb', '.rpm']);
  try {
    run('node', ['scripts/build-linux.mjs', '--appimage']);
    collect(join(root, 'dist-linux'), ['.AppImage']);
  } catch {
    console.log('\n  no AppImage this time; the .deb and .rpm above are unaffected.');
  }
}

// `.sig` too: it is the installer's signature, written beside it because
// createUpdaterArtifacts is on, and `npm run publish` needs it in release/ to
// write latest.json. Without it a release installs by hand and updates nothing.
if (wantWindows) collect(join(root, 'src-tauri', 'target', 'release', 'bundle'), ['.exe', '.msi', '.sig']);

console.log(`\n  release/  —  everything for ${version}, in one place:`);
show(RELEASE, ['.exe', '.msi', '.deb', '.rpm', '.AppImage']);
console.log('\n  gh release upload v' + version + ' release/*\n');

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
import { SIGNING_KEY, VCVARS, loadSigningKey } from './paths.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
// only this build's artifacts are collected; older ones stay where they are
const version = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8')).version;
const args = process.argv.slice(2);
const has = (f) => args.includes(f);
const only = has('--linux') || has('--windows');
const wantWindows = !only || has('--windows');
const wantLinux = !only || has('--linux');

// Every bundle is signed, because every bundle is one the app may be asked to
// install over itself. Without the key `tauri build` stops on its own; stopping
// here instead says which file is missing.
if (!loadSigningKey()) {
  console.error(
    `\n  No signing key at ${SIGNING_KEY}.\n\n` +
      '    npx tauri signer generate -w ' + SIGNING_KEY + '\n\n' +
      '    The public half goes in src-tauri/tauri.conf.json under plugins.updater.\n' +
      '    Changing it strands everyone already running a build signed by the old one.\n',
  );
  process.exit(1);
}

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

if (wantWindows) {
  console.log('== Windows ==');
  if (!VCVARS) {
    run('npx', ['tauri', 'build']);
  } else {
    console.log(`  toolchain: ${VCVARS}`);
    execSync(`call "${VCVARS.replace(/\//g, "\\")}" >nul && npx tauri build`, {
      cwd: root,
      stdio: 'inherit',
      shell: 'cmd.exe',
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
    collect(join(root, 'dist-linux'), ['.AppImage', '.AppImage.sig']);
  } catch {
    console.log('\n  no AppImage this time; the .deb and .rpm above are unaffected.');
  }
}

if (wantWindows) {
  collect(join(root, 'src-tauri', 'target', 'release', 'bundle'), ['.exe', '.msi', '.exe.sig']);
}

console.log(`\n  release/  —  everything for ${version}, in one place:`);
show(RELEASE, ['.exe', '.msi', '.deb', '.rpm', '.AppImage']);
console.log('\n  gh release upload v' + version + ' release/*\n');

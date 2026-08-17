// One command for a release: set the version, check it, commit, tag, push.
//
// Pushing a tag is what publishes — the workflow in .github/workflows/release.yml
// fires on `v*`, builds the Windows installer and the three Linux packages, and
// cuts the release notes out of the first section of CHANGELOG.md. So this walks
// the same path every time and refuses the mistakes that are easy to make by
// hand: a version the changelog does not mention, a tag that already exists, a
// branch that is not main.
//
//   npm run ship 0.9.9      # that version
//   npm run ship patch      # 0.9.8 -> 0.9.9
//   npm run ship minor      # 0.9.8 -> 0.10.0
//   npm run ship 0.9.9 "what changed"   # that as the commit message
//   npm run ship -- --dry   # say what would happen and stop
//   npm run ship -- -y      # do not ask
//
// Flags go after `--` so npm passes them on, and none of them start with
// `--no-`: npm reads those as its own configuration and never hands them over.
//
// Nothing is written or pushed until every check has passed.

import { execFileSync } from 'node:child_process';
import { createInterface } from 'node:readline/promises';
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const args = process.argv.slice(2);
const flag = (name) => args.includes(name);
const dry = flag('--dry');
const yes = flag('-y') || flag('--yes');
const skipTests = flag('--skip-tests');
// What the commit says, and the version to ship.
//
// The message is a plain second argument. `-m` is npm's own flag and npm keeps
// it: `npm run ship -- 0.9.9 -m "…"` reaches here as `0.9.9` alone, with the
// message gone. It is still read if it survives — run directly rather than
// through npm, it does — but the quoted form is the one that always works.
const flagAt = args.findIndex((a) => a === '-m' || a === '--message');
const noteAt = flagAt >= 0 ? flagAt + 1 : -1;
const loose = args.filter((a, i) => !a.startsWith('-') && i !== noteAt);
const wanted = loose[0];
const note = (flagAt >= 0 ? args[noteAt] : loose[1]) ?? null;

const die = (why) => {
  console.error(`\n  ${why}\n`);
  process.exit(1);
};

/** Run something and hand back its output; the command's own output is shown. */
function run(file, argv, { quiet = false } = {}) {
  // On Windows npm is a batch file, npm.cmd. execFile runs a program rather
  // than a shell and does not try extensions, so plain "npm" is ENOENT while
  // node.exe and git.exe are found — and naming npm.cmd outright does not help
  // either: since the fix for CVE-2024-27980 Node refuses to run a .cmd unless
  // it is asked through a shell, and answers EINVAL. So npm gets one, and only
  // npm: everything else is spawned directly, where no argument of ours can be
  // read as shell syntax.
  const shell = process.platform === 'win32' && file === 'npm';
  return execFileSync(file, argv, {
    cwd: root,
    encoding: 'utf8',
    shell,
    stdio: quiet ? ['ignore', 'pipe', 'pipe'] : 'inherit',
  });
}
const git = (...argv) => run('git', argv, { quiet: true }).trim();

// ── what version are we shipping ──────────────────────────────────────────────
const pkg = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8'));
const current = pkg.version;
const SEMVER = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/;

function bump(kind) {
  const [major, minor, patch] = current.split('.').map(Number);
  if (kind === 'major') return `${major + 1}.0.0`;
  if (kind === 'minor') return `${major}.${minor + 1}.0`;
  return `${major}.${minor}.${patch + 1}`;
}

if (!wanted) {
  die(`which version? \`npm run ship 0.9.9\`, or patch / minor / major (now on ${current})`);
}
const version = ['patch', 'minor', 'major'].includes(wanted) ? bump(wanted) : wanted;
if (!SEMVER.test(version)) die(`"${version}" is not a version — expected something like 0.9.9`);

// ── the checks ────────────────────────────────────────────────────────────────
const branch = git('rev-parse', '--abbrev-ref', 'HEAD');
if (branch !== 'main' && !flag('--any-branch')) {
  die(`on branch "${branch}", not main. Pass --any-branch if that is deliberate.`);
}

const tag = `v${version}`;
if (git('tag', '--list', tag)) die(`${tag} already exists. Releases are not rewritten; pick the next one.`);

// The workflow cuts the release notes from the first section of the changelog.
// A version it does not mention would ship a release described as another one.
const changelogPath = join(root, 'CHANGELOG.md');
const changelog = readFileSync(changelogPath, 'utf8');
const heading = changelog.split('\n').find((l) => l.startsWith('## '));
// A section written as "Unreleased" is this release, named before it had a
// number. Renaming it here is the difference between a release whose notes are
// its own and one titled "Unreleased" on GitHub — which is what --skip-notes
// would have published, since the workflow cuts the body from this section too.
const pending = heading?.trim() === '## Unreleased';
if (pending && !dry) {
  const today = new Date().toISOString().slice(0, 10);
  writeFileSync(changelogPath, changelog.replace('## Unreleased', `## ${version} — ${today}`));
  console.log(`  CHANGELOG.md: "Unreleased" is now ${version}`);
}
if (!pending && !heading?.includes(version) && !flag('--skip-notes')) {
  die(
    `CHANGELOG.md opens with "${heading?.trim() ?? 'nothing'}", which is not ${version}.\n` +
      `  The release notes are cut from that section. Write it first, or pass --skip-notes.`,
  );
}

// ── the plan ──────────────────────────────────────────────────────────────────
const dirty = git('status', '--porcelain');
console.log(`\n  ${current} → ${version}   on ${branch}\n`);
console.log(`    version   package.json, tauri.conf.json, Cargo.toml`);
// This is the one command that makes something public, and it runs
// `git add -A`. Naming the files is the difference between reviewing a commit
// and trusting one.
// The three version files are written by the step after this one, so they are
// not dirty yet and were being left out of the count — "no file(s)" for a
// commit that was about to carry three.
const files = dirty ? dirty.split('\n').filter(Boolean) : [];
console.log(`    commit    ${files.length + 3} file(s)  —  "${note ?? version}"`);
for (const line of files.slice(0, 20)) console.log(`              ${line}`);
if (files.length > 20) console.log(`              … and ${files.length - 20} more`);
console.log(`               M the three version files above`);
console.log(`    tag       ${tag}`);
console.log(`    push      origin ${branch}, then ${tag}  →  the tag is what publishes`);
console.log(`    notes     ${pending ? `"Unreleased" → ${version}` : heading?.trim()}\n`);

if (dry) {
  console.log('  --dry, so nothing was done.\n');
  process.exit(0);
}
if (!yes) {
  const ask = createInterface({ input: process.stdin, output: process.stdout });
  const said = await ask.question('  Ship it? [y/N] ');
  ask.close();
  if (!/^y(es)?$/i.test(said.trim())) die('nothing was done.');
}

// ── do it ─────────────────────────────────────────────────────────────────────
if (pending) {
  const today = new Date().toISOString().slice(0, 10);
  writeFileSync(changelogPath, changelog.replace('## Unreleased', `## ${version} — ${today}`));
  console.log(`\n▸ notes\n  CHANGELOG.md: "Unreleased" is now ${version} — ${today}`);
}
console.log('\n▸ version');
run('node', [join('scripts', 'set-version.mjs'), version]);

if (!skipTests) {
  console.log('\n▸ build and test');
  run('npm', ['run', 'build'], { quiet: false });
  // On Windows the linker needs the Visual Studio environment, the same one
  // tauri-dev.cmd loads; elsewhere cargo is enough on its own.
  if (process.platform === 'win32') {
    run('cmd', ['/c', join(root, 'test.cmd')]);
  } else {
    run('cargo', ['test', '--manifest-path', join('src-tauri', 'Cargo.toml')]);
  }
}

console.log('\n▸ commit and tag');
run('git', ['add', '-A']);
run('git', ['commit', '-m', note ?? version]);

// The tag goes on the commit just made, named outright rather than left to
// mean "wherever HEAD is" — and only once that commit has been read back and
// found to carry this version. The release workflow checks the tag out and
// builds whatever it finds, so a tag one commit adrift ships the wrong thing;
// v0.9.89 once landed on the 0.9.88 commit and the build failed on its own
// version check, which is a late and confusing place to hear about it.
const head = git('rev-parse', 'HEAD');
const committed = JSON.parse(git('show', `${head}:package.json`)).version;
if (committed !== version) {
  die(`${head.slice(0, 7)} says ${committed}, not ${version}. Not tagging it.`);
}
// Annotated, not lightweight. `git push --follow-tags` carries annotated tags
// and ignores the other kind without a word, so the tag stayed on this machine
// while the script announced it was away — and the workflow, which fires on
// the tag and nothing else, never ran. It is also pushed by name below rather
// than trusting that distinction twice.
run('git', ['tag', '-a', tag, '-m', note ?? version, head]);

console.log('\n▸ push');
run('git', ['push', 'origin', branch]);
run('git', ['push', 'origin', tag]);

const remote = git('remote', 'get-url', 'origin').replace(/\.git$/, '');
console.log(`\n  ${tag} is away. The build is at ${remote}/actions\n`);

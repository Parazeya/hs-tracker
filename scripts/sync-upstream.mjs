// `npm run sync` — take what upstream has done, and renumber this fork.
//
// This is a fork with its own features, so upstream is merged rather than
// tracked: `main` here is their history plus ours. Doing that by hand is six
// commands and one decision, and four of the six are always the same because
// the two projects always disagree about exactly the same files.
//
//   npm run sync              # fetch, merge, renumber, report
//   npm run sync -- --dry     # say what would happen and stop
//   npm run sync -- --check   # only say whether upstream has moved (exit 0/2)
//
// The numbering. A plain 1.0.6 is upstream's release; 1.0.6-tc.1 is this fork's
// first build on top of it. The suffix counts up while upstream stands still
// and resets when it moves, so this works it out rather than asking:
//
//   upstream 1.0.6, here 1.0.6-tc.1  ->  1.0.6-tc.2
//   upstream 1.0.7, here 1.0.6-tc.2  ->  1.0.7-tc.1
//
// It exists because the two projects collided: both reached 1.0.6 on the same
// day, `git fetch --tags` refused upstream's v1.0.6 because ours was in the
// way, and `gh` — asked to look at a release "v1.0.6" — answered from their
// repository instead of this one.
//
// What it will not do is resolve CHANGELOG.md. Upstream's entries and ours both
// belong in the file, under their own headings, and which is which is a
// judgement no script should make. It stops there and says so.

import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join, resolve } from 'node:path';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const args = process.argv.slice(2);
const has = (f) => args.includes(f);
const dry = has('--dry');

/// The fork's own suffix. One place, because it is in the tag, the installer's
/// filename, the changelog headings and the About page.
const MARK = 'tc';
const UPSTREAM = 'https://github.com/Parazeya/hs-tracker.git';

const run = (file, argv, opts = {}) =>
  execFileSync(file, argv, { cwd: root, encoding: 'utf8', ...opts });
const git = (...argv) => run('git', argv).trim();
const loud = (file, argv) => run(file, argv, { stdio: 'inherit' });

function die(why) {
  console.error(`\n  ${why}\n`);
  process.exit(1);
}

// ── is there an upstream to fetch from ───────────────────────────────────────
const remotes = git('remote').split('\n');
if (!remotes.includes('upstream')) {
  if (dry) die(`no "upstream" remote. It would be added: ${UPSTREAM}`);
  console.log(`  adding the upstream remote: ${UPSTREAM}`);
  git('remote', 'add', 'upstream', UPSTREAM);
}

// `--tags` would refuse upstream's tag whenever this fork has one of the same
// name, and say so in a line that reads like a failure. Their tags are not
// needed here — the merge follows a branch — so they are left alone.
console.log('\n▸ fetching upstream');
loud('git', ['fetch', 'upstream', 'main']);

const behind = Number(git('rev-list', '--count', 'HEAD..upstream/main'));
if (!behind) {
  console.log('\n  nothing new upstream.\n');
  process.exit(0);
}

const commits = git('log', '--oneline', 'HEAD..upstream/main').split('\n');
console.log(`\n  ${behind} commit(s) to take:\n`);
for (const line of commits) console.log(`    ${line}`);

if (has('--check')) process.exit(2);

// ── what this fork will be called afterwards ─────────────────────────────────
const here = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8')).version;
const theirs = JSON.parse(git('show', 'upstream/main:package.json')).version;

/// `1.0.6-tc.2` -> base 1.0.6, count 2. A version with no suffix has no count.
function split(v) {
  const at = v.indexOf(`-${MARK}.`);
  return at < 0
    ? { base: v, count: 0 }
    : { base: v.slice(0, at), count: Number(v.slice(at + MARK.length + 2)) || 0 };
}
const mine = split(here);
const next = mine.base === theirs ? `${theirs}-${MARK}.${mine.count + 1}` : `${theirs}-${MARK}.1`;

console.log(`\n  upstream ${theirs}   here ${here}   ->   ${next}\n`);

if (dry) {
  console.log('  --dry, so nothing was done.\n');
  process.exit(0);
}

// ── the merge ────────────────────────────────────────────────────────────────
const dirty = git('status', '--porcelain');
if (dirty) die(`the working tree is not clean, and a merge would bury it:\n\n${dirty}`);

const branch = git('rev-parse', '--abbrev-ref', 'HEAD');
console.log(`▸ merging upstream/main into ${branch}`);

let conflicted = false;
try {
  loud('git', ['merge', '--no-edit', 'upstream/main']);
} catch {
  conflicted = true;
}

// The four files the two projects always disagree about, and the only ones this
// resolves by itself. Three carry a version number that is ours by definition —
// set-version.mjs rewrites all three a moment later anyway — and Cargo.lock
// carries the same number a fourth time. Taking "ours" here loses nothing:
// upstream's changes to them are the version line and nothing else.
const MECHANICAL = ['package.json', 'src-tauri/tauri.conf.json', 'src-tauri/Cargo.toml', 'src-tauri/Cargo.lock'];

if (conflicted) {
  const stuck = git('diff', '--name-only', '--diff-filter=U').split('\n').filter(Boolean);
  const ours = stuck.filter((f) => MECHANICAL.includes(f));
  for (const f of ours) {
    console.log(`  ${f}: keeping ours — the version is this fork's to set`);
    git('checkout', '--ours', '--', f);
    git('add', '--', f);
  }
  const left = stuck.filter((f) => !MECHANICAL.includes(f));
  if (left.length) {
    console.log(`\n  ${left.length} file(s) need a person:\n`);
    for (const f of left) console.log(`    ${f}`);
    console.log(
      `\n  The merge is left open. Resolve those, then:\n` +
        `\n    node scripts/set-version.mjs ${next}` +
        `\n    git add -A && git commit` +
        `\n` +
        `\n  CHANGELOG.md is on that list nearly every time, and deliberately:` +
        `\n  upstream's entries go under a plain "## ${theirs}" heading and this` +
        `\n  fork's under "## ${next}" above it. Neither set is dropped.\n`,
    );
    process.exit(1);
  }
  console.log('\n  every conflict was mechanical; committing the merge');
  loud('git', ['commit', '--no-edit']);
}

// ── renumber ─────────────────────────────────────────────────────────────────
console.log('\n▸ version');
loud('node', [join('scripts', 'set-version.mjs'), next]);

console.log(
  `\n  upstream is in and this is ${next}. Nothing is committed past the merge:` +
    `\n  write the changelog heading, then` +
    `\n\n    git add -A && git commit -m "${next}"` +
    `\n    npm run ship ${next}\n`,
);

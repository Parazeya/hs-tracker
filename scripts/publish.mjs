// `npm run publish` — cut the release on GitHub from what was built here.
//
// The workflow used to do this: pushing a tag started it, it built the four
// packages on hosted runners and created the release as a side effect. That is
// also why `gh release upload` answered "release not found" — the release did
// not exist yet, because the run that would have made it was still going.
//
// Building here instead means nothing creates the release, so this does: notes
// cut from the top of CHANGELOG.md exactly as the workflow cut them, and the
// artifacts from release/ attached in one call.
//
//   npm run publish              # the version in package.json
//   npm run publish -- --draft   # create it unpublished, to look at first
//   npm run publish -- --dry     # say what would happen and stop
//
// The tag must already exist and be pushed — `npm run ship` does that.

import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join, resolve } from 'node:path';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const args = process.argv.slice(2);
const has = (f) => args.includes(f);
const dry = has('--dry');

const version = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8')).version;
const tag = `v${version}`;

const run = (file, argv, opts = {}) =>
  execFileSync(file, argv, { cwd: root, encoding: 'utf8', ...opts });

function die(why) {
  console.error(`\n  ${why}\n`);
  process.exit(1);
}

// ── the checks, before anything is written ───────────────────────────────────
try {
  run('gh', ['auth', 'status'], { stdio: 'ignore' });
} catch {
  die('gh is not signed in. Run: gh auth login');
}

const tags = run('git', ['tag', '-l', tag]).trim();
if (!tags) die(`no tag ${tag}. Run: npm run ship ${version}`);

const remote = run('git', ['ls-remote', '--tags', 'origin', tag]).trim();
if (!remote) die(`${tag} exists here but not on origin. Run: git push origin ${tag}`);

const dir = join(root, 'release');
if (!existsSync(dir)) die('nothing in release/. Run: npm run all');
const files = readdirSync(dir).filter((n) => n.includes(version));
if (!files.length) die(`release/ holds nothing for ${version}. Run: npm run all`);

// The four a full release carries. A missing one is worth saying out loud
// rather than discovering on the release page: the AppImage in particular is
// the one that fails on its own.
const WANTED = ['.exe', '.deb', '.rpm', '.AppImage'];
const missing = WANTED.filter((ext) => !files.some((n) => n.endsWith(ext)));

// The workflow cut the notes from the first section of the changelog; the same
// awk in three lines, so the release reads the same either way.
const changelog = readFileSync(join(root, 'CHANGELOG.md'), 'utf8').split(/\r?\n/);
const first = changelog.findIndex((l) => l.startsWith('## '));
if (first < 0) die('CHANGELOG.md has no section to cut notes from');
let last = changelog.findIndex((l, i) => i > first && l.startsWith('## '));
if (last < 0) last = changelog.length;
const notes = changelog.slice(first, last).join('\n').trim();
if (!notes.includes(version)) {
  die(`CHANGELOG.md opens with "${changelog[first].trim()}", which is not ${version}`);
}

console.log(`\n  ${tag}\n`);
for (const name of files.sort()) console.log(`    ${name}`);
if (missing.length) console.log(`\n    missing: ${missing.join(', ')}`);
console.log(`\n    notes    ${changelog[first].trim()}`);
console.log(`    release  ${has('--draft') ? 'draft' : 'published'}\n`);

if (dry) {
  console.log('  --dry, so nothing was done.\n');
  process.exit(0);
}

const notesPath = join(root, 'RELEASE_NOTES.md');
writeFileSync(notesPath, notes + '\n');

// `gh release create` refuses to touch one that already exists, which is the
// right refusal — a second run should add what is missing, not replace what is
// there.
const exists = (() => {
  try {
    run('gh', ['release', 'view', tag], { stdio: 'ignore' });
    return true;
  } catch {
    return false;
  }
})();

const paths = files.map((n) => join(dir, n));
if (exists) {
  console.log('  the release is already there; attaching the files\n');
  run('gh', ['release', 'upload', tag, ...paths, '--clobber'], { stdio: 'inherit' });
} else {
  run(
    'gh',
    [
      'release',
      'create',
      tag,
      ...paths,
      '--title',
      version,
      '--notes-file',
      notesPath,
      ...(has('--draft') ? ['--draft'] : []),
    ],
    { stdio: 'inherit' },
  );
}

console.log(`\n  ${run('gh', ['release', 'view', tag, '--json', 'url', '-q', '.url']).trim()}\n`);

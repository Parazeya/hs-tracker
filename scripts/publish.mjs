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
// It also writes latest.json — the manifest the app's updater reads: the new
// version, this release's changelog section, and for each platform the artifact
// to fetch and the signature to check it against.
//
//   npm run publish              # the version in package.json
//   npm run publish -- --draft   # create it unpublished, to look at first
//   npm run publish -- --dry     # say what would happen and stop
//   npm run publish -- --replace # overwrite assets already on the release
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

// Both the tag object and the commit under it, which is why the pattern ends
// in a star: `ls-remote --tags origin v1.0.0` matches the ref and not the
// `v1.0.0^{}` line beside it, so an annotated tag comes back as the object's
// own hash and nothing that could be compared to a commit.
const remote = run('git', ['ls-remote', '--tags', 'origin', tag + '*']).trim();
if (!remote) die(`${tag} exists here but not on origin. Run: git push origin ${tag}`);

// What is being published has to be what was tagged.
//
// Nothing here looked. The version is read from package.json, the artifacts are
// matched by the version in their filenames, and the notes come from the
// changelog — none of which says the binaries were built from the commit the tag
// points at. Fix a bug, rebuild, forget to bump, and every check still passes:
// the four assets on a published release are silently replaced with ones nobody
// downloading yesterday's has.
const dirty = run('git', ['status', '--porcelain']).trim();
if (dirty) {
  die(
    `the working tree is not clean, so the artifacts in release/ match no commit:\n\n${dirty}\n\n` +
      '  commit it, or check it out again, and rebuild.',
  );
}
const head = run('git', ['rev-parse', 'HEAD']).trim();
const tagged = run('git', ['rev-parse', `${tag}^{commit}`]).trim();
if (head !== tagged) {
  die(
    `HEAD is ${head.slice(0, 8)} and ${tag} is ${tagged.slice(0, 8)}.\n\n` +
      `  The artifacts were built from HEAD, so publishing them under ${tag} would ship\n` +
      '  something that tag does not describe. Check the tag out and rebuild.',
  );
}

// And the tag on origin has to mean that commit too.
//
// The check above compares HEAD to the LOCAL tag, which agrees with itself. A
// tag that was moved here and refused by `git push` as a non-fast-forward —
// one line, easy to walk past — leaves origin pointing at the old commit, and
// the release is then cut there: notes and artifacts from a commit nobody
// downloading it would ever get.
//
// An annotated tag answers `ls-remote` on two lines and the one ending `^{}`
// carries the commit; a lightweight tag has only the first, and that one is
// the commit.
const peeled = remote.split('\n').find((l) => l.endsWith(tag + '^{}'));
const [remoteSha] = (peeled ?? remote).split(/\s/);
if (remoteSha !== tagged) {
  die(
    `${tag} is ${tagged.slice(0, 8)} here and ${remoteSha.slice(0, 8)} on origin.\n\n` +
      `  The artifacts were built from this one. Push it: git push --force origin ${tag}`,
  );
}

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

// ── the manifest the updater reads ───────────────────────────────────────────
//
// One entry per platform that can install over itself: the Windows installer
// and the AppImage. A .deb or .rpm is the package manager's to replace, so
// neither is listed and neither machine is offered an update it cannot apply.
//
// The signature is the .sig the bundler wrote beside each artifact, carried
// inline; the app refuses anything the private half of the key did not sign.
// The URL has to be the one GitHub serves from, and GitHub writes a space in an
// asset name as a dot.
const TARGETS = [
  ['windows-x86_64', (n) => n.endsWith('-setup.exe')],
  ['linux-x86_64', (n) => n.endsWith('.AppImage')],
];
const slug = run('gh', ['repo', 'view', '--json', 'nameWithOwner', '-q', '.nameWithOwner']).trim();
const platforms = {};
const unsigned = [];
for (const [target, wanted] of TARGETS) {
  const name = files.find((n) => wanted(n));
  if (!name) continue;
  const sig = join(dir, name + '.sig');
  if (!existsSync(sig)) {
    unsigned.push(name);
    continue;
  }
  platforms[target] = {
    signature: readFileSync(sig, 'utf8').trim(),
    url: `https://github.com/${slug}/releases/download/${tag}/${name.replace(/ /g, '.')}`,
  };
}
const manifest = { version, notes, pub_date: new Date().toISOString(), platforms };
const manifestPath = join(dir, 'latest.json');

console.log(`\n  ${tag}\n`);
for (const name of files.sort()) console.log(`    ${name}`);
if (missing.length) console.log(`\n    missing: ${missing.join(', ')}`);
console.log(`\n    updates  ${Object.keys(platforms).join(', ') || 'nothing was signed'}`);
if (unsigned.length) {
  // Not fatal: the release is still worth cutting. But everyone on that
  // platform stays where they are until a signed build replaces it.
  console.log(`    unsigned ${unsigned.join(', ')}`);
}
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
// there. `--clobber` does replace, so it is behind a flag: a rerun that fills in
// a package the first pass missed is routine, and quietly overwriting an asset
// somebody has already downloaded is not.
const exists = (() => {
  try {
    run('gh', ['release', 'view', tag], { stdio: 'ignore' });
    return true;
  } catch {
    return false;
  }
})();

writeFileSync(manifestPath, JSON.stringify(manifest, null, 2) + '\n');

// The signatures are already inside latest.json; on the release page they would
// be four more files nobody downloads.
const paths = files.filter((n) => !n.endsWith('.sig')).map((n) => join(dir, n));
if (exists) {
  console.log(`  the release is already there; ${has('--replace') ? 'replacing' : 'adding what is missing'}\n`);
  const upload = ['release', 'upload', tag, ...paths];
  if (has('--replace')) upload.push('--clobber');
  run('gh', upload, { stdio: 'inherit' });
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

// Always replaced, and uploaded after the artifacts it points at: it is one
// file per release rather than a new one each time, and a rerun that fills in a
// missing package has to leave the manifest describing what is actually there.
if (Object.keys(platforms).length) {
  run('gh', ['release', 'upload', tag, manifestPath, '--clobber'], { stdio: 'inherit' });
}

console.log(`\n  ${run('gh', ['release', 'view', tag, '--json', 'url', '-q', '.url']).trim()}\n`);

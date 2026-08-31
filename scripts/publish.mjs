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
const all = readdirSync(dir).filter((n) => n.includes(version));
if (!all.length) die(`release/ holds nothing for ${version}. Run: npm run all`);

// The signature is not an asset. It is one line of base64 that belongs inside
// latest.json, where the app reads it; on the release page it would be a file
// nobody has a use for next to the installer it describes.
const sig = all.find((n) => n.endsWith('.sig'));
const files = all.filter((n) => !n.endsWith('.sig'));
const installer = files.find((n) => n.endsWith('-setup.exe')) ?? files.find((n) => n.endsWith('.exe'));

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
console.log(`    release  ${has('--draft') ? 'draft' : 'published'}`);
console.log(
  `    updates  ${
    sig && installer
      ? `latest.json, signing ${installer}`
      : 'NO latest.json — nobody running the app will be offered this'
  }\n`,
);
if (!sig && installer) {
  console.log(
    '  There is an installer but no .sig beside it, so this release cannot be\n' +
      '  offered as an update: the app installs only what its own key signed.\n' +
      '  The key is missing or unset — see DEVELOPING.md, "Updates" — and\n' +
      '  `npm run all` says which when it runs.\n',
  );
}

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

const paths = files.map((n) => join(dir, n));
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

// ── the update manifest ──────────────────────────────────────────────────────
//
// Written after the upload rather than before it, because it has to name the
// installer by the URL GitHub actually gave it — and GitHub renames an asset
// on the way in, turning every space in "HS Tracker_1.0.6_x64-setup.exe" into
// a dot. Guessing that rule is how a manifest ends up pointing at a 404 that
// only shows up as "update failed" on somebody else's machine a week later.
// So it is read back from the release.
if (sig && installer) {
  const assets = JSON.parse(
    run('gh', ['release', 'view', tag, '--json', 'assets']),
  ).assets;
  // found by suffix rather than by the local filename, which is the one thing
  // GitHub is known to have changed
  const asset = assets.find((a) => a.name.endsWith('-setup.exe')) ?? assets.find((a) => a.name.endsWith('.exe'));
  if (!asset) {
    die(`${installer} was uploaded but the release does not list it. Nothing was written to latest.json.`);
  }
  // Built from the name GitHub settled on rather than read off the asset
  // record: `gh` reports two URLs per asset and only one of them hands back
  // the file itself to an unauthenticated GET, which is all the updater does.
  const repo = run('gh', ['repo', 'view', '--json', 'nameWithOwner', '-q', '.nameWithOwner']).trim();
  const url = `https://github.com/${repo}/releases/download/${tag}/${asset.name}`;
  const manifest = {
    version,
    notes,
    // The plugin parses this; an invalid one is a check that fails silently
    // on every machine at once.
    pub_date: new Date().toISOString().replace(/\.\d+Z$/, 'Z'),
    platforms: {
      'windows-x86_64': {
        signature: readFileSync(join(dir, sig), 'utf8').trim(),
        url,
      },
    },
  };
  const manifestPath = join(dir, 'latest.json');
  writeFileSync(manifestPath, JSON.stringify(manifest, null, 2) + '\n');
  // Always --clobber: the manifest is derived from what is already on the
  // release, so a second run rewrites it rather than adding a second one, and
  // "asset already exists" would stop the release half-published.
  run('gh', ['release', 'upload', tag, manifestPath, '--clobber'], { stdio: 'inherit' });
  console.log(`\n  latest.json → ${asset.name}`);
}

console.log(`\n  ${run('gh', ['release', 'view', tag, '--json', 'url', '-q', '.url']).trim()}\n`);

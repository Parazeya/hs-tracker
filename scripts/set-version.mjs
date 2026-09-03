// package.json is the single source of the version. This copies it into the two
// files that also carry one: src-tauri/tauri.conf.json (stamps the binary and
// names the installer) and src-tauri/Cargo.toml (what the crate reports).
//
// Tauri can point at a package.json itself, but it resolves that path against
// the current working directory rather than the config file, so it depends on
// where the build was started from. Copying is boring and always right.
//
//   npm run ver           # sync the other files to package.json
//   npm run ver 1.1.0     # set the version everywhere
//
// `npm run release` runs this first, so the three cannot drift apart.
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const SEMVER = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/;

// The README lists the files a release carries, and a file's name carries the
// version, so the list is written here rather than by hand. The names are the
// bundlers' own — Tauri's NSIS installer, its .deb and its AppImage, and the
// .rpm, whose name puts the packaging revision after the version.
//
// No count beside each file. Shields can only count an asset within one
// release, so those four numbers were the current version's alone and read as
// nought for the minutes between the tag going up and the release being cut.
// The one badge above the table counts every download of every release, which
// is the figure worth showing.
// Windows alone, because that is what this fork releases. Upstream's table
// lists four and upstream uploads four; a row here is a link to a file on this
// fork's own release, so the three Linux ones would have been three 404s under
// every tag. The Linux packages build \u2014 `npm run deb` does it in a container \u2014
// but they are not published here, and the row comes back the day they are.
const ASSETS = [
  ['Windows', (v) => `HS.Tracker_${v}_x64-setup.exe`],
];

/** The table between the `downloads` markers, for one version. */
function downloads(version, eol) {
  const rows = ASSETS.map(([what, named]) => {
    const file = named(version);
    return [
      '  <tr>',
      `    <td><b>${what}</b></td>`,
      `    <td><a href="../../releases/download/v${version}/${file}">${file}</a></td>`,
      '  </tr>',
    ].join(eol);
  });
  return ['<table align="center">', ...rows, '</table>'].join(eol);
}

const pkgPath = join(root, 'package.json');
const current = JSON.parse(readFileSync(pkgPath, 'utf8')).version;

const wanted = process.argv[2];
if (wanted && !SEMVER.test(wanted)) {
  console.error(`"${wanted}" is not a version — expected something like 1.1.0`);
  process.exit(1);
}
const version = wanted ?? current;
if (!SEMVER.test(version)) {
  console.error(`package.json holds "${version}", which is not a semver version`);
  process.exit(1);
}

/** replace the first match, and report whether anything actually changed */
function patch(path, re, next) {
  const before = readFileSync(path, 'utf8');
  if (!re.test(before)) {
    console.error(`could not find the version line in ${path} — has the file changed shape?`);
    process.exit(1);
  }
  const after = before.replace(re, next);
  if (after !== before) writeFileSync(path, after);
  return after !== before;
}

const touched = [];
if (version !== current) {
  patch(pkgPath, /("version"\s*:\s*")[^"]+(")/, `$1${version}$2`);
  touched.push('package.json');
}
// the top-level "version" sits right under productName — the first match, and
// the only key in the file shaped like this
if (patch(join(root, 'src-tauri', 'tauri.conf.json'), /("version"\s*:\s*")[^"]+(")/, `$1${version}$2`))
  touched.push('src-tauri/tauri.conf.json');
// [package] version — the first `version = "…"` at the start of a line
if (patch(join(root, 'src-tauri', 'Cargo.toml'), /(^version\s*=\s*")[^"]+(")/m, `$1${version}$2`))
  touched.push('src-tauri/Cargo.toml');
// the download table, names, links and all
if (
  patch(
    join(root, 'README.md'),
    /(<!-- downloads -->\r?\n)[\s\S]*?(\r?\n<!-- \/downloads -->)/,
    // the file is checked out with CRLF on Windows, and a block written with
    // bare newlines inside one reads as a single line to anything that cares
    (_, open, close) =>
      open + downloads(version, open.includes('\r') ? '\r\n' : '\n') + close,
  )
)
  touched.push('README.md');

console.log(
  touched.length
    ? `v${version} — updated ${touched.join(', ')}`
    : `v${version} — everything already in sync`,
);

// `npm run items` — rebuild the item tables from the game itself.
//
// Until today this started with a download: tools/fetch_items.py asked a
// datamining site for rarities, grades and identities, and a season could not
// be tracked until someone else had datamined it. All of that now comes out of
// Hero_Siege.exe and translationsItem.csv, read by the extractor next door, so
// the only thing standing between a game patch and correct tables is this
// command.
//
//   npm run items                 # read the game, regenerate the tables
//   npm run items -- --dry        # write the item file, do not touch the repo
//
// The extractor lives outside this repository because it is a tool for reading
// the game, not part of the tracker. Point HS_EXTRACTOR at it if it is not
// beside this checkout.

import { execFileSync } from 'node:child_process';
import { copyFileSync, existsSync, statSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join, resolve } from 'node:path';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const dry = process.argv.includes('--dry');

import { EXTRACTOR, GAME } from './paths.mjs';

const home = EXTRACTOR;
if (!existsSync(join(home, 'target', 'release', 'hse-extractor.exe'))) {
  console.error(
    `\n  No built extractor at\n    ${home}\n` +
      '  Set HS_EXTRACTOR in .env (see .env.example), and build it there first.\n'
  );
  process.exit(1);
}

const exe = join(home, 'target', 'release', 'hse-extractor.exe');
const staged = join(home, 'out', 'items.json');
const target = join(root, 'tools', 'data', 'helper', 'items.json');

// Run it from its own folder: it looks for data/ beside itself.
console.log(`\n  $ hse-extractor --update  (in ${home})\n`);
execFileSync(exe, ['--update', staged], { cwd: home, stdio: 'inherit' });

if (!existsSync(staged)) {
  console.error('\n  The extractor exited without writing the file.\n');
  process.exit(1);
}

// A file this much smaller than the one it replaces is a decode that went wrong
// rather than a game that shed half its items, and it is not worth finding that
// out from a table of blanks.
if (existsSync(target)) {
  const [was, now] = [statSync(target).size, statSync(staged).size];
  if (now < was * 0.5) {
    console.error(
      `\n  Refusing to install: ${(now / 1024) | 0} KB against the previous ${(was / 1024) | 0} KB.` +
        `\n  The file is at ${staged} if you want to look at it.\n`
    );
    process.exit(1);
  }
}

if (dry) {
  console.log(`\n  --dry, so nothing was installed. The file is at\n    ${staged}\n`);
  process.exit(0);
}

copyFileSync(staged, target);
console.log(`\n  installed ${target}\n`);

// The generator reads the game too — for the translated names and the stat
// labels — and it looks for it in the environment, which is not where this
// checkout keeps that path. Told nothing it looked at a folder that does not
// exist on this machine and fell back to the datamined names without failing,
// so a season of renamed items would have gone in under last season's names.
console.log('  $ python tools/gen_items.py\n');
execFileSync('python', [join(root, 'tools', 'gen_items.py')], {
  cwd: root,
  stdio: 'inherit',
  env: { ...process.env, HERO_SIEGE_BIN: GAME },
});

// `npm run deb` — the Linux packages, built in a container on this machine.
//
// Nothing about the host matters except that Docker is running: the toolchain,
// the WebKitGTK headers and the glibc the binary is linked against all come
// from the image, so the same command gives the same package on any machine.
//
// The cargo registry and the target directory live in named volumes, so the
// first build is slow and every one after it is not.
//
//   npm run deb                 # a .deb
//   npm run deb -- --appimage   # a .deb and an AppImage
//   npm run deb -- --rebuild    # rebuild the image first
//   npm run deb -- --clean      # throw the build caches away

import { execFileSync } from 'node:child_process';
import { mkdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const args = process.argv.slice(2);
const has = (f) => args.includes(f);

const IMAGE = 'hs-tracker-linux-build';
const OUT = join(root, 'dist-linux');
const win = process.platform === 'win32';

/** Docker is a real executable everywhere; only npm needs a shell on Windows. */
function docker(argv, { quiet = false } = {}) {
  return execFileSync('docker', argv, {
    cwd: root,
    encoding: 'utf8',
    stdio: quiet ? ['ignore', 'pipe', 'pipe'] : 'inherit',
  });
}

try {
  docker(['version', '--format', '{{.Server.Version}}'], { quiet: true });
} catch {
  console.error(
    '\n  Docker is not answering.' +
      (win ? ' Start Docker Desktop and try again.' : ' Is the daemon running?') +
      '\n',
  );
  process.exit(1);
}

if (has('--clean')) {
  for (const v of ['hs-cargo', 'hs-target']) {
    try {
      docker(['volume', 'rm', v], { quiet: true });
      console.log(`  removed volume ${v}`);
    } catch {}
  }
  if (!has('--rebuild')) process.exit(0);
}

const built = (() => {
  try {
    return docker(['image', 'inspect', IMAGE, '--format', '{{.Id}}'], { quiet: true }).trim();
  } catch {
    return '';
  }
})();

if (!built || has('--rebuild')) {
  console.log('\n▸ building the image (once; a few minutes)\n');
  docker(['build', '-t', IMAGE, join('docker')]);
}

mkdirSync(OUT, { recursive: true });

const bundles = ['deb'];
if (has('--appimage')) bundles.push('appimage');
if (has('--rpm')) {
  console.error('\n  The .rpm has to be built on Fedora; see DEVELOPING.md.\n');
  process.exit(1);
}

console.log(`\n▸ building: ${bundles.join(', ')}\n`);
docker([
  'run', '--rm',
  '-v', `${root}:/src:ro`,
  '-v', `${OUT}:/out`,
  '-v', 'hs-cargo:/cargo',
  '-v', 'hs-target:/target',
  '-e', `BUNDLES=${bundles.join(',')}`,
  IMAGE,
]);

console.log(`\n  packages are in ${OUT}\n`);

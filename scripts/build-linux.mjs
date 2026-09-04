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
//   npm run deb -- --rpm        # and an .rpm, from a Fedora image of its own
//   npm run deb -- --rebuild    # rebuild the images first
//   npm run deb -- --clean      # throw the build caches away
//
// The .rpm is built in a second container and not beside the other two. A
// binary linked on Ubuntu wants `libpcap.so.0.8`; Fedora has `libpcap.so.1`,
// so the package installed and then would not start. Three releases went out
// that way. See docker/Dockerfile.fedora.

import { execFileSync } from 'node:child_process';
import { mkdirSync, readdirSync, statSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { loadSigningKey } from './paths.mjs';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const args = process.argv.slice(2);
const has = (f) => args.includes(f);

// Two images, because two package formats mean two sets of shared libraries.
// See docker/Dockerfile.fedora for what an .rpm built on Ubuntu does.
const IMAGES = {
  deb: { tag: 'hs-tracker-linux-build', file: 'Dockerfile', cargo: 'hs-cargo', target: 'hs-target' },
  rpm: {
    tag: 'hs-tracker-fedora-build',
    file: 'Dockerfile.fedora',
    // Its own caches: a target directory holds objects linked against this
    // image's libraries, and sharing one between the two would hand each
    // build the other's.
    cargo: 'hs-cargo-fedora',
    target: 'hs-target-fedora',
  },
};
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
  for (const v of Object.values(IMAGES).flatMap((i) => [i.cargo, i.target])) {
    try {
      docker(['volume', 'rm', v], { quiet: true });
      console.log(`  removed volume ${v}`);
    } catch {}
  }
  if (!has('--rebuild')) process.exit(0);
}

// An edit to docker/ that nobody rebuilt is invisible from here: the run reuses
// the old image and then fails on precisely what that edit was adding. That is
// how an APPIMAGE_EXTRACT_AND_RUN and a patchelf sat in the Dockerfile, in no
// image, while AppImage bundling kept dying. Compare, do not trust.
function ensureImage(image) {
  const builtAt = (() => {
    try {
      return Date.parse(
        docker(['image', 'inspect', image.tag, '--format', '{{.Created}}'], { quiet: true }).trim(),
      );
    } catch {
      return 0;
    }
  })();
  const stale =
    builtAt > 0 &&
    readdirSync(join(root, 'docker')).some(
      (f) => statSync(join(root, 'docker', f)).mtimeMs > builtAt,
    );
  if (!builtAt || stale || has('--rebuild')) {
    console.log(
      `\n▸ building ${image.tag} (${stale ? 'docker/ has changed' : 'once'}; a few minutes)\n`,
    );
    docker(['build', '-t', image.tag, '-f', join('docker', image.file), join('docker')]);
  }
}

mkdirSync(OUT, { recursive: true });

// An unsigned AppImage still installs by hand; it just cannot be offered as an
// update. `npm run all` insists on the key, this one only says so.
if (!loadSigningKey()) {
  console.log('\n  no signing key, so the AppImage will carry no .sig\n');
}

function bundle(image, bundles) {
  ensureImage(image);
  console.log(`\n▸ ${image.tag}: ${bundles.join(', ')}\n`);
  docker([
    'run', '--rm',
    '-v', `${root}:/src:ro`,
    '-v', `${OUT}:/out`,
    '-v', `${image.cargo}:/cargo`,
    '-v', `${image.target}:/target`,
    '-e', `BUNDLES=${bundles.join(',')}`,
    // By name, not by value: the key reaches the container through the
    // environment instead of sitting in a command line every process on the
    // host can read.
    '-e', 'TAURI_SIGNING_PRIVATE_KEY',
    '-e', 'TAURI_SIGNING_PRIVATE_KEY_PASSWORD',
    image.tag,
  ]);
}

// The .deb and the AppImage share a compile, so they share a run.
const debian = ['deb'];
if (has('--appimage')) debian.push('appimage');
bundle(IMAGES.deb, debian);

// The .rpm is a second compile in a second image, and that is the whole point
// of it: the bundler would happily write one from the Ubuntu build, and the
// binary inside would ask Fedora for a libpcap Fedora has never shipped.
if (has('--rpm')) bundle(IMAGES.rpm, ['rpm']);

console.log(`\n  packages are in ${OUT}\n`);

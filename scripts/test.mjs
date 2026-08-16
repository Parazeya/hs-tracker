// `npm test`, on whichever machine this is.
//
// On Windows the linker needs the Visual Studio environment, which test.cmd
// loads; elsewhere cargo is enough on its own. This existed only as a batch
// file, so on Linux the documented way to run the tests was a syntax error.

import { execFileSync } from 'node:child_process';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const args = process.argv.slice(2);
const manifest = join('src-tauri', 'Cargo.toml');

const [file, argv] =
  process.platform === 'win32'
    ? ['cmd', ['/c', join(root, 'test.cmd'), ...args]]
    : ['cargo', ['test', '--manifest-path', manifest, ...args]];

execFileSync(file, argv, { cwd: root, stdio: 'inherit' });

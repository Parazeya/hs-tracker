// `npm start`, on whichever machine this is.
//
// The batch files it used to call are Windows only — they exist because the
// MSVC linker needs the Visual Studio environment loaded first, which is not a
// thing anywhere else. On Linux and macOS the Tauri CLI is enough on its own,
// and the same command that worked on Windows now works there too.
//
// Capture rights are worth a word on Linux: cap_net_raw lives on the inode, so
// every rebuild drops it and a dev build reads nothing until it is granted
// again. Rather than sudo behind the user's back, it says so.

import { execFileSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const args = process.argv.slice(2);
const win = process.platform === 'win32';

/** npm is a batch file on Windows, and Node will not run one without a shell. */
function run(file, argv) {
  execFileSync(file, argv, { cwd: root, stdio: 'inherit', shell: win && file === 'npm' });
}

if (win) {
  run('cmd', ['/c', join(root, 'tauri-dev.cmd'), ...args]);
} else {
  const binary = join(root, 'src-tauri', 'target', 'debug', 'hs-tracker');
  if (existsSync(binary)) {
    try {
      const caps = execFileSync('getcap', [binary], { encoding: 'utf8' }).trim();
      if (!caps.includes('cap_net_raw')) throw new Error('none');
      console.log(`  ${caps}`);
    } catch {
      console.log(
        `\n  This build has no capture rights, so it will count nothing.\n` +
          `  Grant them — every relink drops them again:\n\n` +
          `    sudo setcap cap_net_raw=ep ${binary}\n`,
      );
    }
  }
  run('npx', ['tauri', 'dev', ...args]);
}

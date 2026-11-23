import { execFileSync } from 'node:child_process';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { PYTHON } from './paths.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
execFileSync(process.execPath, ['--test', 'scripts/item-bundle.test.mjs', 'scripts/collection-catalogue.test.mjs'], { cwd: root, stdio: 'inherit' });
execFileSync(PYTHON, ['-m', 'unittest', 'discover', '-s', 'tools', '-p', 'test*.py'], { cwd: root, stdio: 'inherit' });

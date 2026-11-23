// Export one game build, generate into staging, then promote the complete set.
// A failed export cannot reuse an earlier output. --dry validates and generates
// without replacing installed data or source files.
import { execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { BUNDLE, EXTRACTOR, EXTRACTOR_BIN, GAME, PYTHON, REPO } from './paths.mjs';
import {
  generatedEntries, publishItemUpdate, recoverItemUpdate,
  stageBundle, validateBundle, withItemUpdateLock,
} from './item-bundle.mjs';

export async function updateItemTables({
  root = REPO, game = GAME, extractor = EXTRACTOR_BIN,
  extractorHome = EXTRACTOR, bundle = BUNDLE,
  dry = false, run = execFileSync,
} = {}) {
  return withItemUpdateLock(root, async () => {
    if (recoverItemUpdate(root)) console.log('Recovered the previous interrupted item update.');
    const data = join(root, 'tools', 'data');
    mkdirSync(data, { recursive: true });
    const staging = mkdtempSync(join(data, '.item-stage-'));
    const exported = join(staging, 'export');
    const prepared = join(staging, 'helper');
    const generated = join(staging, 'generated');
    mkdirSync(generated, { recursive: true });
    try {
      let incoming = bundle;
      if (!incoming) {
        if (!existsSync(extractor)) throw new Error(`No built Extractor at ${extractor}. Set HS_EXTRACTOR_BIN or build ${extractorHome}.`);
        if (!game || !existsSync(join(game, 'data.win'))) throw new Error('No game installation found. Set HERO_SIEGE_BIN or HS_DATA_BUNDLE.');
        console.log(`Extracting ${game}`);
        run(extractor, ['--export', exported], {
          cwd: extractorHome, stdio: 'inherit',
          env: { ...process.env, HERO_SIEGE: game, HERO_SIEGE_BIN: game },
        });
        incoming = exported;
      }
      const checkGame = game && existsSync(join(game, 'data.win')) ? game : '';
      const verified = await validateBundle(incoming, { game: checkGame });
      stageBundle(verified, prepared);
      const staged = await validateBundle(prepared, { game: checkGame });
      console.log(`Validated build ${staged.manifest.game.build_id ?? staged.manifest.game.fingerprint.slice(0, 12)}.`);
      const partial = Object.entries(staged.manifest.datasets).filter(([, d]) => d.status === 'partial').map(([name]) => name);
      if (partial.length) console.log(`Partial extraction: ${partial.join(', ')}. The manifest retains the unresolved fields.`);
      run(PYTHON, [join(root, 'tools', 'gen_items.py')], {
        cwd: root, stdio: 'inherit',
        env: {
          ...process.env, HERO_SIEGE: game || '', HERO_SIEGE_BIN: game || '',
          HS_DATA_BUNDLE: prepared, HS_ITEMS_INPUT: staged.files.get('items'),
          HS_ITEMS_OUT_JS: join(generated, 'items.js'),
          HS_ITEMS_OUT_RS: join(generated, 'items.rs'),
          HS_ITEMS_LANG_OUT: join(generated, 'lang'),
        },
      });
      const outputs = generatedEntries(generated);
      for (const output of outputs) {
        if (!existsSync(output.source) || !readFileSync(output.source).length) throw new Error(`Generator did not write ${output.source}.`);
      }
      // Steam or the input package may have changed during generation.
      await validateBundle(prepared, { game: checkGame });
      if (dry) {
        console.log(`Dry run complete. Validated outputs are at ${staging}`);
        return { staging, manifest: staged.manifest, installed: false };
      }
      publishItemUpdate(root, [{ source: prepared, target: 'tools/data/helper' }, ...outputs]);
      console.log(`Installed items, talents, manifest and generated tables for ${staged.manifest.game.fingerprint.slice(0, 12)}.`);
      rmSync(staging, { recursive: true, force: true });
      return { manifest: staged.manifest, installed: true };
    } catch (error) {
      // Retained staging is evidence, never an input to the next invocation.
      throw new Error(`${error.message}\nStaging retained at ${staging}`, { cause: error });
    }
  });
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const unknown = process.argv.slice(2).filter((arg) => arg !== '--dry');
  if (unknown.length) {
    console.error(`Unknown arguments: ${unknown.join(' ')}. Use --dry or set HS_DATA_BUNDLE.`);
    process.exitCode = 1;
  } else {
    updateItemTables({ dry: process.argv.includes('--dry') }).catch((error) => {
      console.error(error.message);
      process.exitCode = 1;
    });
  }
}

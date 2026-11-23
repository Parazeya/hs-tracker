import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, isAbsolute, join, relative } from 'node:path';
import test from 'node:test';
import { publishItemUpdate, recoverItemUpdate, validateBundle } from './item-bundle.mjs';
import { updateItemTables } from './update-items.mjs';
import { findGame, steamLibraries } from './paths.mjs';

const hash = (value) => createHash('sha256').update(value).digest('hex');
function temporary(t) {
  const root = mkdtempSync(join(tmpdir(), 'hs-item-pipeline-'));
  t.after(() => {
    const rel = relative(tmpdir(), root);
    assert.ok(rel && !rel.startsWith('..') && !isAbsolute(rel));
    rmSync(root, { recursive: true, force: true });
  });
  return root;
}
function put(path, content) { mkdirSync(dirname(path), { recursive: true }); writeFileSync(path, content); }
function fixture(directory, { game = '' } = {}) {
  const sources = {
    items: [{ metadata: { tkey: 'helmet_test', record_id: 'helmet_test:0', ID: [1, 0, 0], name: 'Test', rarity: 'Satanic', tier: 'S', fromGame: ['ID', 'name', 'rarity', 'tier'] } }],
    talents: [{ id: 2, key: 'testTalent' }],
    translationsItem: '[Items]|en\nhelmet_test|Test\n',
  };
  const datasets = {};
  for (const [name, rows] of Object.entries(sources)) {
    const file = `generations/fixture/${name === 'translationsItem' ? 'source/translationsItem.csv' : `${name}.json`}`;
    const bytes = typeof rows === 'string' ? rows : JSON.stringify(rows);
    put(join(directory, file), bytes);
    datasets[name] = { file, sha256: hash(bytes), records: Array.isArray(rows) ? rows.length : 1, status: 'complete' };
  }
  if (game) { put(join(game, 'data.win'), 'game bytes'); put(join(game, 'Hero_Siege.exe'), 'game executable'); }
  const inventory = [{ path: 'Hero_Siege.exe', sha256: hash('game executable'), size: 15 }, { path: 'data.win', sha256: hash('game bytes'), size: 10 }];
  const manifest = {
    schema_version: 1,
    game: { fingerprint: hash(JSON.stringify(inventory)), files: inventory },
    datasets,
  };
  put(join(directory, 'manifest.json'), JSON.stringify(manifest));
  return manifest;
}
function existing(root) {
  put(join(root, 'tools/data/helper/items.json'), 'old items');
  put(join(root, 'tools/data/helper/talents.json'), 'old talents');
  put(join(root, 'src/items.js'), 'old js');
  put(join(root, 'src-tauri/src/items.rs'), 'old rust');
}
function assertOld(root) {
  assert.equal(readFileSync(join(root, 'tools/data/helper/items.json'), 'utf8'), 'old items');
  assert.equal(readFileSync(join(root, 'tools/data/helper/talents.json'), 'utf8'), 'old talents');
  assert.equal(readFileSync(join(root, 'src/items.js'), 'utf8'), 'old js');
  assert.equal(readFileSync(join(root, 'src-tauri/src/items.rs'), 'utf8'), 'old rust');
}
function generate(_command, _args, { env }) {
  put(env.HS_ITEMS_OUT_JS, 'export const ITEMS = {};');
  put(env.HS_ITEMS_OUT_RS, 'pub const ITEMS: usize = 0;');
  put(join(env.HS_ITEMS_LANG_OUT, 'ru.json'), '{"items":{}}');
}

test('a nested generation is validated against dataset hashes and its installed game', async (t) => {
  const root = temporary(t), bundle = join(root, 'bundle'), game = join(root, 'game');
  fixture(bundle, { game });
  const valid = await validateBundle(bundle, { game });
  assert.match(valid.files.get('items'), /generations[\\/]fixture[\\/]items.json$/);
  put(valid.files.get('items'), '[]');
  await assert.rejects(validateBundle(bundle, { game }), /hash does not match/);
});

test('Steam discovery follows libraryfolders and the installed game directory', (t) => {
  const root = temporary(t), steam = join(root, 'steam'), library = join(root, 'other-library');
  put(join(steam, 'steamapps/libraryfolders.vdf'), `"libraryfolders" { "0" { "path" "${library.replace(/\\/g, '\\\\')}" } }`);
  put(join(library, 'steamapps/appmanifest_269210.acf'), '"AppState" { "installdir" "HeroSiege-copy" }');
  const game = join(library, 'steamapps/common/HeroSiege-copy/bin');
  put(join(game, 'data.win'), 'fixture');
  const libraries = steamLibraries({ platform: 'win32', env: {}, registry: `SteamPath    REG_SZ    ${steam}` });
  assert.ok(libraries.includes(library));
  assert.equal(findGame(libraries), game);
});

test('a bundle for another game build is rejected', async (t) => {
  const root = temporary(t), bundle = join(root, 'bundle'), game = join(root, 'game');
  fixture(bundle, { game });
  put(join(game, 'data.win'), 'other game');
  await assert.rejects(validateBundle(bundle, { game }), /differs from the bundle/);
});

test('the game fingerprint certifies a unique mandatory UTF-8 source inventory', async (t) => {
  const root = temporary(t), bundle = join(root, 'bundle');
  const manifest = fixture(bundle);
  manifest.game.files.push({ size: 3, sha256: hash('csv').toUpperCase(), path: 'source/日本語.csv' });
  const normalized = manifest.game.files.map(({ path, sha256, size }) => ({ path, sha256: sha256.toLowerCase(), size }))
    .sort((a, b) => Buffer.compare(Buffer.from(a.path), Buffer.from(b.path)));
  manifest.game.fingerprint = hash(JSON.stringify(normalized));
  put(join(bundle, 'manifest.json'), JSON.stringify(manifest));
  await validateBundle(bundle);
  manifest.game.files[0].size++;
  put(join(bundle, 'manifest.json'), JSON.stringify(manifest));
  await assert.rejects(validateBundle(bundle), /fingerprint does not match/);
  manifest.game.files.push(manifest.game.files[0]);
  put(join(bundle, 'manifest.json'), JSON.stringify(manifest));
  await assert.rejects(validateBundle(bundle), /Duplicate game source/);
  manifest.game.files = manifest.game.files.filter(({ path }) => path !== 'Hero_Siege.exe');
  put(join(bundle, 'manifest.json'), JSON.stringify(manifest));
  await assert.rejects(validateBundle(bundle), /must include/);
});

test('a manifest cannot escape its bundle with a dataset or source path', async (t) => {
  const root = temporary(t), bundle = join(root, 'bundle');
  const manifest = fixture(bundle);
  manifest.game.files[0].path = '../outside';
  put(join(bundle, 'manifest.json'), JSON.stringify(manifest));
  await assert.rejects(validateBundle(bundle), /escapes/);
});

test('publication rolls back the whole group when an installation step fails', (t) => {
  const root = temporary(t); existing(root);
  put(join(root, 'stage/js'), 'new js');
  put(join(root, 'stage/rust'), 'new rust');
  assert.throws(() => publishItemUpdate(root, [
    { source: join(root, 'stage/js'), target: 'src/items.js' },
    { source: join(root, 'stage/rust'), target: 'src-tauri/src/items.rs' },
  ], { afterInstall(index) { if (index === 1) throw new Error('simulated failure'); } }), /simulated failure/);
  assertOld(root);
  assert.equal(existsSync(join(root, 'tools/data/.item-update-transaction.json')), false);
});

test('an interrupted publication is recovered before a new update', (t) => {
  const root = temporary(t); existing(root);
  put(join(root, 'tools/data/.old-js'), 'old js');
  put(join(root, 'src/items.js'), 'uncommitted js');
  put(join(root, 'tools/data/.item-update-transaction.json'), JSON.stringify({
    schema_version: 1, phase: 'installing', entries: [{ target: 'src/items.js', backup: 'tools/data/.old-js', existed: true }],
  }));
  assert.equal(recoverItemUpdate(root), true);
  assertOld(root);
});

test('an exporter that exits successfully without a new manifest cannot install old files', async (t) => {
  const root = temporary(t), game = join(root, 'game'), extractor = join(root, 'extractor');
  existing(root); put(join(game, 'data.win'), 'game'); put(extractor, 'fixture');
  let called = false;
  await assert.rejects(updateItemTables({ root, game, extractor, extractorHome: root, bundle: '', run(_file, args, options) {
    called = true;
    assert.equal(args[0], '--export');
    assert.equal(options.env.HERO_SIEGE, game);
    assert.equal(options.env.HERO_SIEGE_BIN, game);
  } }), /manifest.json/);
  assert.ok(called); assertOld(root);
});

test('a newly created empty lock cannot be reclaimed by another update', async (t) => {
  const root = temporary(t); existing(root);
  put(join(root, 'tools/data/.item-update.lock'), '');
  await assert.rejects(updateItemTables({ root, game: '', bundle: '', run() { assert.fail('must not run'); } }), /lock is incomplete/);
  assert.equal(readFileSync(join(root, 'tools/data/.item-update.lock'), 'utf8'), '');
  assertOld(root);
});

test('a generator failure leaves both installed data and lookup tables intact', async (t) => {
  const root = temporary(t), bundle = join(root, 'input'); existing(root); fixture(bundle);
  await assert.rejects(updateItemTables({ root, game: '', bundle, run() { throw new Error('generator failed'); } }), /generator failed/);
  assertOld(root);
});

test('dry run validates and generates without publication', async (t) => {
  const root = temporary(t), bundle = join(root, 'input'); existing(root); fixture(bundle);
  const result = await updateItemTables({ root, game: '', bundle, dry: true, run: generate });
  assert.equal(result.installed, false);
  assert.ok(existsSync(join(result.staging, 'generated/items.js')));
  assertOld(root);
});

test('successful publication installs items, talents, manifest and all generated outputs', async (t) => {
  const root = temporary(t), bundle = join(root, 'input'); existing(root); const manifest = fixture(bundle);
  const result = await updateItemTables({ root, game: '', bundle, run: generate });
  assert.equal(result.installed, true);
  const installed = await validateBundle(join(root, 'tools/data/helper'));
  assert.equal(installed.manifest.game.fingerprint, manifest.game.fingerprint);
  assert.equal(JSON.parse(readFileSync(join(root, 'tools/data/helper/talents.json'), 'utf8'))[0].key, 'testTalent');
  assert.equal(readFileSync(join(root, 'src/items.js'), 'utf8'), 'export const ITEMS = {};');
  assert.equal(readFileSync(join(root, 'src/lang/ru.json'), 'utf8'), '{"items":{}}');
});

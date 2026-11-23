// Validate an Extractor generation and publish its inputs and generated tables
// as a recoverable transaction. The journal is deliberately kept outside the
// helper directory: replacing that directory cannot replace the recovery path.
import { createHash } from 'node:crypto';
import { createReadStream } from 'node:fs';
import {
  closeSync, copyFileSync, existsSync, mkdirSync, mkdtempSync,
  openSync, readFileSync, readdirSync, realpathSync, renameSync, rmSync,
  statSync, unlinkSync, writeFileSync,
} from 'node:fs';
import { dirname, isAbsolute, join, relative, resolve, sep } from 'node:path';

const SHA256 = /^[0-9a-f]{64}$/i;
export async function sha256(path) {
  const hash = createHash('sha256');
  for await (const chunk of createReadStream(path)) hash.update(chunk);
  return hash.digest('hex');
}

export function within(root, name) {
  if (typeof name !== 'string' || !name || isAbsolute(name) || /^[A-Za-z]:/.test(name)) {
    throw new Error(`Expected a relative bundle path: ${name}`);
  }
  const full = resolve(root, name);
  const rel = relative(resolve(root), full);
  if (!rel || rel === '..' || rel.startsWith(`..${sep}`) || isAbsolute(rel)) {
    throw new Error(`Path escapes its directory: ${name}`);
  }
  if (existsSync(full)) {
    const actual = relative(realpathSync(root), realpathSync(full));
    if (actual === '..' || actual.startsWith(`..${sep}`) || isAbsolute(actual)) {
      throw new Error(`Bundle path resolves outside its directory: ${name}`);
    }
  }
  return full;
}

export async function validateBundle(directory, { game = '' } = {}) {
  const root = resolve(directory);
  const manifest = JSON.parse(readFileSync(join(root, 'manifest.json'), 'utf8'));
  if (manifest.schema_version !== 1 || !manifest.game || !SHA256.test(manifest.game.fingerprint ?? '')) {
    throw new Error('Unsupported or incomplete Extractor manifest (expected schema_version 1 and game fingerprint).');
  }
  if (!manifest.datasets || typeof manifest.datasets !== 'object') throw new Error('Bundle has no datasets.');
  const files = new Map();
  for (const [name, dataset] of Object.entries(manifest.datasets)) {
    if (!dataset || !SHA256.test(dataset.sha256 ?? '') || !['complete', 'partial'].includes(dataset.status)) {
      throw new Error(`Dataset ${name} has no valid hash or extraction status.`);
    }
    const path = within(root, dataset.file);
    if (!statSync(path).isFile()) throw new Error(`Dataset ${name} is not a file.`);
    if (await sha256(path) !== dataset.sha256.toLowerCase()) throw new Error(`Dataset ${name} hash does not match its manifest.`);
    files.set(name, path);
  }
  for (const name of ['items', 'talents']) {
    if (!files.has(name)) throw new Error(`Bundle has no ${name} dataset.`);
    const rows = JSON.parse(readFileSync(files.get(name), 'utf8'));
    const dataset = manifest.datasets[name];
    if (!Array.isArray(rows) || !rows.length || !Number.isInteger(dataset.records) || dataset.records !== rows.length) {
      throw new Error(`Dataset ${name} is empty or its record count does not match.`);
    }
    if (name === 'items') {
      const recordIds = new Set();
      for (const row of rows) {
        const meta = row?.metadata;
        if (!meta || typeof meta.tkey !== 'string' || !meta.tkey || !Array.isArray(meta.fromGame)) {
          throw new Error('An item is missing its translation key or field provenance.');
        }
        const recordId = meta.record_id ?? row.record_id;
        if (typeof recordId !== 'string' || !recordId || recordIds.has(recordId)) {
          throw new Error('Item record identities are absent or duplicated.');
        }
        recordIds.add(recordId);
      }
    } else {
      const ids = new Set();
      for (const row of rows) {
        if (!Number.isInteger(row?.id) || typeof row.key !== 'string' || !row.key || ids.has(row.id)) {
          throw new Error('Talent identities are absent or duplicated.');
        }
        ids.add(row.id);
      }
    }
  }
  if (!files.has('translationsItem')) throw new Error('Bundle has no translationsItem CSV dataset.');
  if (!Array.isArray(manifest.game.files) || !manifest.game.files.length) throw new Error('Manifest has no source file inventory.');
  const inventory = [], sourceNames = new Set();
  for (const input of manifest.game.files) {
    if (!input || !SHA256.test(input.sha256 ?? '') || !Number.isSafeInteger(input.size) || input.size < 0) {
      throw new Error('Invalid game source fingerprint.');
    }
    // Checking the path even without an installed game rejects traversal in a
    // package that would otherwise become dangerous when a game is configured.
    const source = within(game || root, input.path);
    if (sourceNames.has(input.path)) throw new Error(`Duplicate game source path: ${input.path}`);
    sourceNames.add(input.path);
    inventory.push({ path: input.path, sha256: input.sha256.toLowerCase(), size: input.size });
    if (game) {
      if (!existsSync(source) || statSync(source).size !== input.size || await sha256(source) !== input.sha256.toLowerCase()) {
        throw new Error(`Game source ${input.path} differs from the bundle. Export the installed build again.`);
      }
    }
  }
  if (!sourceNames.has('Hero_Siege.exe') || !sourceNames.has('data.win')) throw new Error('Source inventory must include Hero_Siege.exe and data.win.');
  inventory.sort((a, b) => Buffer.compare(Buffer.from(a.path, 'utf8'), Buffer.from(b.path, 'utf8')));
  if (createHash('sha256').update(JSON.stringify(inventory), 'utf8').digest('hex') !== manifest.game.fingerprint.toLowerCase()) {
    throw new Error('Game fingerprint does not match its source inventory.');
  }
  return { root, manifest, files };
}

export function stageBundle(bundle, destination) {
  mkdirSync(destination, { recursive: true });
  for (const dataset of Object.values(bundle.manifest.datasets)) {
    const target = within(destination, dataset.file);
    mkdirSync(dirname(target), { recursive: true });
    copyFileSync(within(bundle.root, dataset.file), target);
  }
  // Legacy readers get the same bytes, while canonical readers resolve the
  // generation paths named by the manifest.
  for (const name of ['items', 'talents']) copyFileSync(bundle.files.get(name), join(destination, `${name}.json`));
  copyFileSync(join(bundle.root, 'manifest.json'), join(destination, 'manifest.json'));
}

function transactionPaths(root) {
  const data = join(root, 'tools', 'data');
  mkdirSync(data, { recursive: true });
  return { data, journal: join(data, '.item-update-transaction.json'), lock: join(data, '.item-update.lock') };
}

function journalWrite(path, journal) {
  const next = `${path}.next`;
  writeFileSync(next, JSON.stringify(journal, null, 2));
  renameSync(next, path);
}

function rollback(root, journal) {
  for (const entry of [...journal.entries].reverse()) {
    const target = within(root, entry.target);
    const backup = within(root, entry.backup);
    if (existsSync(backup)) {
      rmSync(target, { recursive: true, force: true });
      renameSync(backup, target);
    } else if (!entry.existed) {
      rmSync(target, { recursive: true, force: true });
    }
  }
}

export function recoverItemUpdate(root) {
  const { journal } = transactionPaths(root);
  if (!existsSync(journal)) return false;
  const state = JSON.parse(readFileSync(journal, 'utf8'));
  if (state.schema_version !== 1 || !Array.isArray(state.entries)) throw new Error('Invalid item update recovery journal.');
  if (state.phase !== 'committed') rollback(root, state);
  for (const entry of state.entries) rmSync(within(root, entry.backup), { recursive: true, force: true });
  unlinkSync(journal);
  return true;
}

export function withItemUpdateLock(root, action) {
  const { lock } = transactionPaths(root);
  let fd;
  try { fd = openSync(lock, 'wx'); }
  catch (e) {
    if (e.code !== 'EEXIST') throw e;
    // Serialize stale-lock reclamation. Otherwise two callers can both observe
    // a dead PID and the second can unlink the first caller's replacement lock.
    const recovery = `${lock}.recovering`;
    let guard;
    try { guard = openSync(recovery, 'wx'); }
    catch { throw new Error('Another item update is acquiring or recovering its lock.'); }
    try {
      if (existsSync(lock)) {
        const content = readFileSync(lock, 'utf8').trim();
        const pid = /^\d+$/.test(content) ? Number(content) : 0;
        // open('wx') necessarily precedes writing its PID. An empty lock is a
        // live acquisition window, never evidence that its owner is dead.
        if (!Number.isSafeInteger(pid) || pid <= 0) throw new Error('Item update lock is incomplete; another update may be acquiring it.');
        try { process.kill(pid, 0); throw new Error(`Another item update is running (PID ${pid}).`); }
        catch (probe) { if (probe.code !== 'ESRCH') throw probe; }
        unlinkSync(lock);
      }
      fd = openSync(lock, 'wx');
    } finally {
      closeSync(guard);
      unlinkSync(recovery);
    }
  }
  try { writeFileSync(fd, String(process.pid)); }
  catch (error) { closeSync(fd); unlinkSync(lock); throw error; }
  return Promise.resolve().then(action).finally(() => { closeSync(fd); unlinkSync(lock); });
}

export function publishItemUpdate(root, entries, { afterInstall } = {}) {
  const { data, journal } = transactionPaths(root);
  if (existsSync(journal)) throw new Error('Recover the previous item update before publishing.');
  const backupRoot = mkdtempSync(join(data, '.item-backup-'));
  const state = { schema_version: 1, phase: 'installing', entries: entries.map((entry, index) => {
    const target = within(root, entry.target);
    if (!existsSync(entry.source)) throw new Error(`Staged output is missing: ${entry.source}`);
    return { target: relative(root, target), backup: relative(root, join(backupRoot, String(index))), existed: existsSync(target) };
  }) };
  journalWrite(journal, state);
  try {
    for (let index = 0; index < entries.length; index++) {
      const target = within(root, state.entries[index].target);
      const backup = within(root, state.entries[index].backup);
      mkdirSync(dirname(target), { recursive: true });
      if (state.entries[index].existed) renameSync(target, backup);
      // Staging is on the repository's volume, so the directory promotion and
      // each generated file promotion are renames, never partial byte copies.
      renameSync(entries[index].source, target);
      afterInstall?.(index);
    }
    state.phase = 'committed';
    journalWrite(journal, state);
  } catch (error) {
    rollback(root, state);
    unlinkSync(journal);
    rmSync(backupRoot, { recursive: true, force: true });
    throw error;
  }
  for (const entry of state.entries) rmSync(within(root, entry.backup), { recursive: true, force: true });
  rmSync(backupRoot, { recursive: true, force: true });
  unlinkSync(journal);
}

export function generatedEntries(staging) {
  const entries = [
    { source: join(staging, 'items.js'), target: 'src/items.js' },
    { source: join(staging, 'items.rs'), target: 'src-tauri/src/items.rs' },
  ];
  const languages = join(staging, 'lang');
  if (existsSync(languages)) {
    for (const name of readdirSync(languages).sort()) {
      if (!/^[a-z]{2}\.json$/.test(name)) throw new Error(`Unexpected generated language file: ${name}`);
      JSON.parse(readFileSync(join(languages, name), 'utf8'));
      entries.push({ source: join(languages, name), target: `src/lang/${name}` });
    }
  }
  return entries;
}

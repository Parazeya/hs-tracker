import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { ITEMS, TIER_LETTERS } from '../src/items.js';

const VIRTUAL_ID = 'virtual:tracker-item-sources';
const RESOLVED_ID = `\0${VIRTUAL_ID}`;

const nameKey = (name) => name?.trim().toLowerCase();
const exactIdentity = (recordId) => recordId?.match(/@(\d+:\d+:\d+)$/)?.[1];
const SOURCE_QUALIFIER = /\s*\(Inferno(?: Only| Difficulty)?\)$/i;

export function roomLabels(map, sidecars) {
  const labels = {};
  for (const [lang, words] of Object.entries(sidecars)) {
    for (const [index, node] of Object.entries(words.nodes ?? {})) {
      const room = map.nodes?.[Number(index)]?.room;
      if (room && typeof node.name === 'string') (labels[room] ??= {})[lang] = node.name;
    }
  }
  return labels;
}

export function itemSources(codex, map, items = ITEMS) {
  if (!codex.source?.fingerprint || codex.source.fingerprint !== map.source?.fingerprint) {
    throw new Error('The bundled hs-map Codex and map data come from different game builds.');
  }

  const identitiesByName = new Map();
  for (const [identity, name] of Object.entries(items)) {
    const key = nameKey(name);
    const identities = identitiesByName.get(key) ?? [];
    identities.push(identity);
    identitiesByName.set(key, identities);
  }

  const catalogued = new Map();
  const codexNameCounts = new Map();
  for (const [key, primary] of Object.entries(codex.items)) {
    const records = [primary, ...(primary.variants ?? [])];
    for (const name of new Set(records.map((record) => nameKey(record.names?.en ?? primary.names?.en)))) {
      if (name) codexNameCounts.set(name, (codexNameCounts.get(name) ?? 0) + 1);
    }
    for (const record of records) {
      const identity = exactIdentity(record.record_id);
      if (!identity || !Object.hasOwn(items, identity)
          || nameKey(record.names?.en ?? primary.names?.en) !== nameKey(items[identity])) continue;
      const matches = catalogued.get(identity) ?? [];
      matches.push({ key, record, variant: record !== primary });
      catalogued.set(identity, matches);
    }
  }

  const nodes = new Map((map.nodes ?? []).map((node) => [node.room, node]));
  const roomsByName = new Map();
  for (const node of nodes.values()) {
    for (const name of node.drops ?? []) {
      const rooms = roomsByName.get(name) ?? [];
      rooms.push(node.room);
      roomsByName.set(name, rooms);
    }
  }

  const dropsByName = new Map();
  for (const [source, group] of Object.entries(map.bosses ?? {})) {
    for (const drop of group.drops ?? []) {
      const name = nameKey(drop.item);
      if (!name) continue;
      const drops = dropsByName.get(name) ?? [];
      drops.push({
        source,
        inferno: drop.inferno || group.inferno_only,
        reference: (drop.reference ?? group.reference)?.verified_for_game_build === false,
      });
      dropsByName.set(name, drops);
    }
  }

  const index = {};
  for (const [identity, records] of catalogued) {
    if (records.length !== 1) continue;
    const [{ key, record, variant }] = records;
    const name = nameKey(items[identity]);
    const unambiguous = identitiesByName.get(name).length === 1 && codexNameCounts.get(name) === 1;
    const mapItem = map.items?.[name];
    const matchesMapRow = mapItem?.key
      ? mapItem.key === key && (mapItem.record_id ? mapItem.record_id === record.record_id : !variant)
      : unambiguous && !variant;
    const projection = record.map_sources;
    const roomIds = projection ? projection.rooms ?? [] : matchesMapRow ? roomsByName.get(name) ?? [] : [];
    const shelves = projection ? new Set(projection.shelves ?? []) : null;
    const locations = new Map();
    for (const label of record.places ?? []) {
      const source = label.replace(SOURCE_QUALIFIER, '');
      locations.set(label, [label, Object.hasOwn(map.bosses ?? {}, source) ? source : '', false]);
    }
    if (matchesMapRow || shelves) {
      for (const { source, inferno, reference } of dropsByName.get(name) ?? []) {
        if (shelves && !shelves.has(source)) continue;
        const label = inferno ? `${source} (Inferno Only)` : source;
        if (!locations.has(label)) locations.set(label, [label, source, reference]);
      }
    }
    // A stated area name is already part of the item facts. Add precise room
    // chips when those facts omit the area or the map attaches a reference.
    for (const id of roomIds) {
      const node = nodes.get(id);
      if (!node) continue;
      const reference = node.reference_drops?.includes(name) ?? false;
      if (record.places?.length && !reference) continue;
      const label = node.name?.en || id;
      locations.set(`room:${id}`, [label, id, reference, 'room']);
    }
    const rarity = record.rarity === 'Satanic Set' ? 'Set' : record.rarity;
    const tier = TIER_LETTERS.indexOf(record.tier) + 1;
    index[identity] = {
      key,
      ...(variant && { record: record.record_id }),
      ...(rarity && { rarity }),
      ...(tier && { tier }),
      ...(record.rate && { rate: record.rate }),
      ...(record.chase && { chase: record.chase }),
      ...(locations.size && { locations: [...locations.values()] }),
      ...(roomIds.length && { find: true }),
    };
  }
  return index;
}

export function trackerItemSources() {
  let files;
  return {
    name: 'tracker-item-sources',
    configResolved(config) {
      files = ['codex.json', 'map.json'].map((name) => resolve(config.publicDir, 'hs-map', 'data', name));
    },
    resolveId(id) {
      if (id === VIRTUAL_ID) return RESOLVED_ID;
    },
    load(id) {
      if (id !== RESOLVED_ID) return;
      for (const path of files) this.addWatchFile(path);
      const [codex, map] = files.map((path) => JSON.parse(readFileSync(path, 'utf8')));
      const sidecars = {};
      for (const lang of map.langs ?? []) {
        if (lang === 'en') continue;
        const path = resolve(dirname(files[1]), `map.${lang}.json`);
        this.addWatchFile(path);
        sidecars[lang] = JSON.parse(readFileSync(path, 'utf8'));
      }
      return `export const locations = ${JSON.stringify(roomLabels(map, sidecars))}; export default ${JSON.stringify(itemSources(codex, map))};`;
    },
  };
}

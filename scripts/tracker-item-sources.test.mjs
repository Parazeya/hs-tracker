import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { itemSources, roomLabels } from './tracker-item-sources.mjs';

const load = (name) => JSON.parse(readFileSync(new URL(`../public/hs-map/data/${name}.json`, import.meta.url)));

test('joins bundled source records by exact identity and preserves source confidence', () => {
  const index = itemSources(load('codex'), load('map'));
  const crown = index['0:84:0'];
  assert.equal(crown.key, 'helmet_crown_of_doom');
  assert.equal(crown.tier, 6);
  assert.deepEqual(crown.locations, [['King Rakhul (Inferno Only)', 'King Rakhul', false]]);

  const belt = index['8:19:0'];
  assert.equal(belt.key, 'belts_belt_of_infinite_wealth');
  assert.deepEqual(belt.locations, [['Crystal Chest', 'Crystal Chest', true]]);
  assert.ok(index['5:70:0'].locations.some(([label, source, reference]) =>
    label === 'Dungeon Chest (Inferno Only)' && source === 'Dungeon Chest' && reference,
  ));
  assert.equal(index['3:4:15'].locations, undefined);

  assert.equal(index['3:11:14'].key, 'w_gun_angel');
  assert.equal(index['3:11:14'].rarity, 'Set');
  assert.equal(index['15:116:0'].key, 'socketable_orb_of_angel');
  assert.equal(index['15:116:0'].rarity, undefined);
  const diablo = index['3:9:7'];
  assert.equal(diablo.key, 'w_chainsaw_diablo');
  assert.ok(diablo.find);
  assert.ok(diablo.locations.some(([name, room, reference, kind]) =>
    name === "The Devil's Breach" && room === 'Act_04_05' && reference && kind === 'room',
  ));
  assert.equal(roomLabels(load('map'), { ru: load('map.ru') }).Act_04_05.ru, 'Дьявольский пролом');
  assert.ok(Buffer.byteLength(JSON.stringify(index)) < 250_000);
});

test('does not blindly join source tables keyed by a shared name', () => {
  const source = { fingerprint: 'build' };
  const codex = {
    source,
    items: {
      one: { record_id: 'one@3:1:0', names: { en: 'One' }, places: ['King'] },
      two: { record_id: 'two@3:2:0', names: { en: 'One' } },
    },
  };
  const map = { source, bosses: { King: { drops: [{ item: 'one' }] } }, items: { one: { zones: ['1-01'] } } };
  const index = itemSources(codex, map, { '3:1:0': 'One', '3:2:0': 'One' });
  assert.deepEqual(index['3:1:0'].locations, [['King', 'King', false]]);
  assert.equal(index['3:2:0'].locations, undefined);
  assert.equal(index['3:2:0'].find, undefined);
});

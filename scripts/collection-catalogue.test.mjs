import test from 'node:test';
import assert from 'node:assert/strict';
import { currentCatalogue, unverifiedHistory } from '../src/collection-catalogue.js';

test('a known unresolved identity cannot inherit a namesake rarity or old extra metadata', () => {
  const catalogue = currentCatalogue({
    items: { '3:1:14': 'Verified', '3:2:9': 'Unresolved' },
    byId: { '3:2:9': ['Unresolved', 'Unknown', 0, 0] },
    rarities: { verified: 'Set', unresolved: 'Heroic' }, tiers: {}, rates: {}, chases: {}, places: {}, zones: {},
  });
  assert.deepEqual(catalogue.map(item => item.name), ['Verified']);
});

test('unverified saved finds remain visible as history without fabricated quality facts', () => {
  const saved = { 'ethereal musket': 100, verified: 200, 'unrecognized saved item': 300 };
  const history = unverifiedHistory(saved, [{ key: 'verified' }]);
  assert.deepEqual(history, [['unrecognized saved item', 300], ['ethereal musket', 100]]);
  assert.equal(saved['ethereal musket'], 100);
  assert.equal(history.flat().includes('Heroic'), false);
});

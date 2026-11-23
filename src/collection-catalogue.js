const GEAR = new Set([0, 1, 2, 3, 4, 5, 6, 7, 8, 10, 18]);
const UNIQUE = new Set(['Satanic', 'Set', 'Heroic', 'Angelic', 'Unholy']);

// Only the current generated facts admit new catalogue entries. Historical
// manual metadata cannot establish an item's current quality or equipment slot.
export function currentCatalogue({ items, byId, rarities, tiers, rates, chases, places, zones }) {
  const seen = new Set();
  return Object.entries(items).flatMap(([key, name]) => {
    const [type, id, weapon] = key.split(':').map(Number);
    const lower = name.toLowerCase(), known = byId[key];
    const rarity = known?.[1] ?? rarities[lower] ?? '';
    if (!GEAR.has(type) || !UNIQUE.has(rarity) || seen.has(lower)) return [];
    seen.add(lower);
    return [{ key: lower, name, type, id, weapon, rarity, tier: known?.[2] ?? tiers[lower] ?? 0,
      rate: known?.[3] ?? rates[lower] ?? 0, chase: chases[lower] ?? 0,
      places: (places[lower] ?? []).filter(Boolean), zones: zones[lower] ?? [] }];
  });
}

export function unverifiedHistory(owned, catalogue) {
  const current = new Set(catalogue.map(item => item.key));
  return Object.entries(owned ?? {}).filter(([name]) => !current.has(name)).sort((a, b) => b[1] - a[1]);
}

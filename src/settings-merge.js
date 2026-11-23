const equal = (a, b) => JSON.stringify(a) === JSON.stringify(b);
const object = (x) => x !== null && typeof x === 'object' && !Array.isArray(x);

// Apply only local differences onto the newest backend state, recursively.
// Lists are intentional whole-list edits; unrelated object fields can merge.
export function mergeSettings(base, edited, current) {
  const result = structuredClone(current);
  for (const [key, value] of Object.entries(edited)) {
    if (equal(base[key], value)) continue;
    result[key] = object(base[key]) && object(value) && object(current[key])
      ? mergeSettings(base[key], value, current[key])
      : structuredClone(value);
  }
  for (const key of Object.keys(base)) if (!(key in edited)) delete result[key];
  return result;
}

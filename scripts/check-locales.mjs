import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import { parse } from 'svelte/compiler';

const root = new URL('../', import.meta.url);
const read = path => readFileSync(new URL(path, root), 'utf8');
const files = path => readdirSync(new URL(path, root));
const languages = [...read('src/say.svelte.js').matchAll(/^  \['([a-z]+)', '/gm)].map(([, code]) => code);
const labels = new Map();

function staticLabels(node) {
  if (node?.type === 'Literal' && typeof node.value === 'string') return [node.value];
  if (node?.type === 'TemplateLiteral' && !node.expressions.length) return [node.quasis[0].value.cooked];
  if (node?.type === 'ConditionalExpression') {
    return [...staticLabels(node.consequent), ...staticLabels(node.alternate)];
  }
  return [];
}

for (const name of files('src/').filter(name => /\.(svelte|js)$/.test(name))) {
  const file = read(`src/${name}`);
  const seen = new WeakSet();
  function visit(node) {
    if (!node || typeof node !== 'object' || seen.has(node)) return;
    seen.add(node);
    if (node.type === 'CallExpression' && ['t', 'say'].includes(node.callee.name)) {
      for (const label of staticLabels(node.arguments[0])) labels.set(label, name);
    }
    for (const value of Object.values(node)) {
      if (Array.isArray(value)) value.forEach(visit);
      else visit(value);
    }
  }
  visit(parse(name.endsWith('.svelte') ? file : `<script>${file}</script>`, { modern: true }));
}

// The tray and native dialogs also read from the shipped dictionaries.
for (const name of files('src-tauri/src/').filter(name => name.endsWith('.rs'))) {
  for (const [, label] of read(`src-tauri/src/${name}`).matchAll(/say::say\(\s*"([^"\n]+)"\)/g)) {
    labels.set(label, name);
  }
}

const placeholders = text => [...text.matchAll(/\{[a-zA-Z_][\w-]*\}/g)].map(([key]) => key).sort().join();
const issues = [];
for (const lang of languages.filter(code => code !== 'en')) {
  const dictionary = JSON.parse(read(`src/lang/${lang}.json`));
  for (const [section, entries] of Object.entries(dictionary)) {
    for (const [key, text] of Object.entries(entries)) {
      if (typeof text !== 'string' || !text.trim() || text.includes('\uFFFD')) issues.push(`${lang}/${section}: ${key}`);
    }
  }
  for (const [label, file] of labels) {
    const text = dictionary.ui[label];
    if (!text || placeholders(text) !== placeholders(label)) issues.push(`${lang}/${file}: ${label}`);
  }
}
assert.ok(!issues.length, issues.join('\n'));
console.log(`${labels.size} fixed interface labels checked in ${languages.length} languages.`);

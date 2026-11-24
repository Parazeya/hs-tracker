import { writeFile } from 'node:fs/promises';

const api = 'https://api.github.com/repos/Parazeya/hs-tracker/releases';
const headers = { Accept: 'application/vnd.github+json' };
if (process.env.GH_TOKEN) headers.Authorization = `Bearer ${process.env.GH_TOKEN}`;

async function get(url) {
  const response = await fetch(url, { headers, signal: AbortSignal.timeout(20000) });
  if (!response.ok) throw new Error(`GitHub releases request failed: HTTP ${response.status}`);
  return response;
}

async function totalDownloads() {
  let url = `${api}?per_page=100`;
  let total = 0;
  while (url) {
    const response = await get(url);
    for (const release of await response.json()) {
      if (release.draft) continue;
      for (const asset of release.assets) total += asset.download_count;
    }
    url = response.headers.get('link')?.match(/<([^>]+)>; rel="next"/)?.[1];
  }
  return total;
}

const escape = (text) => text.replace(/[<>&]/g, (char) => ({ '<': '&lt;', '>': '&gt;', '&': '&amp;' })[char]);

function badge(label, value, color) {
  const left = Math.ceil(label.length * 6.5 + 14);
  const right = Math.ceil(value.length * 7 + 14);
  const width = left + right;
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="22" role="img" aria-labelledby="title">
  <title id="title">${escape(label)}: ${escape(value)}</title>
  <rect width="${width}" height="22" rx="4" fill="${color}"/>
  <path d="M4 0h${left - 4}v22H4a4 4 0 0 1-4-4V4a4 4 0 0 1 4-4" fill="#343a46"/>
  <g fill="#fff" font-family="Verdana, DejaVu Sans, sans-serif" font-size="11" text-anchor="middle">
    <text x="${left / 2}" y="15">${escape(label)}</text>
    <text x="${left + right / 2}" y="15">${escape(value)}</text>
  </g>
</svg>
`;
}

const [release, downloads] = await Promise.all([
  get(`${api}/latest`).then((response) => response.json()),
  totalDownloads(),
]);
if (!release.tag_name || !Number.isSafeInteger(downloads) || downloads < 0) {
  throw new Error('Invalid public release statistics');
}
const formatted = new Intl.NumberFormat('en-US', { notation: 'compact', maximumFractionDigits: 1 });
await Promise.all([
  writeFile(new URL('../screenshots/readme-release.svg', import.meta.url), badge('latest release', release.tag_name, '#287d3c')),
  writeFile(new URL('../screenshots/readme-downloads.svg', import.meta.url), badge('total downloads', formatted.format(downloads), '#6f42c1')),
]);

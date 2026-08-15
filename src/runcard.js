// A finished run as one picture, drawn in the game's own colours so it looks
// like it came out of Hero Siege rather than out of a spreadsheet. It exists to
// be pasted into a chat, which is why it is a picture at all: Discord will not
// keep a table readable, and a screenshot of the panel carries the whole window
// with it.
//
// The card is drawn here rather than in Rust because this is where the fonts
// and the sprites already are. Rust only takes the finished pixels and puts
// them on the clipboard.

import { zoneLabel } from './items.js';

const W = 760;
const H = 430;
// the two lower boxes, and the floor their contents may not cross
const BOX_TOP = 190;
const BOX_H = 200;
const BOX_FLOOR = BOX_TOP + BOX_H - 14;

const BLACK = '#120b0d';
const PLATE = '#221517';
const CRIMSON = '#962538';
const BONE = '#e8d8a8';
const GOLD = '#e8c860';
const DIM = '#8c7668';

const RARITIES = [
  ['Satanic', '#ca1717'],
  ['Set', '#40d040'],
  ['Heroic', '#00ffae'],
  ['Angelic', '#f6f794'],
  ['Unholy', '#e04a7a'],
];

const DIFFICULTIES = ['Normal', 'Nightmare', 'Hell'];

const font = (size, weight = '') => `${weight} ${size}px "CookieRun Bold", sans-serif`.trim();

function short(n) {
  const v = n ?? 0;
  const abs = Math.abs(v);
  if (abs >= 1e9) return `${(v / 1e9).toFixed(1)}B`;
  if (abs >= 1e6) return `${(v / 1e6).toFixed(1)}M`;
  if (abs >= 1000) return `${(v / 1e3).toFixed(1)}k`;
  return String(v);
}

function span(secs) {
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  return h > 0 ? `${h}h ${String(m).padStart(2, '0')}m` : `${m}m`;
}

const perHour = (value, secs) => (secs > 0 ? Math.round((value * 3600) / secs) : 0);

/// One of the game's chips: a dark slab with a thin bronze edge.
function chip(ctx, x, y, w, h) {
  ctx.fillStyle = '#1b1113';
  ctx.fillRect(x, y, w, h);
  ctx.strokeStyle = '#4a3428';
  ctx.lineWidth = 2;
  ctx.strokeRect(x + 1, y + 1, w - 2, h - 2);
}

/// A number with its label above and its rate below — the same three lines the
/// Runs panel shows, because a card that disagreed with the app would be worse
/// than no card.
function tile(ctx, x, y, w, label, value, sub, colour) {
  chip(ctx, x, y, w, 84);
  ctx.textBaseline = 'alphabetic';
  ctx.fillStyle = DIM;
  ctx.font = font(15);
  ctx.fillText(label, x + 14, y + 26);
  ctx.fillStyle = colour;
  ctx.font = font(32);
  ctx.fillText(value, x + 14, y + 58);
  ctx.fillStyle = DIM;
  ctx.font = font(14);
  ctx.fillText(sub, x + 14, y + 76);
}

/// Draw one finished run. `art` carries the images the page has already loaded
/// (the app's mark and the game's coin), both optional — a card without them is
/// still a card.
export function drawRunCard(run, art = {}) {
  const canvas = document.createElement('canvas');
  const dpr = 2; // a chat window will scale it down, never up
  canvas.width = W * dpr;
  canvas.height = H * dpr;
  const ctx = canvas.getContext('2d');
  ctx.scale(dpr, dpr);
  ctx.imageSmoothingEnabled = false;

  // the plate, lit from the top the way the game's panels are
  const sky = ctx.createLinearGradient(0, 0, 0, H);
  sky.addColorStop(0, PLATE);
  sky.addColorStop(1, BLACK);
  ctx.fillStyle = sky;
  ctx.fillRect(0, 0, W, H);
  ctx.strokeStyle = CRIMSON;
  ctx.lineWidth = 6;
  ctx.strokeRect(3, 3, W - 6, H - 6);
  ctx.strokeStyle = BLACK;
  ctx.lineWidth = 4;
  ctx.strokeRect(8, 8, W - 16, H - 16);

  // title: whose run, when, and how long it ran
  ctx.textBaseline = 'alphabetic';
  ctx.fillStyle = GOLD;
  ctx.font = font(26);
  ctx.fillText(run.character || 'Hero Siege', 26, 48);
  ctx.fillStyle = DIM;
  ctx.font = font(15);
  const who = [
    run.level ? `Lv ${run.level}` : null,
    DIFFICULTIES[run.difficulty] ?? null,
    new Date(run.started_ms).toLocaleString('en-GB', {
      day: '2-digit',
      month: 'short',
      hour: '2-digit',
      minute: '2-digit',
    }),
  ]
    .filter(Boolean)
    .join(' · ');
  ctx.fillText(who, 26, 70);

  ctx.fillStyle = BONE;
  ctx.font = font(30);
  ctx.textAlign = 'right';
  ctx.fillText(span(run.secs), W - 26, 52);
  ctx.fillStyle = DIM;
  ctx.font = font(14);
  ctx.fillText('this run', W - 26, 70);
  ctx.textAlign = 'left';

  // the four numbers, across
  const drops = RARITIES.reduce((sum, [name]) => sum + (run.items?.[name] ?? 0), 0);
  const cell = (W - 52 - 3 * 10) / 4;
  const tiles = [
    ['Gold', short(run.gold), `${short(perHour(run.gold, run.secs))}/h`, GOLD],
    ['XP', short(run.xp), `${short(perHour(run.xp, run.secs))}/h`, '#a06ae0'],
    ['Kills', short(run.kills), `${short(perHour(run.kills, run.secs))}/h`, '#00ffae'],
    ['Drops', short(drops), `${short(perHour(drops, run.secs))}/h`, BONE],
  ];
  tiles.forEach(([label, value, sub, colour], i) => {
    tile(ctx, 26 + i * (cell + 10), 90, cell, label, value, sub, colour);
  });

  // left: loot by rarity, with the ones that never dropped left out
  const half = (W - 52 - 10) / 2;
  chip(ctx, 26, 190, half, BOX_H);
  ctx.fillStyle = GOLD;
  ctx.font = font(16);
  ctx.fillText('Loot', 42, 216);
  let y = 244;
  const got = RARITIES.filter(([name]) => (run.items?.[name] ?? 0) > 0);
  for (const [name, colour] of got.length ? got : RARITIES) {
    ctx.fillStyle = colour;
    ctx.font = font(15);
    ctx.fillText(name, 42, y);
    ctx.textAlign = 'right';
    ctx.fillStyle = BONE;
    ctx.fillText(String(run.items?.[name] ?? 0), 26 + half - 16, y);
    ctx.textAlign = 'left';
    y += 24;
  }
  // Whatever the save counted: bosses put down, chests opened. It shares the
  // box with the loot, so it takes what room the loot left and no more — a
  // session with every rarity in it simply has no space for this list.
  const room = Math.floor((BOX_FLOOR - (y + 26)) / 22);
  const tallies = (run.tallies ?? []).slice(0, Math.max(0, Math.min(4, room)));
  if (tallies.length) {
    y += 8;
    ctx.fillStyle = GOLD;
    ctx.font = font(16);
    ctx.fillText('Killed & opened', 42, y);
    y += 26;
    for (const t of tallies) {
      ctx.fillStyle = DIM;
      ctx.font = font(15);
      ctx.fillText(t.label, 42, y);
      ctx.textAlign = 'right';
      ctx.fillStyle = t.group === 'chest' ? GOLD : '#ca1717';
      ctx.fillText(String(t.total), 26 + half - 16, y);
      ctx.textAlign = 'left';
      y += 22;
    }
  }

  // right: where the time actually went, longest first
  const rx = 26 + half + 10;
  chip(ctx, rx, 190, half, BOX_H);
  ctx.fillStyle = GOLD;
  ctx.font = font(16);
  ctx.fillText('Where it happened', rx + 16, 216);
  let ry = 240;
  for (const [where, secs] of (run.zones ?? []).slice(0, 5)) {
    ctx.fillStyle = BONE;
    ctx.font = font(14);
    ctx.fillText(zoneLabel(where), rx + 16, ry);
    ctx.textAlign = 'right';
    ctx.fillStyle = DIM;
    ctx.fillText(span(secs), rx + half - 16, ry);
    ctx.textAlign = 'left';
    // the bar underneath is the share of the run, which is the point of the list
    const width = Math.max(2, Math.round(((secs / Math.max(1, run.secs)) * (half - 32))));
    ctx.fillStyle = '#2c1e20';
    ctx.fillRect(rx + 16, ry + 6, half - 32, 5);
    ctx.fillStyle = CRIMSON;
    ctx.fillRect(rx + 16, ry + 6, width, 5);
    ry += 32;
  }
  if (!(run.zones ?? []).length) {
    ctx.fillStyle = DIM;
    ctx.font = font(14);
    ctx.fillText('the game never said where', rx + 16, ry);
  }

  // the finds, as a single line along the bottom — names, not counts
  const finds = (run.notable ?? []).map((d) => d.name).filter(Boolean);
  ctx.fillStyle = DIM;
  ctx.font = font(13);
  if (finds.length) {
    let line = finds.join(' · ');
    while (line && ctx.measureText(line).width > W - 150) {
      line = line.slice(0, line.lastIndexOf(' · '));
    }
    ctx.fillText(line || finds[0], 26, H - 20);
  }
  ctx.textAlign = 'right';
  ctx.fillStyle = '#5e4b45';
  ctx.fillText('HS Tracker', W - 26, H - 20);
  ctx.textAlign = 'left';

  if (art.coin) {
    ctx.drawImage(art.coin, 0, 0, art.coin.height, art.coin.height, W - 128, H - 34, 18, 18);
  }
  return canvas;
}

/// The card as raw RGBA, which is the shape a clipboard wants. Base64 because
/// the bridge to Rust is JSON, and a megabyte of numbers is not.
export function cardBytes(canvas) {
  const ctx = canvas.getContext('2d');
  const { data, width, height } = ctx.getImageData(0, 0, canvas.width, canvas.height);
  let binary = '';
  const chunk = 0x8000;
  for (let i = 0; i < data.length; i += chunk) {
    binary += String.fromCharCode.apply(null, data.subarray(i, i + chunk));
  }
  return { width, height, rgba: btoa(binary) };
}

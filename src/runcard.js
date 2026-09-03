// A finished run as one picture, in the game's own colours.
//
// A picture because it is made to be pasted into a chat: a table loses its
// shape there, and a screenshot of the panel drags the whole window along.
// Drawn here rather than in Rust because this is where the fonts and the
// sprites are; Rust only puts the finished pixels on the clipboard.
//
// Two cards, one function, so they cannot drift apart. The summary is the small
// one: headline figures and a single line of names. The full card is the same
// head over a ledger of every find, best grade first, in as many columns as it
// takes to keep the picture wider than it is tall — a chat window fits an image
// into a fixed box, so a squarer card renders SMALLER, and one that holds more
// can end up saying less.

import { nameOf, say, t, locale } from './say.svelte.js';
import { fmt, difficulty, RARITY_RANK } from './format.js';
import { DROP_RATE, tierByName, rarityByName, tierLabel } from './items.js';

const W_SUMMARY = 760;
const H_SUMMARY = 430;

const PAD = 26;
// the two boxes under the tiles, and the floor their contents may not cross
const BOX_TOP = 190;
const BOX_H = 200;
const BOX_FLOOR = BOX_TOP + BOX_H - 14;

// the ledger, on the full card only
const LEDGER_TOP = 404; // the rule above the heading
const ROW0 = 452; // the first row's baseline
const ROW = 26; // and the pitch after it
const GUT = 14; // between columns
const FOOT = 42; // the wordmark's own room, under the last row
// A column narrower than the first has no room for the odds, and one narrower
// than the second none for the clock. Sized against the longest names in the
// tables, which run to 49 characters in Russian against 36 in English.
const RATE_MIN_COL = 250;
const TIME_MIN_COL = 340;
// Wider than tall, and by this much. It is an observed property of the chat
// windows the card is pasted into rather than a documented one, so it is one
// named number: when it is wrong, this moves and every card re-shapes.
const MIN_ASPECT = 1.6;

const BLACK = '#120b0d';
const PLATE = '#221517';
const CRIMSON = '#962538';
const BONE = '#e8d8a8';
const GOLD = '#e8c860';
const DIM = '#8c7668';

const RARITIES = [
  // Literals, not tokens: this is drawn on a canvas, and getComputedStyle is
  // not consulted there. Kept in step with --rar-satanic by hand.
  ['Satanic', '#ff6a6a'],
  ['Set', '#40d040'],
  ['Heroic', '#00ffae'],
  ['Angelic', '#f6f794'],
  ['Unholy', '#e04a7a'],
];

// The five above are the ones a drop is normally announced under. A player who
// widens their alert list can put a Common or a Mythic in the journal too, and
// an undefined fillStyle does not throw — canvas keeps the previous colour, so
// the row would quietly borrow the tint of the row above it.
const RARITY_HEX = Object.fromEntries(RARITIES);

const font = (size, weight = '') => `${weight} ${size}px "CookieRun Bold", sans-serif`.trim();

// The card is the one thing that leaves the app, so it says the numbers the way
// the panels do. A second dialect here — M and B rather than kk, or turning to k
// three digits early — reads as another program's screenshot.
const short = fmt;

function span(secs) {
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  return h > 0 ? `${h}${t('h')} ${String(m).padStart(2, '0')}${t('m')}` : `${m}${t('m')}`;
}

const perHour = (value, secs) => (secs > 0 ? Math.round((value * 3600) / secs) : 0);

// ── what the finds are worth looking at first ────────────────────────────────
//
// A saved find is only { name, rarity, tier, ts_ms } — the identity was never
// recorded — so where the grade or the rarity is missing it is recovered from
// the tables by name, which is what stops an ungraded drop falling off the
// bottom of the sort.

const gradeOf = (d) => d.tier || tierByName(d.name) || 0;
const rankOf = (d) => RARITY_RANK[d.rarity || rarityByName(d.name)] ?? 0;
const rateOf = (name) => DROP_RATE[String(name ?? '').toLowerCase()] ?? null;

/// Grade first, then rarity, then how seldom the game gives it, then newest.
/// "Starting from SS" is this order and not a filter: on a real forty-drop run
/// five rows are SS, so a cut at SS would have printed five lines and called it
/// the whole run.
const byValue = (a, b) =>
  gradeOf(b) - gradeOf(a) ||
  rankOf(b) - rankOf(a) ||
  (rateOf(b.name) ?? 0) - (rateOf(a.name) ?? 0) ||
  (b.ts_ms ?? 0) - (a.ts_ms ?? 0);

/// The same item twice is one row and a count. Folded by name AND grade: the
/// tables give two different items the same English name eleven times over, and
/// a run that found both should not read as one item found twice.
function groupFinds(list) {
  const by = new Map();
  for (const d of list ?? []) {
    if (!d?.name) continue;
    const tier = gradeOf(d);
    const key = `${d.name}|${tier}`;
    const seen = by.get(key);
    if (seen) {
      seen.qty += 1;
      seen.ts_ms = Math.max(seen.ts_ms ?? 0, d.ts_ms ?? 0);
      continue;
    }
    by.set(key, {
      name: d.name,
      rarity: d.rarity || rarityByName(d.name) || '',
      tier,
      qty: 1,
      ts_ms: d.ts_ms ?? 0,
      rate: rateOf(d.name),
    });
  }
  return [...by.values()].sort(byValue);
}

/// How tall a card carrying `rows` ledger rows has to be.
const cardHeight = (rows) => (rows ? ROW0 + (rows - 1) * ROW + 8 + FOOT : LEDGER_TOP + FOOT);

/// How many columns to cut the ledger into.
///
/// Four first and only then wider: the common case gets the widest columns,
/// which is the room the odds and the longer German and Russian names need.
/// More columns are taken only when the card would otherwise grow taller than
/// the shape a chat window can show.
function ledgerShape(n, width) {
  for (const cols of [4, 5, 6]) {
    const rows = Math.ceil(n / cols);
    if (cardHeight(rows) <= width / MIN_ASPECT) return { cols, rows };
  }
  return { cols: 6, rows: Math.ceil(n / 6) };
}

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

/// As much of `text` as fits in `max`, with an ellipsis where it was cut.
function clip(ctx, text, max) {
  const s = String(text ?? '');
  if (!s || ctx.measureText(s).width <= max) return s;
  let cut = s.length;
  while (cut > 1 && ctx.measureText(`${s.slice(0, cut)}…`).width > max) cut -= 1;
  return `${s.slice(0, cut)}…`;
}

/// One line of the ledger: how many, what grade, what it is called, how seldom
/// the game gives it, and when it fell.
function findRow(ctx, x, y, col, f, i, showRate, showTime) {
  if (i % 2) {
    ctx.fillStyle = 'rgba(0, 0, 0, 0.25)';
    ctx.fillRect(x - 8, y - 18, col + 16, ROW);
  }
  const colour = RARITY_HEX[f.rarity] ?? BONE;

  if (f.qty > 1) {
    ctx.textAlign = 'right';
    ctx.fillStyle = DIM;
    ctx.font = font(12);
    ctx.fillText(`×${f.qty}`, x + 30, y);
    ctx.textAlign = 'left';
  }

  if (f.tier > 0) {
    ctx.fillStyle = '#1b1113';
    ctx.fillRect(x + 36, y - 14, 26, 18);
    ctx.strokeStyle = colour;
    ctx.lineWidth = 1;
    ctx.strokeRect(x + 36.5, y - 13.5, 25, 17);
    ctx.fillStyle = colour;
    ctx.font = font(11);
    ctx.textAlign = 'center';
    ctx.fillText(tierLabel(f.tier), x + 49, y - 1);
    ctx.textAlign = 'left';
  }

  const tail = (showRate ? 42 : 0) + (showTime ? 50 : 0);
  ctx.fillStyle = colour;
  ctx.font = font(13);
  ctx.fillText(clip(ctx, nameOf(f.name), col - 70 - tail), x + 70, y);

  ctx.textAlign = 'right';
  if (showRate) {
    ctx.font = font(12);
    if (f.rate) {
      ctx.fillStyle = BONE;
      ctx.fillText(short(f.rate), x + col - (showTime ? 50 : 0), y);
    } else {
      ctx.fillStyle = '#5e4b45';
      ctx.fillText('—', x + col - (showTime ? 50 : 0), y);
    }
  }
  if (showTime && f.ts_ms) {
    ctx.fillStyle = DIM;
    ctx.font = font(12);
    const at = new Date(f.ts_ms).toLocaleTimeString(locale(), { hour: '2-digit', minute: '2-digit' });
    ctx.fillText(at, x + col, y);
  }
  ctx.textAlign = 'left';
}

/// Draw one finished run. `art` carries the images the page has already loaded
/// (the app's mark and the game's coin), both optional — a card without them is
/// still a card. `opts.mode` is 'summary' (the default) or 'full'.
export function drawRunCard(run, art = {}, opts = {}) {
  const full = opts.mode === 'full';
  const finds = full ? groupFinds(run.notable) : [];
  const width = full ? (finds.length > 44 ? 1680 : 1280) : W_SUMMARY;
  const shape = full ? ledgerShape(finds.length, width) : { cols: 0, rows: 0 };
  const W = width;
  const H = full ? cardHeight(shape.rows) : H_SUMMARY;

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
  ctx.fillText(run.character || 'Hero Siege', PAD, 48);
  ctx.fillStyle = DIM;
  ctx.font = font(15);
  const who = [
    run.level ? `${t('Lv')} ${run.level}` : null,
    difficulty(run.difficulty, run.hell_sub),
    new Date(run.started_ms).toLocaleString(locale(), {
      day: '2-digit',
      month: 'short',
      hour: '2-digit',
      minute: '2-digit',
    }),
  ]
    .filter(Boolean)
    .join(' · ');
  ctx.fillText(who, PAD, 70);

  ctx.fillStyle = BONE;
  ctx.font = font(30);
  ctx.textAlign = 'right';
  ctx.fillText(span(run.secs), W - PAD, 52);
  ctx.fillStyle = DIM;
  ctx.font = font(14);
  ctx.fillText(t('this run'), W - PAD, 70);
  ctx.textAlign = 'left';

  // the numbers, across. The full card has the room for the two the panel
  // shows and the card never did: how many of the finds were SS, and the magic
  // find the run ended on.
  const drops = RARITIES.reduce((sum, [name]) => sum + (run.items?.[name] ?? 0), 0);
  const tiles = [
    [t('Gold'), short(run.gold), `${short(perHour(run.gold, run.secs))}${t('/h')}`, GOLD],
    [t('XP'), short(run.xp), `${short(perHour(run.xp, run.secs))}${t('/h')}`, '#a06ae0'],
    [t('Kills'), short(run.kills), `${short(perHour(run.kills, run.secs))}${t('/h')}`, '#00ffae'],
    [t('Drops'), short(drops), `${short(perHour(drops, run.secs))}${t('/h')}`, BONE],
  ];
  if (full) {
    const ss = finds.reduce((n, f) => n + (f.tier >= 6 ? f.qty : 0), 0);
    tiles.push(['SS', short(ss), `${short(perHour(ss, run.secs))}${t('/h')}`, GOLD]);
    // Both are `#[serde(default)]` on the run and were read into it later, so a
    // run filed before that carries zeroes rather than a figure — and a tile
    // reading nought is worse than one column more of everything else.
    if (run.mf || run.herolevel) {
      tiles.push([t('MF'), short(run.mf ?? 0), `${t('HLv')} ${run.herolevel ?? 0}`, '#6ab0e0']);
    }
  }
  const cell = (W - 2 * PAD - (tiles.length - 1) * 10) / tiles.length;
  tiles.forEach(([label, value, sub, colour], i) => {
    tile(ctx, PAD + i * (cell + 10), 90, cell, label, value, sub, colour);
  });

  // left: loot by rarity, with the ones that never dropped left out
  const half = (W - 2 * PAD - GUT) / 2;
  chip(ctx, PAD, BOX_TOP, half, BOX_H);
  ctx.fillStyle = GOLD;
  ctx.font = font(16);
  ctx.fillText(t('Loot'), PAD + 16, 216);
  let y = 244;
  const got = RARITIES.filter(([name]) => (run.items?.[name] ?? 0) > 0);
  for (const [name, colour] of got.length ? got : RARITIES) {
    ctx.fillStyle = colour;
    ctx.font = font(15);
    ctx.fillText(t(name), PAD + 16, y);
    ctx.textAlign = 'right';
    ctx.fillStyle = BONE;
    ctx.fillText(String(run.items?.[name] ?? 0), PAD + half - 16, y);
    ctx.textAlign = 'left';
    y += 24;
  }

  // right: whatever the save counted — bosses put down, chests opened. It has
  // a box of its own now that the zone list is gone, so it takes the room the
  // list needs rather than what the loot left it.
  const rx = PAD + half + GUT;
  chip(ctx, rx, BOX_TOP, half, BOX_H);
  ctx.fillStyle = GOLD;
  ctx.font = font(16);
  ctx.fillText(t('Killed & opened'), rx + 16, 216);
  let ry = 244;
  const tallies = (run.tallies ?? []).slice(0, Math.floor((BOX_FLOOR - ry) / 22) + 1);
  for (const row of tallies) {
    ctx.fillStyle = DIM;
    ctx.font = font(15);
    ctx.fillText(t(nameOf(row.label)), rx + 16, ry);
    ctx.textAlign = 'right';
    ctx.fillStyle = row.group === 'chest' ? GOLD : '#ff6a6a';
    ctx.fillText(String(row.total), rx + half - 16, ry);
    ctx.textAlign = 'left';
    ry += 22;
  }
  if (!tallies.length) {
    ctx.fillStyle = DIM;
    ctx.font = font(14);
    ctx.fillText(t('nothing counted this run'), rx + 16, ry);
  }

  if (full) {
    drawLedger(ctx, W, finds, shape);
  } else {
    // the finds, as a single line along the bottom — the best of them first,
    // and a count of what would not fit, which is what the full card is for
    drawStrip(ctx, W, H, run.notable);
  }

  ctx.textAlign = 'right';
  ctx.fillStyle = '#5e4b45';
  ctx.font = font(13);
  ctx.fillText('HS Tracker', W - PAD, H - 20);
  ctx.textAlign = 'left';

  if (art.coin) {
    ctx.drawImage(art.coin, 0, 0, art.coin.height, art.coin.height, W - 128, H - 34, 18, 18);
  }
  return canvas;
}

/// The summary card's one line of names.
///
/// Built forward rather than by cutting the joined string back: `lastIndexOf`
/// answers -1 once a single name is already too wide, so cutting back chops one
/// character at a time and then prints the untrimmed name anyway. Says how many
/// it left out.
function drawStrip(ctx, W, H, notable) {
  const names = [...(notable ?? [])].sort(byValue).map((d) => d.name && nameOf(d.name)).filter(Boolean);
  if (!names.length) return;
  ctx.fillStyle = DIM;
  ctx.font = font(13);
  const room = W - 150;
  let line = '';
  let shown = 0;
  for (const name of names) {
    const next = line ? `${line} · ${name}` : name;
    if (shown && ctx.measureText(next).width > room) break;
    line = next;
    shown += 1;
  }
  if (shown < names.length) {
    // Recounted as the line shrinks, or the tail says how many were left out
    // before the last name was taken back off.
    let tail = say('and {n} more', { n: names.length - shown });
    while (shown > 1 && ctx.measureText(`${line} · ${tail}`).width > room) {
      line = line.slice(0, line.lastIndexOf(' · '));
      shown -= 1;
      tail = say('and {n} more', { n: names.length - shown });
    }
    line = `${line} · ${tail}`;
  }
  ctx.fillText(clip(ctx, line, room), PAD, H - 20);
}

/// Every find, in columns, filled top to bottom so the first column holds the
/// best of them rather than every fourth one.
function drawLedger(ctx, W, finds, { cols, rows }) {
  ctx.strokeStyle = CRIMSON;
  ctx.lineWidth = 1;
  ctx.beginPath();
  ctx.moveTo(PAD, LEDGER_TOP + 0.5);
  ctx.lineTo(W - PAD, LEDGER_TOP + 0.5);
  ctx.stroke();

  const ss = finds.reduce((n, f) => n + (f.tier >= 6 ? f.qty : 0), 0);
  ctx.fillStyle = GOLD;
  ctx.font = font(16);
  ctx.fillText(say('Finds — {n}', { n: finds.reduce((n, f) => n + f.qty, 0) }), PAD, 424);
  ctx.textAlign = 'right';
  ctx.fillText(`${ss} ${tierLabel(6)}`, W - PAD, 424);
  ctx.textAlign = 'left';

  const col = (W - 2 * PAD - (cols - 1) * GUT) / cols;
  const showRate = col >= RATE_MIN_COL;
  const showTime = col >= TIME_MIN_COL;

  for (let c = 0; c < cols; c++) {
    const x = PAD + c * (col + GUT);
    // Only the numbers get a caption; the name is plainly a name. No rule under
    // the row either — at a 26px pitch it cuts across the first badge.
    ctx.fillStyle = DIM;
    ctx.font = font(11);
    ctx.textAlign = 'right';
    if (showRate) ctx.fillText(t('1 in'), x + col - (showTime ? 50 : 0), 440);
    if (showTime) ctx.fillText(t('found'), x + col, 440);
    ctx.textAlign = 'left';

    for (let r = 0; r < rows; r++) {
      const f = finds[c * rows + r];
      if (!f) break;
      findRow(ctx, x, ROW0 + r * ROW, col, f, r, showRate, showTime);
    }
  }
}

/// The card as a PNG, base64'd because the bridge to Rust is JSON.
///
/// Not raw RGBA: the small card is about seven megabytes of base64 that way, and
/// the full card is twenty times the pixels. As a PNG the largest is around six
/// hundred kilobytes.
export async function cardPng(canvas) {
  const blob = await new Promise((resolve) => canvas.toBlob(resolve, 'image/png'));
  if (!blob) throw new Error(t('the picture did not survive the trip'));
  const bytes = new Uint8Array(await blob.arrayBuffer());
  let binary = '';
  const chunk = 0x8000;
  for (let i = 0; i < bytes.length; i += chunk) {
    binary += String.fromCharCode.apply(null, bytes.subarray(i, i + chunk));
  }
  return { png: btoa(binary) };
}

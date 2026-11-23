import { RARITY_TINT } from './look.js';
export const TINTS = RARITY_TINT;
export const number = value => Number.isFinite(Number(value)) ? Number(value).toLocaleString('en-US', { maximumFractionDigits: 1 }) : '0';
export function duration(secs) {
  secs = Math.max(0, Math.floor(secs ?? 0));
  return `${Math.floor(secs / 3600) ? `${Math.floor(secs / 3600)}:` : ''}${String(Math.floor(secs / 60) % 60).padStart(2, '0')}:${String(secs % 60).padStart(2, '0')}`;
}
export function reading(id, snap, seconds) {
  const count = rarity => number(snap.items?.[rarity]?.total ?? 0);
  const boss = (snap.tallies ?? []).filter(tally => tally.group === 'boss').reduce((sum, tally) => sum + tally.total, 0);
  return {
    time: ['Run time', duration(seconds)], mail: ['Mail', snap.has_mail ? 'New mail' : '—'], boss: ['Bosses', number(boss)],
    rare: ['Angelic | Unholy', `${count('Angelic')} | ${count('Unholy')}`], sat: ['Satanic', count('Satanic')],
    heroset: ['Heroic | Set', `${count('Heroic')} | ${count('Set')}`], gold: ['Gold', number(snap.gold?.earned)],
    goldh: ['Gold per hour', number(snap.gold?.per_hour)], kills: ['Kills', number(snap.kills?.earned)],
    xp: ['Experience', number(snap.xp?.earned)], xph: ['Experience per hour', number(snap.xp?.per_hour)], ss: ['SS drops', number(snap.ss)],
  }[id] ?? ['', ''];
}
export function currentEntries(entries, now, maxFuture = 15000) {
  return (Array.isArray(entries) ? entries : []).filter(entry => entry && Number.isFinite(entry.until_ms)
    && entry.until_ms > now && entry.until_ms <= now + maxFuture).slice(-6);
}

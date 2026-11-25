export const COMPARISON_MINUTES = 5;
export const MIN_COMPARISONS = 3;
const MIN_SECONDS = COMPARISON_MINUTES * 60;

export function dominantZone(run) {
  let room = '';
  let longest = 0;
  for (const [zone, seconds] of run.zones ?? []) {
    if (typeof zone !== 'string' || !zone.trim() || !Number.isFinite(seconds)) continue;
    if (seconds > longest || (seconds > 0 && seconds === longest && zone < room)) {
      room = zone;
      longest = seconds;
    }
  }
  return room;
}

export function runDifficulty(run) {
  const { difficulty, hell_sub: hellSub } = run;
  if (!Number.isInteger(difficulty) || difficulty < 0) return { key: '', comparable: false };
  // An old record without a character also used zero as its missing difficulty.
  if (difficulty === 0 && !run.character && !run.level) return { key: '', comparable: false };

  if (difficulty === 2) {
    const grade = Number.isInteger(hellSub) && hellSub >= 1 && hellSub <= 5 ? hellSub : 0;
    return { key: `2:${grade}`, difficulty, hellSub: grade, comparable: grade > 0 };
  }
  return { key: String(difficulty), difficulty, comparable: true };
}

export function perHour(total, seconds) {
  return Number.isFinite(total) && Number.isFinite(seconds) && seconds > 0
    ? Math.round((total * 3600) / seconds)
    : null;
}

export function medianRate(runs, value) {
  const rates = runs.map((r) => perHour(value(r), r.secs)).filter(Number.isFinite).sort((a, b) => a - b);
  if (rates.length < MIN_COMPARISONS) return null;
  const middle = Math.floor(rates.length / 2);
  return rates.length % 2 ? rates[middle] : Math.round((rates[middle - 1] + rates[middle]) / 2);
}

export function runComparison(run, history) {
  const zone = dominantZone(run);
  const difficulty = runDifficulty(run);
  const eligible = Boolean(zone && difficulty.comparable);
  const peers = eligible ? history.filter((peer) =>
    peer.started_ms !== run.started_ms &&
    peer.secs >= MIN_SECONDS &&
    dominantZone(peer) === zone &&
    runDifficulty(peer).key === difficulty.key,
  ) : [];
  return { peers, eligible, longEnough: run.secs >= MIN_SECONDS };
}

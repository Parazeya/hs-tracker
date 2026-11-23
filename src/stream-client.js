const TOPICS = new Set(['snapshot', 'appearance', 'notifications', 'drops', 'map']);

export function streamClient({ onFrame, onStatus, onEpoch = () => {}, EventSourceImpl = globalThis.EventSource,
  setTimer = setTimeout, clearTimer = clearTimeout, url = '/events' }) {
  let source = null;
  let retryTimer = null;
  let retryDelay = 1000;
  let stopped = false;
  let epoch = null;
  const revisions = new Map();
  function connect() {
    if (stopped) return;
    const current = new EventSourceImpl(url);
    source = current;
    current.onopen = () => {
      if (source !== current || stopped) return;
      retryDelay = 1000;
      onStatus('connected');
    };
    current.onerror = () => {
      if (source !== current || stopped) return;
      current.close();
      source = null;
      onStatus('reconnecting');
      // 403 (disabled) is fatal for native EventSource retry. Recreate the
      // object ourselves after a bounded delay, including initial disabled
      // subscriptions. Exactly one timer/connection exists per source.
      const delay = retryDelay;
      retryDelay = Math.min(5000, retryDelay * 2);
      retryTimer = setTimer(() => { retryTimer = null; connect(); }, delay);
    };
    current.addEventListener('update', event => {
    if (source !== current || stopped) return;
    try {
      if (typeof event.data !== 'string' || event.data.length > 1024 * 1024) return;
      const update = JSON.parse(event.data);
      if (typeof update.epoch !== 'string' || !Array.isArray(update.frames) || update.frames.length > 8
        || !Number.isFinite(update.server_ms)) return;
      if (update.epoch !== epoch) { epoch = update.epoch; revisions.clear(); onEpoch(epoch); }
      for (const frame of update.frames) {
        if (!TOPICS.has(frame.topic) || !Number.isSafeInteger(frame.id) || frame.id < 1
          || frame.id <= (revisions.get(frame.topic) ?? 0)) continue;
        revisions.set(frame.topic, frame.id);
        onFrame(frame, { epoch, serverMs: update.server_ms });
      }
    } catch { /* A malformed frame cannot clear a good live picture. */ }
    });
  }
  connect();
  return () => { stopped = true; if (retryTimer !== null) clearTimer(retryTimer); source?.close(); };
}

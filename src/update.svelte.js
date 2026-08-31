// Whether there is a newer version, and the one thing to do about it.
//
// The check runs once, a moment after the dashboard has drawn, and says nothing
// whatever unless there is something to say — a panel that announces "you are
// up to date" every launch is a panel people learn not to read. Nothing is
// downloaded until the reader asks for it: the app sits on top of a game, and
// deciding by itself to restart in the middle of a run is not its call.
//
// The updater only accepts a package signed by the key whose public half is
// compiled into the binary (tauri.conf.json, `plugins.updater.pubkey`), so a
// release whose latest.json is unsigned, signed by another key, or edited after
// signing is refused here rather than installed. See DEVELOPING.md, "Updates".

import { check } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';
import { invoke, native } from './bridge.js';

/// The update itself, once one has been found. Held outside the runes because
/// it is a handle with methods on it, not something to draw.
let handle = null;

let found = $state(null);
let stage = $state('idle');
let progress = $state(0);
let failure = $state('');
let asked = false;

/// What the banner reads. Getters rather than the values themselves: a rune
/// exported by name is exported by value, and the banner would never move.
export const updater = {
  get found() {
    return found;
  },
  get stage() {
    return stage;
  },
  get progress() {
    return progress;
  },
  get failure() {
    return failure;
  },
};

const tell = (what) => invoke('report', { level: 'warn', message: what }).catch(() => {});

/// The release notes, readable in a banner.
///
/// They are a section of CHANGELOG.md and arrive as markdown: `### Added` over
/// a list of `- ` lines. Nothing here renders markdown, and a banner is the
/// wrong place to start — so the four marks that would otherwise be read as
/// text are taken off, the bullets are made into bullets, and the blank line
/// that separates every heading from its list is collapsed, because at 84px
/// tall those cost a third of what can be seen.
function plain(body) {
  return body
    .split(/\r?\n/)
    .map((line) => line.replace(/^#{1,6}\s+/, '').replace(/^\s*[-*]\s+/, '• '))
    .join('\n')
    .replace(/\n{2,}/g, '\n')
    .trim();
}

/// Ask once whether there is a newer version.
///
/// Every failure here is silent. No network, GitHub down, a corporate proxy in
/// the way, a release with no manifest yet — none of that is the reader's
/// problem to be told about while they are trying to read a gold count, and
/// all of it is ordinary. It goes to the log and nowhere else.
export async function lookForUpdate() {
  if (!native || asked) return;
  asked = true;
  try {
    const it = await check();
    if (!it) return;
    handle = it;
    found = { version: it.version, notes: plain(it.body ?? '') };
  } catch (e) {
    tell(`update check failed: ${e?.message ?? e}`);
  }
}

/// Fetch it and put it on. The installer runs itself and the app comes back up
/// on the new version; `relaunch` is what brings it back.
export async function installUpdate() {
  if (!handle || stage === 'downloading') return;
  stage = 'downloading';
  progress = 0;
  failure = '';
  let total = 0;
  let far = 0;
  try {
    await handle.downloadAndInstall((e) => {
      if (e.event === 'Started') total = e.data.contentLength ?? 0;
      else if (e.event === 'Progress') {
        far += e.data.chunkLength ?? 0;
        // A download with no length reported still has to look like it is
        // moving, so it creeps towards full instead of sitting at zero.
        progress = total ? Math.min(1, far / total) : Math.min(0.95, progress + 0.01);
      } else if (e.event === 'Finished') progress = 1;
    });
    stage = 'ready';
    await relaunch();
  } catch (e) {
    // This one IS said out loud: the reader pressed a button and it did not
    // happen. The release page is the way round it, which is what About is for.
    stage = 'failed';
    failure = String(e?.message ?? e);
    tell(`update install failed: ${failure}`);
  }
}

/// Not now. It comes back on the next launch, which is soon enough for a
/// session tracker and rare enough not to nag.
export function dismissUpdate() {
  found = null;
  handle = null;
}

import { convertFileSrc } from '@tauri-apps/api/core';
import { invoke, native } from './bridge.js';

import satanicWav from './assets/sounds/satanic.wav';
import setWav from './assets/sounds/set.wav';
import heroicWav from './assets/sounds/heroic.wav';
import angelicWav from './assets/sounds/angelic.wav';
import unholyWav from './assets/sounds/unholy.wav';
import mailWav from './assets/sounds/mail.wav';
import relicWav from './assets/sounds/relic.wav';
import zoneMp3 from './assets/sounds/zone.mp3';

/// Every key that owns a sound of its own. Two of them are not rarities at all
/// — the mail chime and the satanic zone rotating — and they are on this list
/// because everything that loads, tests or replaces a sound walks it.
export const RARITIES = ['satanic', 'set', 'heroic', 'angelic', 'unholy', 'mail', 'zone', 'relic'];

export const DEFAULTS = {
  satanic: satanicWav,
  set: setWav,
  heroic: heroicWav,
  angelic: angelicWav,
  unholy: unholyWav,
  mail: mailWav,
  // The rotation gets a sound of its own rather than borrowing the satanic
  // chime it used to: the two mean different things and were told apart only by
  // where on the screen something happened. Browse… puts any other file over
  // it, as for every key here.
  zone: zoneMp3,
  // A relic gets a chime of its own. It borrowed the mail one at first, for
  // want of anything better — every relic in the game is Common, so there was
  // no rarity sound to take — and a borrowed chime meant a relic and a letter
  // arriving sounded the same. Browse… replaces it like any other key.
  relic: relicWav,
};

// A custom file beside the exe wins over the built-in chime. It is streamed
// through the asset protocol; only if that is unavailable do we fall back to
// hauling the whole file over IPC as a data URL.
export async function soundUrl(rarity) {
  // there are no files beside the executable when there is no executable
  if (!native) return null;
  try {
    const path = await invoke('sound_path', { rarity });
    if (path) {
      const url = convertFileSrc(path);
      if (await loadable(url)) return url;
      // The inlined copy was handed back unchecked, which made the built-in
      // chime below unreachable: a truncated or mislabelled file — the picker
      // accepts anything with the right extension — then meant permanent
      // silence, while the panel still listed the sound as installed.
      const inlined = await invoke('load_sound', { rarity });
      if (inlined && (await loadable(inlined))) return inlined;
    }
  } catch {}
  return DEFAULTS[rarity];
}

function loadable(url) {
  return new Promise((resolve) => {
    const probe = new Audio();
    const done = (ok) => {
      probe.oncanplay = probe.onerror = null;
      resolve(ok);
    };
    probe.oncanplay = () => done(true);
    probe.onerror = () => done(false);
    probe.src = url;
    setTimeout(() => done(false), 2000);
  });
}

export function play(url, volume = 0.7) {
  if (!url) return;
  try {
    const a = new Audio(url);
    a.volume = Math.min(1, Math.max(0, volume));
    a.play().catch(() => {});
  } catch {}
}

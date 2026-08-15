import { convertFileSrc } from '@tauri-apps/api/core';
import { invoke, native } from './bridge.js';

import satanicWav from './assets/sounds/satanic.wav';
import setWav from './assets/sounds/set.wav';
import heroicWav from './assets/sounds/heroic.wav';
import angelicWav from './assets/sounds/angelic.wav';
import unholyWav from './assets/sounds/unholy.wav';
import mailWav from './assets/sounds/mail.wav';

export const RARITIES = ['satanic', 'set', 'heroic', 'angelic', 'unholy', 'mail'];

export const DEFAULTS = {
  satanic: satanicWav,
  set: setWav,
  heroic: heroicWav,
  angelic: angelicWav,
  unholy: unholyWav,
  mail: mailWav,
};

// A custom file beside the exe wins over the built-in chime. It is streamed
// through the asset protocol; only if that is unavailable do we fall back to
// hauling the whole file over IPC as a data URL.
export async function soundUrl(rarity) {
  // a page in a browser has no files beside an executable, and a stream does
  // not want the alerts twice
  if (!native) return null;
  try {
    const path = await invoke('sound_path', { rarity });
    if (path) {
      const url = convertFileSrc(path);
      if (await loadable(url)) return url;
      const inlined = await invoke('load_sound', { rarity });
      if (inlined) return inlined;
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

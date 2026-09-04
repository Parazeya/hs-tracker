// The release waiting, if there is one.
//
// One check, at the start of a launch, held here rather than in the panel that
// shows it: the About tab is where an update is read about, and the tab bar is
// where it has to be noticed. Both read this.
//
// Until now the app asked GitHub nothing unless someone pressed a button, and
// that was worth keeping while an update meant a download page. It no longer
// does — versions come out often enough that the ones people run are the ones
// they were told about — so this asks once per launch. It asks for one file:
// the release manifest. Nothing about the machine goes with the question.

import { check } from '@tauri-apps/plugin-updater';
import { native } from './bridge.js';

export const update = $state({
  /// The Update object the plugin returns: version, date, notes, and the
  /// methods that install it. Null when there is nothing newer.
  found: null,
  /// Whether the question has been asked this launch, so opening About does not
  /// ask again on top of the launch check.
  asked: false,
  /// Why the last check came back empty-handed, if it failed rather than found
  /// nothing.
  why: '',
});

/**
 * Ask what the newest release is.
 *
 * Quiet about failure: a machine with no network, or behind something that eats
 * the request, gets no error on screen from a check nobody asked for. The
 * button in About passes `again` and reads `why` itself.
 */
export async function lookForUpdate(again = false) {
  if (!native) return null;
  if (update.asked && !again) return update.found;
  update.asked = true;
  update.why = '';
  try {
    update.found = await check();
  } catch (e) {
    update.found = null;
    update.why = String(e?.message ?? e);
  }
  return update.found;
}

# Changelog

## 0.9.1 — 2026-08-07

First public release.

### Overlay
- Compact always-on-top overlay skinned with the game's own UI sprites:
  session timer, mail, gold, XP, item counters by rarity, Satanic Zone.
- Lock mode: pinned overlay is click-through except the lock button, and drops
  its frame and Reset button while the game is running.
- Per-section visibility, opacity, scale, remembered window positions.
- Global hotkeys for show/hide, lock and reset.

### Tracking
- Gold, experience and kills with per-hour rates. The game reports these only
  when it saves the character or banks gold, so they arrive in steps; the
  Statistics window says how long ago that last happened.
- Totals carry over a restart in `carried.json`, so the overlay shows the last
  known balance instead of zeros until the game reports again.
- Item counters for Satanic, Set, Heroic, Angelic and Unholy, with magic-find
  splits and resource counters for keys, materials, socketables and
  collectibles.
- Items resolved to their real names from (type, id, weapon type), with rarity
  and grade from datamined tables — the packet fields carry neither reliably.
- Notable drop counters (Angelic Key, Satanic Key, Satanic Dice, S and SS
  runes, graded as the game grades them), configurable in `settings.json`.
- Satanic Zone with pros, cons and a countdown to the half-hour rotation.

### Alerts
- Separate sound per rarity plus a mail reminder, with volume, preview and
  custom files.
- Alerts fire when an item is rolled onto the ground, not when it is picked up;
  the same item never chimes twice, and finds the server announces in chat
  always sound.
- Rarity switches and a minimum grade (D..SS) decide what is announced;
  counters keep recording everything.
- Fading drop ticker under the overlay showing item names.

### Windows
- Statistics: rarity cards, notable drops, gold/h and xp/h graph, drop
  timeline.
- Shopping list: entries copy to the clipboard on click.
- Settings with everything above, plus a packet log for diagnosing the parser.

### Capture
- Listens on every adapter the machine has and keeps the ones the game
  actually talks over, so a VPN, split tunnelling or a second NIC changes
  nothing. Adapters that produce nothing are dropped and retried later.
- Reassembles messages per TCP connection and flushes on a pause, so a save
  that only travels one way is never held back by a busy connection.
- Counters and windows are pushed from the backend when something changes, and
  only to windows that are on screen.

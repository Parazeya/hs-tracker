# Changelog

## 0.9.5 — 2026-08-13

Everything since the first release, in one entry: the three floating windows
became a single dashboard, alerts grew a filter system of their own, statistics
turned into a session overview, and the app now builds on Linux as well.

### The dashboard

- **One window instead of three.** Statistics, Shopping List and Settings are
  no longer separate panels but sections of a resizable dashboard with a
  sidebar. Two new sections joined them: Sound Filter and Sounds.
- **Two faces, one at a time.** The dashboard is where you set things up and
  read the run; **Compact mode** at the bottom of the sidebar folds it into the
  overlay that sits on top of the game, and the overlay's right-click menu has
  **Dashboard** to come back. Which one was up last is remembered, so the tray,
  the hotkey and the next launch all bring back the same face.
- The dashboard is an ordinary window: it takes the taskbar, it is not pinned
  above the game, and it can be dragged by any empty spot or by its title. Eight
  edges resize it, and both size and position come back next launch.
- A minimize button sits beside the close cross. Closing hides to the tray —
  tracking carries on.
- The tray menu follows: **Dashboard** and **Compact overlay** replace the three
  window entries; lock, reset and quit stay where they were.
- Resetting the session asks once — the button turns into **Sure?** and only
  wipes the run on a second click.

### Sound filters

- **Lists of specific items, each with its own sound.** A list holds named items
  and carries a sound file, a volume and a switch. When one of its items drops,
  that sound plays — even when the rarity switches and the minimum grade would
  have kept quiet. A list with no file of its own borrows the rarity's.
- **Filters are packs of lists**, switched from a dropdown, so a farming set and
  a trading set can live side by side. New, Copy (sounds included) and delete,
  plus one master switch for the whole pack.
- **Generate** builds a filter from the datamined drop rates in one click: S and
  SS gear sorted by rarity and cut into Common, Rare and VeryRare bands, with
  Angelic and Unholy in lists of their own.
- **Import… / Export…** move a whole filter as a single file with every list's
  sound embedded, so it arrives on another machine with its sounds intact.
- Search names the item you mean, showing its grade and its odds in short form
  (`1/576k`, `1/1.3M`); Enter adds the top hit. Lists reorder with arrows — the
  first match wins, so order is priority — and an item that sits in two lists
  gets a `?` with a tooltip naming the other one.
- Rarity alerts and the minimum grade moved to the head of the Sound Filter
  section; the six per-rarity sounds moved to a Sounds section of their own.
  Anything destructive now asks once.

### Statistics

- Rebuilt as an overview: the run across the top, then loot and the item
  timeline on the left, the Satanic Zone, the area panel and the rates graph on
  the right.
- **Drops in this area** — while you stand in a zone it names it (`Act 8 · Zone
  2`) and lists the items that roll better there, each with the chance that
  applies in the zone, which is the number the game prints in green, not the
  general one. Items tied to the act's dungeons are counted, not listed.
- The loot counters became a table with labelled `drops` and `per hour` columns.
  Notable finds and resources read as tallies underneath.
- Every row in the drop timeline has a **+** that adds that item to a list of the
  active sound filter on the spot.
- The XP tile also shows `in level` — the game's own bar towards the next hero
  level — so the two numbers can be compared at a glance.
- Totals carried over from the previous run are marked with `*` until the game
  confirms them in this session.
- The rates graph is drawn at the window's real size and pixel density instead
  of being stretched from a fixed bitmap.
- The Keys counter ignores Basic and Crystal keys, which used to bury the
  Angelic and Satanic keys it exists for.

### Tracking

- **Gold read the wrong purse when a new season opened.** Seasonal was decided
  by comparing against a season number compiled into the app, so the day season
  10 started the bank showed the non-seasonal balance. The character's own
  season decides now, and a new season needs no update.
- The status line no longer claims to be capturing when every adapter has
  actually failed to open, and an adapter that refuses to open is retried every
  five minutes instead of every second.
- A device that cannot be opened for want of permission is reported as such,
  instead of as "no suitable interface".
- The current zone is read from the client's own heartbeat.
- Per-connection bookkeeping no longer grows over a long session.
- The heaviest payload — the graph series and the drop journal — travels only
  while the Statistics section is on screen, and nothing is pushed at all while
  the dashboard is minimised.

### Install

- The Windows installer carries its own artwork and a welcome page that says
  what the app is, what Npcap is for and that nothing leaves the machine.
- When Npcap is missing it offers to download the official installer from
  npcap.com and run it. Npcap is still not bundled: its free edition may not be
  redistributed inside another installer.

### Linux

- **The app runs on Linux.** It builds there, its tests pass there, and the
  release now carries a `.deb` and an AppImage beside the Windows installer.
  This is the first Linux build — it has not seen as much play as the Windows
  one, so oddities are worth reporting.
- Capture needs `cap_net_raw`: the `.deb` grants it on install, an AppImage
  needs one `setcap` line by hand.
- Settings, carried totals and custom sounds live in `$XDG_CONFIG_HOME/hs-tracker`
  there, and autostart is a `.desktop` entry. On Windows nothing moves — the
  folder beside the executable stays portable.
- The overlay needs X11. Under Wayland the dashboard works, but click-through,
  window positioning and global hotkeys are not available to an application.

### Removed

- The session history file (`sessions.json`). Nothing ever read it back.
- The per-rarity magic-find column in the loot table; the flag still marks drops
  in the timeline and the counters in the compact overlay.

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

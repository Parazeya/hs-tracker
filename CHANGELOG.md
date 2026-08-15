# Changelog

## 0.9.8 — 2026-08-15

### Added

- Bosses and chests counted for the session, and kept with every run.
- Pause: by hand from the clock, the tray or `Ctrl+Shift+P`, and by itself after
  five quiet minutes. The overlay ices over while it is held.
- Magic find, level and hero level, live from the client's heartbeat.
- A flourish over the screen for the drops worth one, drawn with the game's own
  effects. Its own window: place it, size it, time it, shade it. Off by default.
- **Copy card** in Runs — a session as a picture, on the clipboard.
- An **Ebontharn** skin in Settings: the season's palette, its sprites, and its
  sky behind the dashboard.
- The dashboard now says why the numbers are not moving.

### Fixed

- Linux with an NVIDIA card: the app came up as a tray icon and no window.
- The overlay did not grow when a row was added to it; it measures itself now.
- The overlay could lose always-on-top across a hide and show.
- The minimize button was drawn by hand and did not follow the skin.

### Changed

- The README is for players now; the rest moved to `DEVELOPING.md`.

## 0.9.7 — 2026-08-14

What a session was worth, kept after it ends — and told to Discord while it
is still going.

### Added

- **Runs.** A new dashboard section keeps what each session amounted to: when
  it was, how long it ran, gold, xp, kills and their per-hour rates, drops by
  rarity, the finds it produced, and where the time actually went — the rooms
  the character stood in, longest first. A run is filed when the session ends:
  the Reset button, the tray, `Ctrl+Shift+R`, the game closing, or the app
  quitting. Sessions under a minute and ones where nothing was earned are not
  runs and are dropped, so the list stays worth reading. The last 200 are kept
  in `runs.json`, and the section can clear them.
- **A Discord status.** Switch it on in Settings and, while Hero Siege is
  running, Discord shows the run under your name: the zone and difficulty, the
  SS-grade drops so far with Angelic and Unholy named separately, the gold
  earned, and a timer counting the session. It goes up when the game does and
  comes down when the game closes, so the profile never advertises a run that
  ended hours ago. The tracker speaks to the Discord client on the same machine
  through its local pipe — there is still no server of ours anywhere — and the
  character's name is never sent. Off by default.

### Changed

- **A new icon**, drawn in the game's own pixels rather than borrowed from it:
  HS on the panel plate, standing on a pile of the game's gold. It is designed
  at 16×16 — the size a taskbar actually shows — and every larger size is the
  same grid with bigger squares, so it never blurs. The installer's artwork is
  drawn from it and follows along.

## 0.9.6 — 2026-08-14

Linux, tested on a real desktop rather than a build log.

### Linux

- **The overlay works on Wayland after all.** A Wayland application may not
  float above another program's fullscreen window, so the app starts there as
  the dashboard alone — but Settings now offers **Enable the overlay — restart
  through XWayland**, which relaunches the app on the X11 backend where the
  whole thing works, hotkeys included. Hero Siege runs through XWayland too when
  it runs through Proton, so the two meet in one X server. The choice is
  remembered: every later start comes up the same way, and a second button
  switches back to native Wayland.
- Where the overlay cannot exist, the settings that only steer it — opacity,
  scale, show-with-game, the drop ticker, the overlay sections — are hidden
  instead of sitting there doing nothing, and the tray greys out the two
  overlay entries.
- The `.rpm` is built on Fedora now, in a container of its own. Built on Ubuntu
  it asked for `libpcap.so.0.8`, a name Fedora does not use, and the app died on
  startup with a missing library.
- Sound alerts and the mail reminder keep working in dashboard-only mode.
- Closing a window from the desktop's own title bar hides it to the tray instead
  of destroying it — on Wayland the dashboard is the only face there is, and a
  destroyed one could not be brought back.

### Fixed

- The overlay came back centred instead of where you left it. Hiding a window
  unmaps it, and a window manager is free to place it afresh; the position is
  now remembered across hide and show, and only ever restored onto a screen that
  is still there. Windows never had the problem.
- The overlay appearing with the game no longer takes the keyboard away from it.
- Dropdowns were drawn as a pale native widget with a blue focus ring on Linux.
  They are ours now, arrow and all.
- Sliders looked different on every platform — the rail and the handle are drawn
  by us instead of leaning on `accent-color`.

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
  release now carries a `.deb`, an `.rpm` and an AppImage beside the Windows
  installer. This is the first Linux build — it has not seen as much play as
  the Windows one, so oddities are worth reporting.
- Capture needs `cap_net_raw`: the `.deb` and the `.rpm` grant it during
  installation, an AppImage needs one `setcap` line by hand.
- Settings, carried totals and custom sounds live in `$XDG_CONFIG_HOME/hs-tracker`
  there, and autostart is a `.desktop` entry. On Windows nothing moves — the
  folder beside the executable stays portable.
- **Wayland runs the dashboard alone.** The overlay wants click-through
  windows, window positioning and global hotkeys, and a Wayland application
  gets none of them — so on such a session the app does not create the overlay
  or the drop ticker at all, skips the hotkeys, and hides the settings that
  only steer them, instead of offering things that quietly do nothing.
  Tracking, alerts and every panel are unchanged. An X11 session still gets the
  overlay; so does forcing `GDK_BACKEND=x11`.

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

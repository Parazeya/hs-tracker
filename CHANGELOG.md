# Changelog

## Unreleased

### Added

- A third skin, **Modern**: the app with the game's art taken off it. Flat
  surfaces, hairline borders and the system's UI font on Tailwind's zinc ramp —
  the neutrals shadcn/ui is built from — for reading rather than for looking at.
  Every window follows it, the overlay and the loot pillar included.
- `tools/preview/` — every window in a browser, with a mocked backend, so a skin
  can be looked at without building the Rust side or running the game.

## 1.0.5 — 2026-08-31

### Fixed

- The Linux AppImage draws again on Fedora, RHEL's family and Arch.
- Min tier, Size and Shading answer the mouse again ([#9]).

[#9]: https://github.com/Parazeya/hs-tracker/issues/9

## 1.0.4 — 2026-08-30

### Added

- A Watchlist tab, and the custom filter moved into it.
- A whole rarity or item type added to a list at once.
- Relic alerts, picked by search.
- Bosses this session, on the overlay.

### Changed

- Relics have a chime of their own.

### Removed

- The magic-find readout, from the overlay, Statistics and Runs.
- Where it happened, from Runs.

### Fixed

- 183 names get back the drop chance the game states.
- Each Essence Vault shows its own drop chance.
- A drop chance the game does not state reads as a dash.
- A switched-off list no longer counts as the winner of a conflict.
- Two item categories no longer both read Flask.

## 1.0.3 — 2026-08-28

### Added

- A custom filter's list can name one Essence Vault of the seven, or either
  Angel, spelled `Essence Vault (Angelic)`. The bare name still means any.

### Fixed

- An Essence Vault drop arrives named rather than nameless.

## 1.0.2 — 2026-08-28

### Fixed

- Rarity and grade come from the item's identity rather than its name where the
  two disagree ([#4]) — nineteen identities: Angel, Justice, Essence Vault,
  Shrunken Head, Death's Scythe, Dirge, Satan's Horn and the mana potions.
- A name two items answer to reads Unknown instead of taking the packet's word.

### Added

- A capture can be replayed through the parser:
  `HS_CAPTURE=... cargo test replay_a_capture -- --ignored --nocapture`.

[#4]: https://github.com/Parazeya/hs-tracker/issues/4

## 1.0.1 — 2026-08-27

### Fixed

- Somebody typing "mailbox" in chat rang the mail chime.
- Mail already in the box announced itself as new.
- The panel came out squished on some machines ([#3]). It is measured on both
  axes now, 444 px is a floor, and the icon strip follows the edge.

[#3]: https://github.com/Parazeya/hs-tracker/issues/3

## 1.0.0 — 2026-08-26

### Fixed

- An item a friend gave you counted as a find.
- A quest's reward counted as a find before the quest was started.
- An item listed on the trade board, or taken back off it, counted as a find.
- Dropping something you were wearing and picking it up again counted as a find.
- The merchant's window read as a floor full of loot where the game spells the
  ownership marker `gid` rather than `gd`.
- Gold could be counted twice across a Reset, and a return counted as income.
- A message hidden behind a stray brace is no longer thrown away.
- The renderer was turned off on machines that never had trouble.
- A game at its login screen is no longer reported as a fault.
- "Delete application data" deletes this application's data — everything beside
  the executable included.
- The per-item alert list no longer offers the 476 ordinary bases, whose alerts
  cannot fire.
- A flask counts towards its grade column as well as its rarity.

### Changed

- The packet log is capped at 64 MB with one older copy kept.
- The item tables refuse to be built without the game.
- Plain place names are out of the zone-code table, and chase odds no longer
  need a zone code — 31 items have their odds for the first time.
- The release workflow builds and no longer publishes. `npm run publish`
  refuses a dirty tree or a HEAD that is not its tag, and overwrites an asset
  only with `--replace`. `npm run ship` names the next command.
- The README says which distributions each Linux package is for, glibc floor
  included; `docker/Dockerfile.fedora` records how its base sets that floor.
- A vial is called a flask, and a grade a tier.
- The custom filter's heading says a list adds a voice rather than replacing
  the rarity alerts.

## 0.9.96 — 2026-08-25

### Fixed

- An invisible window on machines with no NVIDIA card. The DMA-BUF renderer is
  now turned off per machine after a start that drew nothing — delete
  `soft-render` beside the settings to let it try again.
- Capture was attempted on Bluetooth, netfilter and D-Bus devices. Those are
  left alone, and anything else with unreadable framing is set aside quietly.
- The `.rpm` is built in a Fedora container, and the build script refuses to
  bundle one anywhere else.

## 0.9.95 — 2026-08-23

### Fixed

- The capture filter named this machine's own address. It names the game's
  servers and stops there.
- An adapter with no addresses was passed over.
- The installer no longer downloads Npcap itself. It says what is missing,
  opens the page, and says which box in Npcap's installer not to tick.

### Added

- **Read every connection, not just the game's** — a setting under More
  settings, which also drops the assumption that port 443 is not worth reading.
- A banner after ninety seconds with the game running and nothing decoded.

### Changed

- The log lists the adapters found and the ones passed over, counts frames past
  the filter apart from frames decoded, and says how many were encrypted.

## 0.9.94 — 2026-08-22

### Fixed

- Turning on the loot pillar froze the whole app.
- Npcap installed with "Restrict driver's access to Administrators only" was
  reported as not installed.
- Split messages were thrown away, and tails larger than 8 KB refused outright.
- Gold earned could read zero for a whole session, in the run history too.
- A room carried over a Reset started no clock, and the rates graph doubled
  back on itself.
- Fifteen items were announced as something else, eleven names belonging to two
  items each.
- "Show it in the folder" opened the wrong folder on every install.
- Closing the dashboard on a session with no tray left a process running with
  no way back.
- A window that fails to start says so instead of leaving an invisible
  rectangle, and a panel that throws no longer takes the sidebar, Compact mode
  and the close button with it.

### Changed

- The drops panel follows the Satanic Zone rather than the room; where you are
  is shown as the act.
- The zone is marked **unconfirmed** once the rotation has come round since the
  game last asked the server.
- Act 9 has drop locations, read from the game's own words: 27 items moved, 5
  gained a location. Chase rates are computed from the rate the game states.
- Every command that touches a file runs off the interface thread.
- On Windows the log opens with which Windows, which WebView2, and where the
  app is installed.

## 0.9.93 — 2026-08-18

### Changed

- The item tables come out of the game — `Hero_Siege.exe` and
  `translationsItem.csv` — rather than a datamining site. `npm run items` does
  the whole job, and brings 12 items the old source never had.
- The **Items** page has a card view beside the table and remembers which you
  used.
- Machine paths moved into `.env`; `.env.example` documents them.

### Added

- A chime and a sweep across the zone chip when the Satanic Zone rotates, on by
  default, with its own row on Alerts. It fires on a rotation and nothing else.
- `npm run all` builds every artifact: installer, `.deb`, `.rpm`, AppImage.

### Fixed

- A buffer of open braces could stall the capture.
- A ground drop and its pickup counted twice when they shared a fingerprint.
- A reset between a deposit and its balance let the new session claim gold.
- Time in a zone accrued while the session was paused.
- A settings file that would not parse is set aside as `settings.json.bad`, and
  writes are staged and flushed before the rename.
- A single left click on the tray icon toggled the window twice.
- The SS chip counted keys and socketables. The grade columns count gear.
- Essence Vaults no longer sound a rarity alert.
- The "waiting for Hero Siege" banner pushed pages past the bottom of their
  box, and the Alerts page could draw its controls on top of one another.
- The statistics payload was pushed every second whether or not anything had
  changed, and the journal formatted its timestamps one at a time.

## 0.9.92 — 2026-08-18

### Changed

- Sounds and the sound filter are one page, **Alerts**, in two columns — one
  row per rarity instead of the same question asked in two tabs.
- The drop announcement moved there and arrives switched on: all five
  rarities, any grade, half volume, a fifth of the screen below centre.
- The announcement can follow the custom filter: anything on a list is
  announced whatever its rarity.
- Settings hides what is set once behind **More settings**, which remembers
  being opened.
- The OBS browser sources are gone, and with them the local server, the port
  and the four addresses.

### Added

- A chime and a sweep on the zone chip when the Satanic Zone rotates — one row
  on **Alerts**, on by default, with its own volume and file.
- Export every setting to one file and read it back, sound files included.

### Fixed

- A single left click on the tray icon toggled the window twice.
- The SS chip counted resources; the grade columns count gear.
- Essence Vaults no longer sound a rarity alert. They are still journalled and
  counted.

## 0.9.91 — 2026-08-17

### Fixed

- A named item's grade comes from the item tables in every case; the packet is
  consulted only for items they have never heard of.
- Ordinary pickups counted as Angelic. The claim is refused where it is
  impossible — an ordinary base cannot be Angelic or Unholy — which takes 52
  false Angelics off a full session and leaves the one real one.
- The Reset and Quit buttons could fire on an ordinary double-click, and stayed
  armed through a lock.
- Locking the overlay from its own strip left the strip lit but dead.
- The game's own minimize plate had no hover frame.
- On an adapter that does segmentation offload, no message longer than one
  frame arrived — which took experience and kills to zero for the session.
- Magic Find was drawn at 2.4:1 contrast, and in the dashboard was not blue at
  all; the Satanic counters at 2.9:1. Both are palette tokens now.
- The drop ticker's plates were 12px wider than the overlay on each side.
- The Reset button was 18px narrower than its column.
- The lock corner's click-through rectangle overlapped the button below it.
- Putting an item on the auction house counted as a fresh drop.
- The OBS browser sources never announced anything, and the drop ticker had no
  address; Settings lists four now.
- Deleting a filter or a list gave no sign that the first click had armed it.
- The rates graph drew gold and experience in the same colour, on black.
- Clicking the tray icon hid a minimised dashboard instead of restoring it.
- The session clock started when the app did, not when the game did.
- Placing the drop announcement showed one sample and then an empty box, with
  its dashed frame painted over it.
- "Restart through XWayland" killed the copy it started.
- State files were truncated in place. Everything is staged and renamed, and a
  failed write says so.
- A request to the OBS server with a bracketed IPv6 host was refused.
- A failing Fedora job withheld the `.deb` and the AppImage that had built.

### Changed

- The overlay is a row shorter: character level, hero level and the two purse
  totals are gone, Magic Find keeps the top row, and the kill counter is always
  on screen.
- The right-click menu is gone. Dashboard, Hide to tray, Reset and Quit are a
  column of buttons beside the overlay, the lock at its top. Reset and Quit
  still ask twice.
- The Reset button left the panel's rows and the session's SS count ends that
  row instead. Resetting is on the new column, in the tray, and on
  `Ctrl+Shift+R`.
- New mail blinks the chip for twenty seconds and then stays gold until it is
  collected.
- The capture probe is asked once a minute rather than several times a second,
  and the endpoint sweep moved onto the five-second beat.
- Repeated warnings are written once, then only on a change or after ten
  minutes.

## 0.9.90 — 2026-08-16

### Fixed

- The app aborted on X11 with the drop announcement on and the overlay locked,
  and saved the setting before applying it.
- A missing tray library killed the app before it had a window.
- Capture rights were not checked until the game ran.
- The capture wrote nothing to the log.
- The status went green on every attempt, including failing ones.
- "The game's traffic is not reaching us" fired in the first minute.
- The game was found by process name alone.
- Its address arrived IPv4-in-IPv6, building a filter nothing could match.
- A VPN reconnect took that adapter down for five minutes.
- Window positions saved on Wayland were always (0, 0).
- The tray's Dashboard entry did nothing after the window was minimised.
- The drop ticker sat across the overlay and lost its always-on-top when hidden.
- Custom alert sounds could not be served over the asset protocol.
- The drop announcement could not be switched on without an overlay.
- The OBS server filled the log and answered a request with no `Host` header.
- Copy buttons said "copied" whether or not it worked.
- Every "open the folder" left a zombie process behind.
- With no `HOME`, everything the app writes went to a directory never created.
- The overlay appeared in Alt-Tab.
- The postinstall script hid its own failure and asked for more rights than it
  uses.
- `npm start` and `npm test` were Windows batch files.
- Two copies could run at once, each with its own sniffer.
- On Linux the old frame stayed under the new one; the panel is painted solid.

### Added

- The interface says when it has painted, and after 20 seconds of nothing the
  log names the two variables worth trying.
- One log line per session: display server, toolkit backend, desktop, driver.
- `npm run deb` builds the Linux packages in a container, against an older
  glibc than the host's.
- Settings: **Enable transparent overlay while locked**, on by default only
  where it does not smear.

### Changed

- The automatic pause after five quiet minutes is gone; the pause by hand stays.

## 0.9.89 — 2026-08-16

### Added

- Bosses and chests counted for the session and kept with every run.
- Pause: by hand from the clock, the tray or `Ctrl+Shift+P`, and by itself
  after five quiet minutes. The overlay ices over while it is held.
- Magic find, level and hero level, live from the client's heartbeat.
- A flourish over the screen for the drops worth one, drawn with the game's own
  effects, in its own window: place it, size it, time it, shade it. Off by
  default.
- **Copy card** in Runs — a session as a picture, on the clipboard.
- An **Ebontharn** skin: the season's palette, its sprites, and its sky behind
  the dashboard.
- The dashboard says why the numbers are not moving.
- Errors are written to a log beside the settings — panics with a backtrace,
  and anything a panel throws. About says where it is and opens the folder.
- **OBS**: the overlay window is named apart from the dashboard, and the
  overlay, the dashboard and the drop announcement are served on `127.0.0.1`
  for a Browser Source. Off by default; addresses and size in About.
- An **Items** section: every named item with its chance anywhere, its better
  chance where it is tied, and where that is. Search by name, rarity or kind.
- An **About** section: the version, who made it, and a check for a newer
  release — the only request the app makes, and only on the button.

### Changed

- The overlay's loot chips show the count alone, and are the width of every
  other chip.
- The scrollbars are the app's own rather than the system's.
- The OBS addresses moved next to the switch that serves them; the capture
  instructions are in the README.
- The README is for players; the rest moved to `DEVELOPING.md`.

### Fixed

- Linux with an NVIDIA card: the app came up as a tray icon and no window.
- The overlay did not grow when a row was added; it measures itself now.
- The overlay could lose always-on-top across a hide and show.
- The minimize button was drawn by hand and did not follow the skin.
- Ordinary items counted as rare ones — 35 of 38 ordinary pickups in one run
  as Satanic. The tables are asked only about an item the game has flagged as
  named, or one whose packet already says it is rare.
- **Odyssey** runs counted every pickup as Angelic. Their drops are counted
  without a rarity instead.
- Everything on a custom list chimed twice. One item, one alert, whichever
  sighting comes first.
- **Copy** made a filter whose lists were all mute; it copies the sounds.
- Choosing a new sound removed the old one before the new one was in place.
- The second click of a two-click delete went to whatever was selected then.
- **Test** played the file of one list at the volume of another.
- Another player's find set off your alerts. Only your character's finds count.
- The log path in About was unreadable in the game's typeface. Paths and
  addresses are set in a plain monospace.
- Browse, Import and Export froze the whole app until the dialog was answered.
- An imported filter said its lists had no sound while their files were on disk.
- Ending a run disarmed the drop announcement. The settings travel in one piece.
- A drop that only the announcement wanted also played the alert sound.
- **Least grade — D** now means every drop of that rarity, graded or not.
- The bank showed nothing until the game next saved.
- Switching the drop announcement off and straight back on, then placing it,
  froze the app.
- Placing the drop announcement could leave the app unusable. It centres
  itself, is plainly a box, takes the keyboard, ends on Escape, and ends by
  itself after three minutes.

## 0.9.7 — 2026-08-14

### Added

- **Runs.** A dashboard section keeping what each session amounted to: when it
  was, how long it ran, gold, xp, kills and their per-hour rates, drops by
  rarity, the finds it produced, and the rooms the character stood in, longest
  first. Filed on Reset, the tray, `Ctrl+Shift+R`, the game closing or the app
  quitting; sessions under a minute are dropped. The last 200 are kept in
  `runs.json`, and the section can clear them.
- **A Discord status**, off by default. While Hero Siege runs, Discord shows
  the zone and difficulty, the SS-grade drops with Angelic and Unholy named
  separately, the gold earned, and a session timer. It speaks to the Discord
  client through its local pipe, and the character's name is never sent.

### Changed

- **A new icon**, drawn in the game's own pixels: HS on the panel plate,
  standing on a pile of gold. Designed at 16×16, every larger size the same
  grid. The installer's artwork follows from it.

## 0.9.6 — 2026-08-14

### Linux

- **The overlay works on Wayland after all**, through Settings → **Enable the
  overlay — restart through XWayland**. The choice is remembered, and a second
  button switches back to native Wayland.
- Where the overlay cannot exist, the settings that only steer it are hidden
  and the tray greys out its two entries.
- The `.rpm` is built on Fedora, in a container of its own.
- Sound alerts and the mail reminder keep working in dashboard-only mode.
- Closing a window from the desktop's own title bar hides it to the tray.

### Fixed

- The overlay came back centred instead of where you left it, and is only ever
  restored onto a screen that is still there.
- The overlay appearing with the game no longer takes the keyboard from it.
- Dropdowns were a pale native widget with a blue focus ring on Linux.
- Sliders looked different on every platform; the rail and handle are ours.

## 0.9.5 — 2026-08-13

### The dashboard

- **One window instead of three.** Statistics, Shopping List and Settings are
  sections of a resizable dashboard with a sidebar, joined by Sound Filter and
  Sounds.
- **Compact mode** folds the dashboard into the overlay, and the overlay's
  right-click menu has **Dashboard** to come back. Which was up last is
  remembered.
- The dashboard is an ordinary window: taskbar, not pinned above the game,
  dragged by any empty spot, eight resize edges, size and position remembered.
- A minimize button beside the close cross; closing hides to the tray.
- The tray menu: **Dashboard** and **Compact overlay** replace the three window
  entries.
- Resetting the session asks once — the button turns into **Sure?**.

### Sound filters

- **Lists of specific items, each with its own sound**, which play even where
  the rarity switches and the minimum grade would have kept quiet. A list with
  no file borrows the rarity's.
- **Filters are packs of lists**, switched from a dropdown, with New, Copy
  (sounds included), delete, and one master switch.
- **Generate** builds a filter from the drop rates: S and SS gear by rarity,
  cut into Common, Rare and VeryRare bands, Angelic and Unholy on their own.
- **Import… / Export…** move a whole filter as one file, sounds embedded.
- Search shows grade and odds in short form (`1/576k`, `1/1.3M`); Enter adds
  the top hit. Lists reorder with arrows, first match wins, and an item in two
  lists gets a `?` naming the other.
- Rarity alerts and the minimum grade head the Sound Filter section; the six
  per-rarity sounds moved to a Sounds section.

### Statistics

- Rebuilt as an overview: the run across the top, loot and the item timeline on
  the left, the Satanic Zone, the area panel and the rates graph on the right.
- **Drops in this area** names the zone (`Act 8 · Zone 2`) and lists what rolls
  better there at the chance that applies in the zone.
- The loot counters became a table with `drops` and `per hour` columns; notable
  finds and resources read as tallies underneath.
- Every row in the drop timeline has a **+** that adds the item to a list.
- The XP tile also shows `in level`.
- Totals carried from the previous run are marked `*` until confirmed.
- The rates graph is drawn at the window's real size and pixel density.
- The Keys counter ignores Basic and Crystal keys.

### Tracking

- **Gold read the wrong purse when a new season opened.** The character's own
  season decides now.
- The status line no longer claims to be capturing when every adapter failed to
  open, and a refusing adapter is retried every five minutes.
- A device that cannot be opened for want of permission is reported as such.
- The current zone is read from the client's own heartbeat.
- Per-connection bookkeeping no longer grows over a long session.
- The graph series and the drop journal travel only while Statistics is on
  screen, and nothing is pushed while the dashboard is minimised.

### Install

- The Windows installer carries its own artwork and a welcome page saying what
  the app is, what Npcap is for, and that nothing leaves the machine.
- When Npcap is missing it offers to download the official installer and run
  it. Npcap is not bundled.

### Linux

- **The app runs on Linux**: it builds there, its tests pass there, and the
  release carries a `.deb`, an `.rpm` and an AppImage.
- Capture needs `cap_net_raw`: the `.deb` and `.rpm` grant it on install, an
  AppImage needs one `setcap` line by hand.
- Settings, carried totals and custom sounds live in
  `$XDG_CONFIG_HOME/hs-tracker`, and autostart is a `.desktop` entry. On
  Windows nothing moves.
- **Wayland runs the dashboard alone** — no overlay, no drop ticker, no
  hotkeys, and the settings that only steer them are hidden. An X11 session,
  or `GDK_BACKEND=x11`, still gets the overlay.

### Removed

- The session history file (`sessions.json`).
- The per-rarity magic-find column in the loot table.

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

- Gold, experience and kills with per-hour rates, arriving in steps as the game
  saves; Statistics says how long ago that was.
- Totals carry over a restart in `carried.json`.
- Item counters for Satanic, Set, Heroic, Angelic and Unholy, with magic-find
  splits and resource counters for keys, materials, socketables and
  collectibles.
- Items resolved to their real names from (type, id, weapon type), with rarity
  and grade from datamined tables.
- Notable drop counters — Angelic Key, Satanic Key, Satanic Dice, S and SS
  runes — configurable in `settings.json`.
- Satanic Zone with pros, cons and a countdown to the half-hour rotation.

### Alerts

- Separate sound per rarity plus a mail reminder, with volume, preview and
  custom files.
- Alerts fire when an item is rolled onto the ground, not when it is picked up;
  the same item never chimes twice, and finds announced in chat always sound.
- Rarity switches and a minimum grade (D..SS) decide what is announced;
  counters keep recording everything.
- Fading drop ticker under the overlay showing item names.

### Windows

- Statistics: rarity cards, notable drops, gold/h and xp/h graph, drop timeline.
- Shopping list: entries copy to the clipboard on click.
- Settings with everything above, plus a packet log for diagnosing the parser.

### Capture

- Listens on every adapter and keeps the ones the game talks over, so a VPN,
  split tunnelling or a second NIC changes nothing. Adapters that produce
  nothing are dropped and retried later.
- Reassembles messages per TCP connection and flushes on a pause.
- Counters and windows are pushed from the backend when something changes, and
  only to windows on screen.

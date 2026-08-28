# Changelog

## 1.0.2 — 2026-08-28

### Fixed

- **Items that are not Angelic were announced as Angelic finds** ([#4]).
  Shrunken Head is a Satanic charm and Angel a Set gun, and both came up green
  in the journal with the Angelic chime. Neither name was in the rarity table,
  and for a reason: eleven names belong to two items each — the charm to a
  Common relic of the same name, the gun to a Heroic orb the game also calls
  Angel — and a name two items disagree about was dropped rather than answered
  wrongly. The note beside that said the packet's own claim would stand
  instead, "which is right for both of them". It is not. Over one 45-minute
  capture the packet's rarity field says 8 on 155,459 of 171,295 items and
  takes four other values between them, every one of which is wrong about the
  item carrying it; on the reporter's session it said 7, which reads as
  Angelic.

  A drop packet does not have the problem the tables do. It names the exact
  item — `(type, id, weaponType)` — and the parser already reads that triple to
  get the name in the first place; only the name was passed on. It is now
  passed on whole, and the nineteen identities the name tables cannot answer
  for are answered by it. In the same capture that is 15 finds named Angel that
  are the Set gun, 5 that are the Heroic orb, and 23 named Justice that are the
  Common tarot card, each told apart by the triple its own packet carried.

  A find announced in the chat line still carries a name and nothing else, and
  for the three names two of the five answer to — Angel, Essence Vault and
  Justice — it now reads Unknown rather than taking the packet's word. Unknown
  is a plain answer where Angelic was a wrong one.

  Two things follow from the same change. Where only one claimant is one of the
  five, that one settles the name: Shrunken Head is Satanic and Death's Scythe
  is Set, and their grades follow the same claimant rather than the relic's.
  And picking up the Common relic that shares one of those names no longer
  borrows the weapon's rarity and grade — the identity says which one it is.

### Added

- A capture can be read back through the parser. `lib.rs` has written
  `debug-capture.jsonl` since the beginning so that a session could be replayed
  "when counters look wrong", and nothing ever replayed it; every check was a
  packet built by hand. `cargo test replay_a_capture -- --ignored --nocapture`,
  with `HS_CAPTURE` pointing at one, prints every named find with the rarity
  and grade the counters would give it, and what the packet claimed instead.

[#4]: https://github.com/Parazeya/hs-tracker/issues/4

## 1.0.1 — 2026-08-27

### Fixed

- **Somebody typing "mailbox" in chat rang the mail chime.**
- **Mail already in the box announced itself as new.**
- **The panel came out squished on some machines** ([#3]). The window was a
  fixed 444 CSS pixels wide and the chips inside it fixed widths that do not
  wrap, so wherever the text drew wider — a substituted font, a system text
  size, a webview's minimum font size — it had nowhere to go and spilled across
  the row. No setting could help, because none of them move a CSS pixel. The
  panel is now measured on both axes instead of one: 444 is a floor, the window
  follows the content above it, and the icon strip beside it moves with the edge
  rather than remembering where it used to be.

[#3]: https://github.com/Parazeya/hs-tracker/issues/3

## 1.0.0 — 2026-08-26

The number is the largest thing in this release. What stands behind it is the
week before it: three releases spent finding out why a handful of players saw
nothing counted at all, each one ending at a cause that was ours and not theirs.

### Fixed

- **An item a friend gave you counted as a find.** Picked off the floor where
  they dropped it, a Torch of Shadows was announced, chimed and journalled as
  though it had fallen for you — and from the packet it is indistinguishable
  from one that had: named, flagged, entering your bags the same way. The one
  thing that separates them is the account the game made the item for, which it
  writes into the fingerprint and the item then carries for life. Across four
  captures, 999 named things entered these bags: 985 were made for this account
  and fourteen were not, and not one of the fourteen had ever been seen falling.
  Five of them arrive one after another from a single account, which is what
  being handed a set of gear looks like from the outside. Nothing is refused
  until the client has said which account it is.
- **A quest's reward counted as a find before the quest was started.** Walking
  into a zone that pays a named item has the client ask the server to make the
  item on the way in, and the answer is a drop answer in every respect — one
  named item, nobody's slot, an `itemGenHash` — so a Mana Bender's Will was
  announced, chimed and journalled at the door. It was never in the world: it
  waits in the save until the quest is finished. What separates the two is that
  a thing in the world says where it is, and a reward waiting to be earned says
  nothing. Across six captures — the five kept in this repository and the one
  from the machine that reported it — a server's answers carry 5,098 named
  items: 4,137 say which spot in the world they are lying on, 943 say whose slot
  they sit in, and eighteen say neither. Seventeen of those eighteen are the
  trade board above. The eighteenth is this.
- **An item taken back off the trade board counted as a find.** The server
  answers a removal from the market with the item in full — named, identified,
  carrying the flag that marks something worth announcing — which is the shape of
  a drop answer and was read as one. Listing an item sends the mirror of it. A
  player who put two pieces of gear up for sale and changed his mind had both
  announced, chimed and written into the journal as though they had fallen at his
  feet. A market message carries a price, a seller or a market id and an answer
  from the floor carries none of those: of the 16,410 messages in one capture
  that hold an item, exactly four hold any of them, and all four are his.
- **Dropping something you were wearing and picking it back up counted as a
  find.** Both halves are ordinary inventory operations, and the second is
  indistinguishable from a real drop down to the named flag — a Pendant of
  Eternity and a pair of Tectonic Grips, worn for hours, announced as new the
  moment they came off the ground. The fingerprint the game gives an item
  survives the round trip unchanged, so what has just left your bags is
  remembered and not counted when it comes back. Once per time it was put down:
  finding another one later still counts.
- **The merchant's window went back to reading as a floor full of loot.** The
  guard that tells a shop slot from a drop looked for the ownership marker under
  `gd`, and the game also spells it `gid` — three captures kept in this
  repository disagree about which, one carrying it under `gid` 28 times while
  `gd` holds a plain position instead. On the patches that say `gid`, opening the
  Black Market poured its whole stock into the journal as finds at your feet,
  which is the exact flood that guard was written to stop. It knows both
  spellings now, and a test says so.
- **Gold could be counted twice across a Reset, and a return could be counted as
  income.** A deposit and the balance that confirms it arrive in either order and
  the engine cancels the two against each other — but only one half of that pair
  survived a Reset, so a Reset landing between them left the survivor with
  nothing to cancel and the coins counted again. The high-water mark that stops a
  bank drawn down and put back reading as earnings did not survive either.
- **A message hidden behind a stray brace is no longer thrown away.** An opener
  in binary framing looks exactly like a truncated message, so it is carried and
  given up on after three flushes — and giving up used to mean dropping
  everything the brace was holding, a real drop answer among it.
- **The renderer was being turned off on machines that never had trouble.** A
  start that hands over to a replacement — which is what choosing the X11 backend
  does — left behind the mark meaning "this run drew nothing", so the next start
  disabled the DMA-BUF renderer and wrote a reason that was not true.
- **A game at its login screen is no longer reported as a fault.** Everything a
  client sends before it joins a server is encrypted, which is indistinguishable
  from a broken capture unless the two are counted apart. The dashboard said
  nothing had been counted for ninety seconds and offered a setting that would
  not have helped.
- **"Delete application data" deletes this application's data.** The box clears
  the two folders a Tauri app usually writes to; this one keeps everything beside
  its executable, so the settings, the run history, the imported sounds, the log
  and the packet capture — which holds an account id and a character name — all
  survived an uninstall that reported success.
- **The per-item alert list stopped offering items whose alerts cannot fire.** It
  kept anything with a rarity, and every item has one since the tables were
  rebuilt, so 476 ordinary bases were on offer. Putting one on a list did nothing
  at all, silently: the game never names an ordinary pickup.
- **A flask counted as a find but never as a tier.** Flasks are equipped, the
  same as charms, and were on neither the gear list nor the stackable one — so a
  Heroic flask went into the Heroic column while the SS column, which counts the
  tier of gear, never saw it. The panel showed twelve Heroic beside ten SS with
  nothing on screen to explain the gap. Every Heroic item in the tables is
  graded SS, so a Heroic that was not also an SS could not have been anything
  else.

### Changed

- The packet log is capped at 64 MB with one older copy kept. It had no ceiling,
  and with **Read every connection** on it is no longer only the game's traffic
  being written down.
- The item tables refuse to be built without the game. A missing folder used to
  produce tables with no rooms and a season-old act table, print a success line
  and exit 0. Plain words like "Sheeponia" no longer go into the table of zone
  codes either — nothing could ever match them, and `dropPlaces` already holds
  them properly. How often a thing drops in its own zone no longer depends on
  that table: it needed a zone code to be written down at all, so taking the
  words out took the odds off everything that drops in Sheeponia with them —
  the Sheep King's three, Steve's five, Loaded Dice, aimbot.exe. Knowing the
  place by name is enough, which is also how thirty-one items that never had
  their odds shown — the dice, the abomination's three pieces, Noxus — have
  them now.
- **The release workflow builds and no longer publishes.** Its trigger was removed
  a while ago without its publishing steps, and dispatch is allowed on a tag — so
  it could still fire as a second writer on a release `npm run publish` had cut,
  replacing tested assets with runner-built ones. `npm run publish` now refuses a
  dirty tree or a HEAD that is not the tag it publishes under, and overwrites an
  existing asset only when asked with `--replace`. `npm run ship` names the next
  command instead of a page that would stay empty.
- The README says which distributions each Linux package is for, glibc floor
  included, and `docker/Dockerfile.fedora` records how its base sets that floor.
- **A vial is called a flask**, which is what the game calls it.
- **A grade is called a tier** wherever the word is shown, which is what the
  game's own tooltip says.
- The custom filter's heading read "lists that outrank the above". That is true
  of an item on a list and was read as true of everything else, so players
  believed a filter switched the rarity alerts off and asked for the adding
  behaviour the app already had. It now says which it is: a list adds a voice,
  it does not take the others away. The behaviour did not change; a test was
  added, because the one that existed armed no rarity at all and so could not
  tell an adding filter from a replacing one.

## 0.9.96 — 2026-08-25

Linux, and one thing 0.9.95 broke.

### Fixed

- **An invisible window on a machine with no NVIDIA in it.** Since 2.40
  WebKitGTK composites through a DMA-BUF renderer, and the app has been turning
  that renderer off wherever it found the NVIDIA driver — which is where it was
  known to fail. It fails elsewhere too: on a KDE session with an AMD card it
  gave `Could not create default EGL display: EGL_BAD_PARAMETER. Aborting...`
  four times over and then the web process died, leaving a window that was up,
  transparent and drawing nothing.

  There is no way to test for that in advance, because it happens inside a
  process that has not started yet. So it is learned instead: every start leaves
  a mark behind and the first frame drawn clears it. A start that finds the mark
  still there knows the run before it drew nothing, and turns the renderer off
  on this machine from then on. The first start fails, the second works, and
  nobody has to read a log or find an environment variable. Delete
  `soft-render` beside the settings to let it try again after a driver update.

  The line the log printed about this said the renderer was worth turning off
  "on an NVIDIA card", which told the one player it could have helped that it
  was not for him. It no longer mentions a brand.

- **0.9.95 tried to capture on Bluetooth, netfilter and D-Bus.** Keeping
  adapters that have no addresses was right — it is what an adapter for VPN
  capture looks like — but libpcap lists more than networks, and those have no
  addresses either. Each got a capture thread, each failed with "link-layer type
  filtering not implemented", and the failures took the status line away from
  the adapter that was working. Opening the D-Bus one was worse: libpcap runs
  `dbus-launch` to find the bus, which inside an AppImage resolves against the
  bundled libdbus and dies with a version error in the player's log.

  Those devices are not opened now, and anything else that turns out to speak a
  framing this app cannot read is set aside after one line at a level nobody has
  to act on.

- **The .rpm could not start on the distributions it is for.** Tauri writes the
  RPM archive itself rather than calling rpmbuild, so the Ubuntu build container
  produced one without complaint — with a binary linked against Debian's
  libpcap, asking the loader for `libpcap.so.0.8`. Fedora, RHEL and openSUSE
  ship `libpcap.so.1` and never had the other name, so the package installed,
  satisfied its declared dependency and then did not run. It is built in a
  Fedora container of its own now, and the build script refuses to bundle one
  anywhere else.

## 0.9.95 — 2026-08-23

Everything here came out of players reporting that nothing was being counted —
no error, no message, every number at zero. Three separate causes, all of them
the same mistake: a rule that narrowed what gets captured, written on an
assumption that is true on the machine it was written on.

### Fixed

- **The capture filter named this machine's own address.** It came from the
  operating system's socket table, which reports the address a socket is *bound*
  to — not necessarily the one on the frames going past. Behind a split-tunnel
  VPN, or on a machine with more than one route, the two differ and the filter
  then matches nothing whatever. It names the game's servers and stops there
  now, which is what was doing the work anyway: the capture is not promiscuous,
  so the adapter only ever hands over this machine's own frames.
- **An adapter with no addresses was passed over.** The rule meant to skip
  loopback was written as "keep a device with at least one address that is not
  loopback", which is false for a device with no addresses at all — and the
  adapter Npcap offers for dialup and VPN capture has none. On a machine whose
  traffic goes through a VPN that is the one place the game could have been
  seen, and it was excluded before Npcap was ever asked.
- **The installer no longer downloads Npcap itself.** It ran PowerShell with the
  execution policy switched off to pull an executable into the temp folder and
  run it — the shape of a dropper whatever it happens to be fetching, and an app
  that reads network traffic has enough working against it with antivirus
  already. It says what is missing and opens the page instead, and says which
  box in Npcap's own installer not to tick.

### Added

- **Read every connection, not just the game's** — a setting, under More
  settings. A route optimiser such as ExitLag redirects the game's packets in a
  driver below the TCP stack, so the connections Windows reports are not the
  ones on the wire and the filter holds only addresses that never appear. This
  takes the filter off, and takes off with it the assumption that port 443 is
  encrypted and not worth reading. It is a setting rather than something the app
  decides on its own: reading everything on the machine is the player's call.
- **The app says when it has heard nothing.** Ninety seconds with the game
  running and not one message decoded puts a banner on the dashboard — with the
  switch above on it, so it can be turned on where the reader already is. When
  everything is already being read the banner says that instead of offering a
  switch that would not help.

### Changed

- The log answers the question it is asked for. It lists the adapters found and
  the ones passed over, counts frames that got past the filter separately from
  frames that decoded, and says how many were encrypted — which is what tells a
  redirected game apart from one still sitting at the login screen. Every report
  of nothing being counted has turned on facts like these and not one of them
  arrived with them.

## 0.9.94 — 2026-08-22

### Fixed

- **Turning on the loot pillar froze the whole app.** Switching the
  announcement on built its window from inside a synchronous Tauri command, and
  a window is built by the event loop — which on Windows is the very thread that
  command is running on. The two waited for each other forever: every tab went
  empty, Compact mode stopped answering, the close button stopped answering, and
  the only way out was the task manager. The setting even unticked itself
  afterwards, because the save that would have written it never returned. The
  window is now built off that thread, so no command can do this again whoever
  calls it. Reported by Mindusz and by one other player, whose settings file
  showed the one thing needed to trigger it: the pillar switched off.
- **Npcap being installed and being usable are different things.** Its own
  installer has a box marked "Restrict Npcap driver's access to Administrators
  only", and with it ticked the app was refused the adapter and reported "Npcap
  is not installed" — sending players to install what they already had. The two
  states are now told apart, and the second one says what to do about it.
- **Split messages were being thrown away.** A message cut across a TCP flush
  was only carried if nothing inside it had closed yet — but every drop answer
  has a closed item object in it long before the end, so a drop cut this way was
  never counted, never chimed and never journalled. Tails larger than 8 KB were
  refused outright, which is every large answer the game sends: the biggest is
  35 KB.
- **Gold earned could read zero for a whole session.** The mark that stops a
  balance climbing back to a level it has already held from counting as income
  survived a change of purse, so a player whose seasonal purse was empty while
  the blood pact one held a million and a half had every later penny measured
  against the wrong peak. It was persisted into the run history that way too.
- The room a Reset carried over started no clock, so the run card's "where it
  happened" was empty for a run that happened in one room. The graph's points
  were stamped with a clock that steps backwards when the idle watch back-dates
  a pause, so the line doubled back and ran off the canvas.
- **Fifteen items were announced as something else.** Rarity, grade and drop
  chance were keyed on the name the game prints, and eleven of those names
  belong to two different items that disagree: the relic "Death's Scythe" is a
  Common D and the polearm of that name is a Set S, so picking up the relic
  played the Set chime, coloured the announcement green and filed it in the Set
  column. A name two items disagree about now answers nothing, and the packet's
  own claim about the item stands instead.
- "Show it in the folder" opened the wrong folder on every install, because the
  product's own name has a space in it and the argument was quoted where
  Explorer wanted it raw.
- Closing the dashboard on a session with no tray left a process running with
  nothing on screen and no way back, and the single-instance guard then
  swallowed every relaunch.
- A window that fails to start now says so in something opaque with a working
  close button, instead of leaving an invisible rectangle that answers no click.
  A panel that throws no longer takes the sidebar, Compact mode and the close
  button down with it. Reading a remembered preference can no longer take a
  window with it either.

### Changed

- **The drops panel follows the Satanic Zone**, not the room under your feet.
  The game names the room only in its own state packet and, since the August
  patch, sends that about twenty times less often than it used to — so the panel
  sat for hours on a zone the player had left three acts ago. The satanic zone
  is announced by name over and over, and it is the one worth reading a drop
  list for. Where you are is shown as the act, which every save states.
- The zone is marked **unconfirmed** once the rotation has come round since the
  game last asked the server, rather than stating an hours-old answer as this
  hour's.
- Act 9 has drop locations at last. They were read from the game's own words for
  where things drop rather than from a datamined snapshot that predates the act:
  27 items moved, 5 gained a location, none were lost. Chase rates are computed
  from the rate the game states now instead of being carried from that snapshot.
- Every command that touches a file now runs off the interface thread, so a save
  landing while an antivirus holds the file no longer stops the window answering.
- On Windows the log opens with which Windows, which WebView2 and where the app
  is installed. A log with none of that in it is what the last freeze report
  arrived with.

## 0.9.93 — 2026-08-18

### Changed

- **The item tables come out of the game now.** Drop rates, names, rarities,
  grades and the identity triple are read from `Hero_Siege.exe` and
  `translationsItem.csv` instead of a datamining site, so a season can be
  tracked the day it arrives rather than the day someone else datamines it.
  `npm run items` does the whole job. Measured against the old source: every
  rate it can read agrees, rarity on 1552 items of 1577, grades on all 955
  named items, and the identity triple on 1861 of 1862 — the one disagreement
  is the old source's mistake. It also brings 12 items that source never had.
- The **Items** page has a card view beside the table and remembers which you
  used. A card gives the name, the kind and grade, both drop chances and where
  the item is tied, in words rather than as a fifth column.
- Machine paths moved out of the build scripts into `.env`; `.env.example`
  documents them. Nobody's drive letters travel with the repository now.

### Added

- **A chime when the Satanic Zone rotates**, with a sweep across the zone chip
  in the overlay. On by default, with its own row on the Alerts page. It fires
  only on a real rotation — not at startup, not after a reset, and not when the
  app first learns which zone is satanic. Coming back to the game an hour later
  was the case that needed the care.
- `npm run all` builds every artifact on this machine: the installer, the
  `.deb`, the `.rpm` and the AppImage.

### Fixed

- **A buffer of open braces could stall the capture.** Finding the end of a
  JSON message was quadratic — 64 KB of `{` took 2.2 seconds and a megabyte
  would have taken minutes. It now costs a fixed multiple of its own length and
  reads real traffic byte for byte the same.
- A ground drop and its pickup were counted twice when they shared an inventory
  fingerprint. A reset between a deposit and the balance that confirmed it made
  the new session claim gold it had not earned. Time in a zone kept accruing
  while the session was paused.
- A settings file that would not parse was answered with defaults, and the next
  save wrote those defaults over the only copy. It is set aside as
  `settings.json.bad` now, and what the app writes survives a power cut: the
  staged file is flushed before the rename that names it.
- A single left click on the tray icon toggled the window twice, because the
  click is reported going down and coming up. It appeared and vanished again;
  only a double click looked like it worked.
- The SS chip counted anything carrying an SS grade, keys and socketables
  included, and read 2 before a piece of gear had dropped. The grade columns
  count gear.
- Essence Vaults no longer sound a rarity alert: seven of them share one
  display name, so it fired constantly and said nothing about what was found.
- With the game not running, the "waiting for Hero Siege" banner pushed every
  page past the bottom of its box, where it was clipped with no way to scroll
  to it. The Alerts page could draw its own controls on top of one another in a
  short window. Both were the same fault, and every flex container was swept
  for it.
- The heavy statistics payload was pushed to the front end every second whether
  anything had changed or not, and the journal formatted its timestamps one at
  a time — 10 ms of every redraw.

## 0.9.92 — 2026-08-18

### Changed

- Sounds and the sound filter are one page, **Alerts**, in two columns. The
  five rarities were being configured in two tabs at once — whether they alert
  at all in one, how loud and with which file in the other. They are one
  question and are now one row each.
- The drop announcement moved there too, and arrives switched on. Off, with the
  narrowest band it had, it announced nothing whatever, which reads as broken
  rather than unset. Now all five rarities, any grade, volume at half, and a
  place a fifth of the screen below centre, where the pillar clears the fight
  instead of landing on it.
- The announcement can follow the custom filter: anything on a list is
  announced whatever its rarity. Putting an item on a list is already a
  statement that it matters.
- Settings keeps what people set and hides what they set once behind **More
  settings**, which remembers being opened.
- The OBS browser sources are gone, and with them the local server, the port
  and the four addresses. One route in: capture the announcement window, which
  can stay on screen for exactly that.

### Added

- The satanic zone rotating says so: a chime, and the zone chip sweeps and
  pulses for a few seconds. The rotation is why anyone watches that chip, and
  watching it means not watching the fight. One row on **Alerts** — *Zone
  change* — carries both, on out of the box, with its own volume and its own
  file if the satanic chime it borrows is not to taste. It fires on a rotation
  and nothing else: starting the tracker between rotations, or resetting the
  session, is this app finding out where the zone is, not the game moving it.
- Export every setting to one file and read it back — rarities, grades, the
  announcement, every filter and list, and the sound files themselves, which
  live outside settings.json and would otherwise arrive as silence.

### Fixed

- A single left click on the tray icon toggled the window twice, because the
  click is reported going down and coming up. It appeared and vanished again;
  only a double click, an even number of toggles away, looked like it worked.
- The SS chip counted anything with an SS grade, resources included, so it read
  2 before a piece of gear had dropped. Keys, socketables and orbs have grades
  of their own; the grade columns are about gear and now count only gear.
- Essence Vaults no longer sound a rarity alert. Seven of them share one
  display name, so the alert fired constantly and said nothing about what had
  been found. They are still journalled and still counted.

## 0.9.91 — 2026-08-17

### Fixed

- A named item's grade now comes from the item tables in every case. The packet
  was allowed to outrank them whenever its claim was not one of the four the
  code treated as unreliable — so a D-grade Satanic ring arrived claiming
  Angelic and was announced, chimed and filed as an Angelic find. The tables
  carry the game's own per-item grade; the packet is consulted only for items
  they have never heard of.
- Ordinary pickups were counted as Angelic again — keys, potions and white
  bases. The packets that do it carry none of the marks that caught this the
  last time, so the claim is now refused where it is impossible instead: an
  ordinary base cannot be Angelic or Unholy, those two grades being named items
  by definition. Over a full session's capture it takes 52 false Angelics off
  the board and leaves the one real one.
- The Reset and Quit buttons could fire on an ordinary double-click: the
  confirming click had a deadline but no settling time, and this is a game
  where the left button is held down. They also stayed armed through a lock.
- Locking the overlay from its own strip left the strip lit but dead, with the
  clicks going into the game: two places set the click-through and only one of
  them remembered doing it.
- The game's own minimize plate had no hover frame — hovering it made the plate
  vanish and left the dash floating. It is drawn from the same recipe as the
  new plates now, and its resting frame comes out byte-identical to the one
  that shipped.
- On an adapter that does segmentation offload, no message longer than one
  frame ever arrived: the capture is handed the whole buffer while the header
  still describes a single segment, and the parser either dropped the frame or
  kept a segment's worth. Drops and gold fit in one frame and were fine; the
  character save is about 5 KB, and it is the only thing carrying experience and
  kills — so those two counters sat at zero all session.
- Magic Find was drawn in a blue that measured 2.4:1 against the plate it sits
  on, under the 3:1 floor. It is a palette token now, measured on both skins —
  and in the dashboard it was not blue at all: a rule one step more specific had
  been repainting it bone since the box was written.
- The Satanic counters measured 2.9:1, the least readable figures on the panel
  and the rarest thing they count. They now use the red the overlay already had
  for standing in the Satanic zone, so there is one red and not two.
- The drop ticker's plates were 12px wider than the overlay on each side: they
  were inset by the gap between chips rather than by the panel's own border.
- The Reset button was 18px narrower than the column it stood in, which pushed
  the top row's second boundary nine pixels right of every other row's.
- The rectangle that reserves the lock corner from a click-through overlay was
  17px wider and 13px taller than the lock, and its spare corner lay on top of
  the button below it.
- Putting an item on the auction house was counted, journalled and announced as
  a fresh drop, chime and all — the listing travels in the same field the server
  uses to report what dropped.
- The OBS browser sources never announced anything. Every notable drop was
  queued and nothing read the queue, so `?view=flourish` could not play; the
  drop ticker had no address at all. Both now receive, and Settings lists four
  addresses instead of three.
- Deleting a filter or a list gave no sign that the first click had armed it —
  the two controls that destroy the most.
- The rates graph asked the canvas to resolve a CSS variable, which it cannot:
  gold and experience were drawn in the same colour, and "the graph appears
  after a couple of minutes" was black on a black panel.
- Clicking the tray icon hid a minimised dashboard instead of bringing it back.
- The session clock started when the app did, not when the game did. On
  autostart every per-hour figure was divided by the idle hours before play,
  and the run filed at the end carried them.
- Placing the drop announcement showed one sample and then an empty box, so
  size and shading could not be judged. Its dashed frame no longer paints over
  the sample either.
- "Restart through XWayland" killed the copy it started: the parent still held
  the single-instance name, so the child saw it taken and exited.
- State files were truncated in place. A crash or a full disk mid-write left a
  file that parses as nothing — and for the run history the loss was made
  permanent by the next save. Everything is staged and renamed now, and a
  write that fails says so instead of passing for a save.
- A request to the OBS server with a bracketed IPv6 host was refused.
- A failing Fedora job withheld the .deb and the AppImage that had built.

### Changed

- The overlay is a row shorter. Character level, hero level and the two purse
  totals are gone — the game's own HUD shows all four — and Magic Find keeps the
  top row, being the one figure it never shows. The drop counters move up a row,
  and the kill counter is now always on screen instead of appearing only when
  the Reset button was switched off.
- The right-click menu is gone. Its entries — Dashboard, Hide to tray, Reset,
  Quit — are a column of buttons standing beside the overlay, clear of the panel
  rather than on it, each the height of a chip. They sit there
  dimmed while the overlay is unlocked, and appear on hover while it is locked,
  the way the lock button always has. The lock is the column's top button.
  Reset and Quit still ask twice, and walking away from the column takes the
  question back.
- The Reset Stats button has left the panel's rows, and the session's SS count
  ends that row instead — the top grade, counted whatever rarity the drops came
  out in. The grades were always tallied; this one had never been shown outside
  the Discord line. The button was also the only cell that came and went, ghost
  mode drawing none, so that row was a cell short exactly when the overlay was
  pinned over the game. Resetting is on the new column, in the tray, and on
  Ctrl+Shift+R.
- New mail blinks the chip for twenty seconds and then stays gold until it is
  collected. It used to read "Mail!" in the same colour as every other figure.
- The capture probe opened and closed a device several times a second; it is
  asked once a minute. The endpoint sweep — which on Linux reads every open
  file of every process — moved onto the five-second beat it already had.
- Repeated warnings are written once and then only on a change or after ten
  minutes. A machine that cannot capture used to fill the log in twenty and
  take the start line, the environment survey and any backtrace with it.

## 0.9.90 — 2026-08-16

Linux, gone over properly. Windows is unchanged.

### Fixed

- The app aborted on X11 when the drop announcement was switched on, and with
  the overlay locked — and saved the setting before applying it, so it did the
  same at every start afterwards.
- A missing tray library killed the app before it had a window.
- Capture rights were never checked until the game ran, so a machine without
  them showed a friendly "waiting for Hero Siege" forever.
- The capture wrote nothing to the log.
- The status went green on every attempt, including the failing ones.
- "The game's traffic is not reaching us" fired in the first minute of every
  session.
- The game was found by process name alone — not enough under a Steam wrapper
  or Proton.
- Its address arrived IPv4-in-IPv6, which built a packet filter nothing could
  match: counters stayed at zero with the capture up.
- A VPN reconnect took that adapter down for five minutes.
- Window positions saved on Wayland were always (0, 0).
- The tray's Dashboard entry did nothing after the window was minimised.
- The drop ticker sat across the overlay instead of under it, and lost its
  always-on-top when hidden.
- Custom alert sounds could never be served over the asset protocol.
- The drop announcement could not be switched on without an overlay, so its
  OBS page never received anything.
- The OBS server filled the log and answered a request with no `Host` header.
- Copy buttons said "copied" whether or not it worked.
- Every "open the folder" left a zombie process behind.
- With no `HOME`, everything the app writes went to a directory never created.
- The overlay appeared in Alt-Tab.
- The postinstall script hid its own failure and asked for more rights than it
  uses.
- `npm start` and `npm test` were Windows batch files.
- Two copies could run at once, each with its own sniffer.
- On Linux the old frame stayed under the new one; the panel is painted solid
  there, which is the only thing that covers it.

### Added

- The interface says when it has painted; if nothing does within 20 seconds the
  log says so and names the two variables worth trying.
- One log line for the session: display server, toolkit backend, desktop,
  graphics driver.
- `npm run deb` builds the Linux packages in a container, against an older
  glibc than the host's.
- Settings: **Enable transparent overlay while locked** — on by default only
  where it does not smear.

### Changed

- The automatic pause after five quiet minutes is gone; the pause by hand
  stays.

## 0.9.89 — 2026-08-16

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
- Errors are written to a log beside the settings — panics with a backtrace, and
  anything a panel throws. About says where it is and opens the folder.
- **OBS**: the overlay window is named apart from the dashboard, so a Window
  Capture can tell them apart — and, for anyone who would rather it were on the
  stream and not on screen, both are served as pages on `127.0.0.1` for a
  Browser Source — the overlay, the dashboard and the drop announcement. Off by
  default; the addresses and the size are in About.
- An **Items** section: every named item with its chance anywhere, its better
  chance where it is tied, and where that is. Search by name, rarity or kind.
- An **About** section: the version, who made it, and a check for a newer
  release. It is the only request the app makes, and only on the button.

### Changed

- The overlay's loot chips show the count and nothing else. Each carried a
  second figure in brackets — how many of those drops the game credited to
  Magic Find — which cost more room than it was worth. The three chips are now
  the same width as every other.
- The scrollbars are the app's own rather than the system's.
- The OBS addresses moved out of About and next to the switch that serves
  them, in Settings. The capture instructions are in the README.
- The README is for players now; the rest moved to `DEVELOPING.md`.

### Fixed

- Linux with an NVIDIA card: the app came up as a tray icon and no window.
- The overlay did not grow when a row was added to it; it measures itself now.
- The overlay could lose always-on-top across a hide and show.
- The minimize button was drawn by hand and did not follow the skin.
- Ordinary items were counted as rare ones. An item going into the bag was
  looked up in the item tables by its slot, and ordinary bases are numbered in
  the same small range as the uniques — so a white sword came back as whatever
  unique shares its number, and the counters believed it. In a capture of one
  seasonal run, 35 of 38 ordinary pickups were being counted as Satanic. The
  tables are now only asked about an item the game has flagged as named, or one
  whose packet already says it is rare. The drop path had this rule and says
  why in its own comment; the pickup path never learnt it.
- **Odyssey** runs counted every pickup as Angelic. Odyssey keeps its own item
  space and its own packet, and the field a seasonal item uses for rarity holds
  something else there — the same 7 on everything, white items included, and 7
  is Angelic on the seasonal scale. Its drops are counted without a rarity
  rather than with the wrong one.
- Everything on a custom list chimed twice — once as it hit the ground and
  again as it went in the bag. One item, one alert, whichever sighting comes
  first.
- **Copy** made a filter whose lists were all mute: they were given new names
  of their own on disk and nothing was put under them. It copies the sounds.
- Choosing a new sound removed the old one before the new one was in place, so
  anything that went wrong in between left the list with no sound at all.
- The second click of a two-click delete went to whatever was selected then,
  not to what had been armed. Picking another list between the clicks deleted
  it outright.
- **Test** played the file of the list that was selected when it was pressed at
  the volume of the one selected when the file finished loading.
- Another player's find set off your alerts. The server puts notable finds in
  chat for the whole shard, and the app treated every one of them as its own —
  a stranger's Angelic sounded the horn mid-run, past a minimum tier that was
  set to silence exactly that. Only your character's finds count now.
- The log path in About was unreadable: the game's typeface has another glyph
  where the backslash should be, so `C:\Users\…` came out as `C:wUsersw…`.
  Paths and addresses are set in a plain monospace now.
- Browse, Import and Export froze the whole app until the file dialog was
  answered — it could not even be closed. The dialog was opened from the thread
  that draws the windows.
- An imported filter said its lists had no sound while their files were on
  disk and Test played them. Every list asked at once, and each reply threw
  away the others.
- Ending a run disarmed the drop announcement. Every setting is carried across
  a reset by hand, and this one had been left out — so the moment the game was
  closed, or the session reset, nothing was announced again until the settings
  were saved. The settings now travel in one piece.
- A drop that only the announcement wanted also played the alert sound: with
  the alerts set to SS and the announcement to D, every D item made a noise.
  Only the alert rules make a sound now.
- **Least grade — D** did not mean D. An item the tables do not grade was read
  as below it, and those are the ones with no grade to compare. The lowest
  setting now means every drop of that rarity, graded or not.
- The bank showed nothing until the game next saved. It is read from the one
  purse that has money in it now, and the save still has the last word.
- Switching the drop announcement off and straight back on, then placing it,
  froze the app. Its window was destroyed and another built under the same
  label; it is only hidden now.
- Placing the drop announcement could leave the app unusable: a transparent
  window took the mouse where the window manager happened to put it, and swallowed
  every click meant for what was underneath. It centres itself, is now plainly a
  box, takes the keyboard so its own button works, ends on Escape, and ends by
  itself after three minutes.

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

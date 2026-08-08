# HS Tracker

A session tracker for **Hero Siege**: a small always-on-top overlay drawn from
the game's own UI art, plus windows for detailed statistics, a drop ticker and
a shopping list.

It reads the game's network traffic (via Npcap) and reports gold, experience,
kills, item drops by rarity, and the current Satanic Zone with its buffs and
debuffs. It never touches the game process, never injects anything and never
sends data anywhere — everything stays on your machine.

![The overlay over a running game](screenshots/overlay.png)

## Features

- **Compact overlay** — session timer, mail, gold, XP, item counters by rarity
  and the active Satanic Zone. Frameless, always on top, drag anywhere.
- **Lock mode** — pin the overlay and it becomes click-through, so it never
  eats a click meant for the game. The lock button itself stays clickable.
  Over a running game the frame also disappears, leaving only the numbers.
- **Drop ticker** — valuable drops appear as a short list under the overlay
  with the item's real name, then fade away.
- **Sound alerts** — a separate configurable sound per rarity (Satanic, Set,
  Heroic, Angelic, Unholy) plus a mail reminder. Alerts fire the moment an item
  hits the ground, not when you pick it up.
- **Statistics window** — per-rarity cards, notable drops (Angelic Key, Satanic
  Dice, S/SS runes …), a gold/h and xp/h graph, Satanic Zone pros and cons with
  a countdown to the next rotation, and a timeline of named drops.
- **Shopping list** — a scratchpad where clicking an entry copies it to the
  clipboard, ready to paste into the market search.
- Global hotkeys, autostart, per-section visibility, opacity and scale.

## The windows

| Statistics | Settings |
| --- | --- |
| ![Statistics](screenshots/statistics.png) | ![Settings](screenshots/settings.png) |

Statistics keeps the per-rarity cards, notable drops, the gold/h and xp/h graph
and the drop timeline; Settings holds the alerts, the overlay sections and the
sound for each rarity. The shopping list is a scratchpad where clicking an
entry copies it, ready to paste into the market search.

![Shopping list](screenshots/shopping-list.png)

## Install

Grab the installer from the [Releases](../../releases) page and run it. It
installs for the current user, so Windows never asks for administrator rights.

![The installer](screenshots/installer.png)

HS Tracker needs [Npcap](https://npcap.com), the packet capture driver, to read
the game's traffic. The installer checks for it and, when it is missing, offers
to download the official installer from npcap.com and run it — its defaults are
fine. Npcap itself is not bundled: its free edition may not be redistributed
inside another installer.

Everything the app writes (settings, carried totals, shopping list, custom sounds)
lives next to the executable. Windows 10 or 11 is required.

## Usage

The tray icon is the control centre: left click toggles the overlay, right
click on the overlay opens the same menu.

| Action | How |
| --- | --- |
| Show / hide overlay | tray click, or `Ctrl+Shift+O` |
| Lock / unlock | lock icon on hover, or `Ctrl+Shift+L` |
| Reset session | overlay button, tray menu, or `Ctrl+Shift+R` |
| Statistics / Shopping list / Settings | tray menu or right click on the overlay |

The overlay only starts counting once the game sends an account packet, which
happens shortly after you enter a zone.

### Alerts

Every rarity has its own channel with an on/off switch, volume and a preview.
**Browse…** copies your own file (mp3/wav/ogg/flac) into `sounds\` next to the
executable, where it overrides the bundled one; **Default** removes it again.

**Min tier** narrows alerts further: item grades

## How it works

Hero Siege talks to its servers over plain TCP. HS Tracker finds the game
process, learns which server addresses it is connected to, and captures those
conversations with Npcap. Messages arrive as JSON, base64 blobs or query
strings, sometimes several per packet and sometimes split across packets, so
the reader reassembles them before parsing.

## Building

```bash
npm install
npm start          # dev run
npm run release    # installer in src-tauri/target/release/bundle/nsis
cd src-tauri && cargo test
```

`package.json` owns the version. `npm run ver 1.1.0` writes it into
`tauri.conf.json` and `Cargo.toml` as well, and `npm run release` runs that
first, so the installer, the crate and the tag can never disagree.

Rust (MSVC toolchain) and Node are required. The `tauri-dev.cmd` /
`tauri-release.cmd` wrappers load the Visual Studio environment first, because
linking fails without it. The Npcap SDK import libraries are vendored in
`src-tauri/npcap-sdk`; `wpcap.dll` is delay-loaded, so the app starts and
reports the problem instead of crashing when Npcap is absent.

### Regenerating assets

`tools/` holds the generators, none of which run during a normal build:

- `fetch_items.py` — pulls the datamined item table (identity, rarity, grade)
  from hero-siege-helper into `tools/data/helper/items.json`.
- `gen_items.py` — rebuilds `src/items.js` and `src-tauri/src/items.rs` from
  that table plus the game's own `translationsItem.csv`, so names read exactly
  as they do in game. Point `HERO_SIEGE_BIN` at the install if it is not on the
  default path.
- `yytex.py`, `datawin.py`, `export_ui.py` — decode the game's own textures and
  re-export the UI sprites the overlay is skinned with, from an installed copy
  of Hero Siege.

## Credits and licensing

- The overlay is skinned with Hero Siege sprites. Hero Siege is © Panic Art Studios. This project is not affiliated
  with or endorsed by them.
- The protocol work builds on
  [hero-siege-stats](https://github.com/GuilhermeFaga/hero-siege-stats) and
  [Hero-Siege-Companion](https://github.com/DemonSkye/Hero-Siege-Companion);
  the item identity, rarity and grade tables are generated from
  [hero-siege-helper](https://hero-siege-helper.vercel.app)'s datamined data
  and are not redistributed here.

The code is released under the [MIT license](LICENSE).

## Known inaccuracies

Inherited from how the protocol reports things:

- Gold received by mail counts as earned.
- Experience is slightly off across a level-up.
- Moving items between inventories can register as a pickup.
- Only named items are identified: the drop of an ordinary base carries a
  different id space, so it is counted but never named or announced.
- `CURRENT_SEASON` in `src-tauri/src/stats.rs` has to be bumped when a new
  season starts, otherwise a seasonal character's gold reads as non-seasonal.

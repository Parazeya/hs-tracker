# Developing HS Tracker

## How this works

Two halves, both in this repository:

1. **`src-tauri/`** — the Rust side. It finds the game process, learns which
   servers it is talking to, captures those conversations with libpcap (Npcap on
   Windows), reassembles the messages and keeps the running totals. Everything
   the app knows starts here.
2. **`src/`** — the Svelte 5 front end. The overlay, the dashboard and its
   sections. It draws what the backend pushes and owns no state of its own worth
   the name.

The pieces of the Rust side, in the order data moves through them:

| | |
| --- | --- |
| `sniffer.rs` | Finds the game, picks the adapters worth listening on, opens the captures and reports what the capture is doing. |
| `parser.rs` | Reassembles TCP payloads into whole messages and turns them into `GameEvent`s. Messages arrive as JSON, base64 blobs or query strings, several per packet or split across packets. |
| `stats.rs` | Applies those events: the session totals, the drop journal, the room timings, the finished runs. |
| `items.rs` | Generated tables — item identity, rarity, grade, drop rates. |
| `lib.rs` | Windows, tray, hotkeys, settings, commands. |
| `presence.rs` | The Discord status. |
| `log.rs` | Panics, warnings and whatever the front end throws, appended to `hs-tracker.log`. A released build has no console, so nothing else survives. |

The front end runs in the app's own windows and nowhere else. It went through
`src/bridge.js` to reach the backend when there was a second way in — a page
served to OBS as a Browser Source — and it still does, because the guard is
worth keeping: a component rendered by a webview with no Tauri under it gets
nothing back rather than an exception in a transparent window.

There is one route into OBS now, and it is a window capture. See the README.

Two things worth knowing before changing anything:

- **The game reports gold, experience and kills only when it saves the
  character.** Between saves they do not move. Drops arrive continuously. Code
  that assumes all four advance together will be wrong.
- **The same item is seen twice** — once when the server rolls it onto the
  ground, once when it lands in the bag. Its hash ties the two together.

## How to develop

```bash
npm install
npm start                    # dev run: vite + tauri
npm test                     # the Rust tests
```

Both work on every platform: on Windows they load the Visual Studio environment
first, elsewhere they call the Tauri CLI and cargo directly. Install Rust with
[rustup](https://rustup.rs) rather than from a distribution's packages — Ubuntu
24.04 ships 1.75 and this tree needs 1.88 — the code itself only wants 1.87, but
the lock file's `time` asks for 1.88, which is what `rust-version` in
`Cargo.toml` records.

On Linux a dev build reads nothing until it is allowed to capture, and the
capability is on the file, so **every relink drops it**:

```bash
sudo setcap cap_net_raw=ep src-tauri/target/debug/hs-tracker
```

`npm start` checks and reminds you.

Rust and Node are required. On Windows the MSVC toolchain must be installed:
`tauri-dev.cmd`, `tauri-release.cmd` and `test.cmd` all load the Visual Studio
environment first, because linking fails without it. Which one they load is
`vcvars.cmd`'s job — it asks `vswhere` for the newest install carrying a C++
toolset and reads both program folders if that is not available, so any edition
and any version does, in any location, and a machine with none of them is told
so plainly instead of failing at the link step. The Npcap SDK import libraries
are vendored in
`src-tauri/npcap-sdk`, and `wpcap.dll` is delay-loaded, so the app starts and
reports the problem instead of crashing when Npcap is absent.

## How to build

```bash
npm run release              # Windows: installer in src-tauri/target/release/bundle/nsis
```

`package.json` owns the version. `npm run ver 1.1.0` writes it into
`tauri.conf.json` and `Cargo.toml` too, and `npm run release` runs that first, so
the installer, the crate and the tag cannot disagree.

Tagging does not publish. It did once, and `.github/workflows/release.yml` is
now `workflow_dispatch` only — nothing in it writes to a release. The packages
are built on a developer's machine and the release is cut from there, which is
also the only way the Linux three get built by the container they are meant for.

### Releasing

```bash
npm run ship 0.9.9        # or: patch / minor / major
npm run ship -- --dry     # print the plan and stop
```

It sets the version, builds, runs the tests, commits, tags and pushes — and asks
before any of it. Flags go after `--`, and none of them start with `--no-`: npm
takes those for its own configuration and never passes them on.

It refuses rather than ships when the tag already exists, the branch is not main,
or `CHANGELOG.md` does not open with the version being released — the release
notes are cut from that section, so a mismatch would describe the wrong release.
`--skip-notes`, `--skip-tests` and `--any-branch` each waive one check.

Then the packages and the release itself:

```bash
npm run all               # the installer, the .deb, the .rpm and the AppImage
npm run publish -- --dry  # print what would be published and stop
npm run publish
```

`npm run publish` refuses a dirty tree or a HEAD that is not the tag it is
publishing under, because the artifacts in `release/` were built from HEAD and
would otherwise be published under a tag that describes something else.

A tag that was pushed but never published can still be moved — nothing points at
it yet. Delete it on both sides first, or `ship` will refuse it:

```bash
git tag -d v1.0.0 && git push origin :refs/tags/v1.0.0
```

### Linux, from any machine

```bash
npm run deb                 # a .deb in dist-linux/
npm run deb -- --appimage   # and an AppImage
npm run deb -- --rebuild    # rebuild the image after changing docker/
```

It builds in a container (`docker/Dockerfile`), so the toolchain, the WebKitGTK
headers and the glibc the binary is linked against come from the image rather
than from whatever the machine happens to have. The base is Ubuntu 22.04 on
purpose: a binary linked against a newer glibc will not start on an older one,
and the README offers the `.deb` to Ubuntu 22.04, Mint 21 and Debian 12. Raise
the base only to drop them.

The cargo registry and the target directory live in named volumes, so only the
first build is slow. `npm run deb -- --clean` throws them away.

### Linux, natively

```bash
sudo apt install build-essential curl wget file patchelf zsync \
                 desktop-file-utils libfuse2t64 \
                 libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev \
                 librsvg2-dev libpcap-dev \
                 gstreamer1.0-plugins-base gstreamer1.0-plugins-good gstreamer1.0-libav
npm ci && npm run build
npx tauri build --bundles deb,rpm,appimage
```

The last three lines of packages are for the AppImage alone: its tooling needs
FUSE 2 and patchelf, and it copies in the GStreamer plugins the webview uses to
play the alert sounds. A `.deb` or `.rpm` needs none of that.

**The `.rpm` has to be built on Fedora.** Debian calls the capture library
`libpcap.so.0.8` and Fedora calls the same library `libpcap.so.1`, so a package
built on Ubuntu installs on Fedora and then refuses to start. CI uses a
`fedora:41` container for that job; locally the same thing in Docker:

```dockerfile
FROM fedora:41
RUN dnf install -y --setopt=install_weak_deps=False \
      gcc gcc-c++ make rust cargo nodejs npm \
      webkit2gtk4.1-devel gtk3-devel libayatana-appindicator-gtk3-devel \
      librsvg2-devel libpcap-devel openssl-devel file which findutils tar
```

Mount the repository read-only and copy it in before building — a container
writing into the working tree leaves Linux output where the Windows build looks
for its own. Check the result with `objdump -p usr/bin/hs-tracker | grep NEEDED`:
`libpcap.so.1` means it will start.

### Platform differences worth remembering

- **Where the app writes.** Windows keeps settings, carried totals and custom
  sounds beside the executable, so the folder is portable. Everywhere else they
  live in `$XDG_CONFIG_HOME/hs-tracker`, because `/usr/bin` and a mounted
  AppImage are read-only. Autostart is a registry value on Windows and a
  `.desktop` file in `~/.config/autostart` elsewhere.
- **Capture rights.** Linux needs `cap_net_raw`; the `.deb` and `.rpm` grant it
  in `installer/grant-capture-rights.sh`. An AppImage cannot be given it, and
  the two ways of trying fail differently. On the `.AppImage` file the right is
  recomputed away at the `execve` of the binary inside, so the app starts and
  captures nothing. On that inner binary the app is privileged and the loader
  refuses both ways the bundle is found — the `LD_LIBRARY_PATH` its `AppRun`
  sets and the `$ORIGIN/../lib` runpath in the binary — so it does not start at
  all: `libwebkit2gtk-4.1.so.0: cannot open shared object file`. Unpacked,
  the binary is on a mount that only exists while the app runs, so there is
  usually no file to try it on. `sudo` is the only route, and it writes settings
  and runs to root's home.
- **The overlay needs X11.** It leans on click-through windows, programmatic
  positioning, the cursor position outside itself and global hotkeys, none of
  which Wayland gives an application. The app checks the session at startup
  (`WAYLAND_DISPLAY` / `XDG_SESSION_TYPE`, with `GDK_BACKEND=x11` taken as X11
  through XWayland) and on Wayland never shows the overlay or the ticker, skips
  the hotkeys and hides their settings. The windows are still created — the
  hidden overlay is what plays the alert sounds. Settings can relaunch the
  process with `GDK_BACKEND=x11`; the choice is honoured before any window
  exists, because a toolkit picks its display server once and cannot be talked
  out of it later.
- **A fullscreen game outranks the overlay.** KWin — and every compositor that
  works the way it does — puts the active fullscreen window in a layer above the
  one that holds keep-above windows. `alwaysOnTop` is honoured and still loses.
  Nothing set on our window changes that: window type hints land in layers that
  are also below it. The game has to run windowed or borderless. `reveal()`
  re-asserts always-on-top after a show anyway, because an unmapped window's
  state is the window manager's business — the same reason the position is
  restored there.
- **NVIDIA.** WebKitGTK composites through a DMA-BUF renderer that the
  proprietary driver does not survive: the web process segfaults inside
  `libnvidia-eglcore` tearing a GL context down, and the app comes up as a tray
  icon with no window. `ease_webkit()` in `lib.rs` turns that renderer off when
  it finds the driver on the machine.

## Regenerating assets

`tools/` holds the generators. None of them run during a normal build.

| | |
| --- | --- |
| `fetch_items.py` | Pulls the datamined item table from hero-siege-helper into `tools/data/helper/items.json`. |
| `gen_items.py` | Rebuilds `src/items.js` and `src-tauri/src/items.rs` from that table plus the game's own `translationsItem.csv`. Point `HERO_SIEGE_BIN` at the install if it is not on the default path. |
| `gen_icon.py` | Draws the app icon on a 16×16 grid and writes every size the app, the tray and Windows want. `--preview` lays them out to look at, `--discord` writes the artwork the Discord application is given. |
| `gen_installer_art.py` | The installer's header and sidebar, drawn from the icon. Run it after `gen_icon.py`. |
| `yytex.py`, `datawin.py`, `export_ui.py` | Decode the game's own textures and re-export the UI sprites the app is skinned with, from an installed copy of Hero Siege. |
| `gen_theme.py` | Rewrites `src/theme.css` — every skin's palette. `--preview` also writes a swatch sheet. |
| `gen_skin.py` | Recolours the game's sprites into a season's own set under `src/assets/game/<season>/`. |

## Skins

A skin is three things, and only the first is required:

| | |
| --- | --- |
| a palette | a `:root[data-theme='<name>']` block in `src/theme.css`, written by `tools/gen_theme.py`. A season is the default palette with its hues moved; **Modern** is a hand-written one, because there is no warm palette left to shift. |
| sprites | `src/assets/game/<name>/*.png`, from `gen_skin.py`. `art()` in `src/skin.svelte.js` falls back to the original for anything a skin has not drawn, so a half-finished set still renders. |
| chrome | only if the skin does not use the game's art at all. **Modern** is the one that does not: `src/modern.css` draws every panel, chip and button out of borders and radii instead, and `art()` answers `null` for a sprite it has no line-icon version of, which is what tells a component to let CSS draw. |

Adding one means a palette block, an `<option>` in Settings, and nothing else —
the backend stores the name as a plain string and never checks it.

### Looking at one

`tools/preview/` mounts every window in a plain browser against a mocked
backend, which is the only way to see the loot pillar and the drop ticker
without the game running:

```
npx vite
http://localhost:5176/tools/preview/?theme=modern
```

`?panel=` takes `runs`, `filter`, `watchlist`, `codex`, `shop`, `settings`,
`about`, `flourish`, `zone` or `ticker`; with none, it draws the dashboard and
the overlay side by side. `?theme=` takes any skin name — point it at `default`
and then at the new one, and the difference is the whole review. `?update=1`
and `?trouble=1` put the two banners up, and `?diag=<selector>` prints what the
browser decided about whatever matches.

## Updates

The app checks once, a few seconds after the dashboard has drawn, and offers
what it finds. It installs nothing on its own: the banner has a button, and the
overlay sits on top of a game where a surprise restart is not the app's call.

It only installs a package signed by the key whose public half is compiled into
the binary — `plugins.updater.pubkey` in `tauri.conf.json`. Generate the pair
once:

```bash
npx tauri signer generate -w "$USERPROFILE/.tauri/hs-tracker.key"
```

The public half goes in `tauri.conf.json`; the private one stays out of the
repository. `npm run all` finds it at that path by default, or wherever
`TAURI_SIGNING_PRIVATE_KEY_PATH` says — see `.env.example`.

**Back the private key up somewhere that is not this machine.** Lose it and no
copy of the app already installed anywhere can ever be updated again: a new key
means a new public half, and a binary only trusts the one it was built with.
Everyone reinstalls by hand.

`npm run publish` writes `latest.json` beside the installer on the release —
version, notes, the signature, and the installer's URL read back from GitHub
rather than guessed, because GitHub renames an asset on the way in and turns
every space into a dot. `plugins.updater.endpoints` points at
`releases/latest/download/latest.json`, so cutting a release is what publishes
the update. A build with no signing key still produces a working installer;
`npm run all` and `npm run publish` both say so, and that release simply is not
offered to anybody.

The version check on the About page is a different thing and stays: it reads the
GitHub API and links to the release page, which is the way round it if an
install ever fails.

## Known inaccuracies

Inherited from how the protocol reports things, not from the parsing:

- Gold received by mail counts as earned.
- Experience is slightly off across a level-up.
- Moving items between inventories can register as a pickup.
- Only named items are identified. The drop of an ordinary base carries a
  different id space, so it is counted but never named or announced.
- The bank total is read from the purse that matches the character: any season
  number means the seasonal one, no season means non-seasonal or blood pact. A
  character left over from a past season that still reports that season number
  would be read from the seasonal purse.

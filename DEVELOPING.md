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
| `map_window.rs` | Queued full-screen map lifecycle, current monitor and Ctrl+Shift+M. |
| `stream.rs` | Read-only loopback Browser Sources, bounded latest-state SSE and validated map view state. |
| `presence.rs` | The Discord status. |
| `log.rs` | Panics, warnings and whatever the front end throws, appended to `hs-tracker.log`. A released build has no console, so nothing else survives. |

Native windows use `src/bridge.js`. OBS has an independent `stream.html` entry
and `StreamView.svelte`; that entry imports no Tauri commands, controls or audio.

The `world-map` window mounts `MapView.svelte`. Its header and close control
are local; the packaged current hs-map runs in a sandboxed loopback iframe.
`capabilities/map-view.json` applies to local content only and grants no remote
origins. A native worker lock serializes show/hide requests and reuses the
window. To preview the compiled map shell in a browser, open `/?view=map`.

Browser Sources use a GET-only loopback service on a stable configurable port.
`configure` projects public appearance fields; the stats pusher also runs for
stream clients when desktop windows are hidden. Deduplicated drops enter the
stream after engine filtering, and notifications have a bounded six-entry
schedule. SSE retains only the latest state for each topic; slow readers skip
obsolete map positions. An epoch identifies Tracker restarts.

The map package is prepared by hs-map's `package:tracker` script into
`public/hs-map`, copied by Vite to `dist/hs-map` and embedded by Tauri. The
postMessage protocol is versioned, origin/source checked and data-only. Map
state includes normalized camera, source document dimensions, room and visible
presentation. OBS renders at those dimensions then scales/letterboxes; it does
not pretend a different aspect ratio reveals the identical view naturally.
`npm run test:stream` checks reconnect, expiry, validation and aspect semantics.

Two things worth knowing before changing anything:

- **Cumulative account counters arrive with saves.** Explicit bank deposits,
  XP gains and drops can arrive between them. Reconcile an explicit gain with
  its later total; do not credit both observations.
- **The same item is seen twice** — once when the server rolls it onto the
  ground, once when it lands in the bag. Its hash ties the two together.

## How to develop

```bash
npm install
npm start                    # dev run: vite + tauri
npm test                     # the Rust tests
npm run test:settings        # pending edits, native changes and failed writes
npm run test:stream          # map/SSE protocol, reconnection and corrected counters
```

The native commands work on every platform: on Windows they load the Visual
Studio environment first, elsewhere they call the Tauri CLI and cargo directly.
Windows entry points share `scripts/paths.mjs` discovery and honor `VCVARS` and
`CARGO_HOME`; no VS 2019 path needs editing in a batch file. Install Rust with
[rustup](https://rustup.rs) rather than from a distribution's packages — Ubuntu
24.04 ships 1.75 and this tree needs 1.88 — the code itself only wants 1.87, but
the lock file's `time` asks for 1.88, which is what `rust-version` in
`Cargo.toml` records.

For isolated Windows GUI checks, copy the executable into a new temporary
profile directory and set process-only `HS_TRACKER_TEST_PROFILE=1`. This skips
the global Windows autostart registry integration for that QA instance; normal
launches continue to apply the user's autostart preference. Redirect WebView2's
user data and keep all test settings beside the copied executable.

On Linux a dev build reads nothing until it is allowed to capture, and the
capability is on the file, so **every relink drops it**:

```bash
sudo setcap cap_net_raw=ep src-tauri/target/debug/hs-tracker
```

`npm start` checks and reminds you.

Rust and Node are required. On Windows the MSVC toolchain must be installed;
`tauri-dev.cmd` and `tauri-release.cmd` load the Visual Studio environment first,
because linking fails without it. The Npcap SDK import libraries are vendored in
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

After a game update, build the sibling HeroSiege Extractor and run:

```bash
python -m pip install -r tools/requirements-data.txt
npm run items -- --dry
npm run items
```

The updater discovers Steam libraries or uses `HERO_SIEGE_BIN`. It asks the
Extractor for a new `--export` bundle in a unique staging directory, verifies
its manifest, dataset hashes and source game files, and generates both lookup
tables and translations before replacing anything. Items, talents and the
manifest are installed together in `tools/data/helper`. An interrupted install
is recovered on the next run; an export or generation failure keeps the
previous files. A dry run retains validated staging for inspection.

Set `HS_DATA_BUNDLE` to consume an already exported bundle. Its translation CSVs
are used directly; if the game is installed, its files must still match the
bundle. `HS_EXTRACTOR_BIN` overrides the platform-specific executable and
`PYTHON` selects a Python interpreter or virtual environment. Relative paths in
`.env` are resolved from this checkout. A manifest marked `partial` retains
unknown values and coverage in the raw export; only fields identified by the
current extraction enter the live lookup tables.

`npm run test:items` exercises package validation, publication recovery and
generation helpers without modifying installed data. For manual generator
work, `HS_ITEMS_INPUT`, `HS_ITEMS_OUT_JS`, `HS_ITEMS_OUT_RS` and
`HS_ITEMS_LANG_OUT` redirect inputs and outputs to a staging directory.

| | |
| --- | --- |
| `fetch_items.py` | Historical reference downloader; the normal update pipeline uses the Extractor bundle. |
| `gen_items.py` | Builds `src/items.js`, `src-tauri/src/items.rs` and language files from a verified Extractor bundle. Direct legacy input remains available for comparison, with its freshness explicitly unverified. |
| `gen_icon.py` | Draws the app icon on a 16×16 grid and writes every size the app, the tray and Windows want. `--preview` lays them out to look at, `--discord` writes the artwork the Discord application is given. |
| `gen_installer_art.py` | The installer's header and sidebar, drawn from the icon. Run it after `gen_icon.py`. |
| `yytex.py`, `datawin.py`, `export_ui.py` | Decode the game's own textures and re-export the UI sprites the app is skinned with, from an installed copy of Hero Siege. |

## Known inaccuracies

Inherited from how the protocol reports things, not from the parsing:

- Gold received by mail counts as earned.
- Direct XP gains are counted immediately. Gains inferred only from a save
  need a second compatible reading, including after a hero-level reset. An
  unconfirmed final climb across a level cannot be reconstructed without its
  old XP requirement. See [AUDIT.md](AUDIT.md) for the tested correction cases.
- Moving items between inventories can register as a pickup.
- Only named items are identified. The drop of an ordinary base carries a
  different id space, so it is counted but never named or announced.
- The bank total is read from the purse that matches the character: any season
  number means the seasonal one, no season means non-seasonal or blood pact. A
  character left over from a past season that still reports that season number
  would be read from the seasonal purse.

## Signing the Windows build

The installer and the exe inside it are unsigned unless a certificate is named.
Defender's ml classifiers judge an unsigned binary on what it does, and this one
reads network traffic and installs under `%LOCALAPPDATA%`: 1.1.9 was flagged as
`Trojan:Win32/Bearfoos.B!ml` on machines that had run every release before it.
Authenticode identifies the publisher and can let reputation carry across
releases. It does not guarantee a clean antivirus verdict or eliminate a
SmartScreen warning for a new build. Microsoft's current
[SmartScreen guidance](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation)
also says that EV certificates no longer confer instant reputation.

The updater's `.sig` files authenticate updates; they are separate from a
Windows Authenticode certificate. `scripts/windows-release-evidence.ps1`
records the exact binary hash and Windows signature status. Its optional
`-Scan` runs a custom Defender scan with `-DisableRemediation`, records the
result and changes no protection settings. A malware false positive can be
reported by the publisher through Microsoft's submission portal; no automatic
sample upload is performed by the release script. See [CAPTURE-BACKENDS.md](CAPTURE-BACKENDS.md)
for the experimental driver alternative and its current verification limits.

Import the certificate into the Windows store, read its thumbprint, and put it
in `.env` beside the game path and the toolchain:

```powershell
Get-ChildItem Cert:\CurrentUser\My | Format-List Subject, Thumbprint
```

```ini
HS_SIGN_THUMBPRINT=a1b2c3…
```

`npm run all` then prints `signing: yes`; with the line absent it prints
`signing: no` and builds unsigned, which is the ordinary case. An environment
variable of the same name wins over `.env`, for a one-off.

`HS_SIGN_TIMESTAMP` overrides the timestamp server; the default is DigiCert's.
Timestamping is not optional — without it every installer already cut stops
verifying the day the certificate expires.

This is a different key from `TAURI_SIGNING_PRIVATE_KEY_PATH` above it. That one
is the updater's, and only tells this app that an update is really ours; this one
is Authenticode, and tells Windows the same thing.

Not in `tauri.conf.json`: a thumbprint committed to a public repository names a
certificate for anyone who wants to go looking for it, and a machine without the
certificate has to be able to cut a build anyway.

For an open-source project, SignPath Foundation issues certificates free and
Azure Trusted Signing is about ten dollars a month with no hardware token; a
traditional OV certificate has needed one since 2023.

<p align="center">
  <img src="src-tauri/icons/discord/cover.png" width="720" alt="HS Tracker — session tracker for Hero Siege">
</p>

<p align="center">
  Gold, experience, kills and every drop — counted while you farm,
  <br>in the game's own skin.
</p>

<p align="center">
  <a href="../../releases"><b>➡️ Download for Windows ⬅️</b></a>
</p>

<p align="center">
  <a href="../../releases"><img alt="downloads" src="https://img.shields.io/github/downloads/th3conc3pt3ur/hs-tracker/total?style=for-the-badge&amp;label=downloads&amp;color=6f42c1"></a>
</p>

<!-- downloads -->
<table align="center">
  <tr>
    <td><b>Windows</b></td>
    <td><a href="../../releases/download/v1.1.2-tc.1/HS.Tracker_1.1.2-tc.1_x64-setup.exe">HS.Tracker_1.1.2-tc.1_x64-setup.exe</a></td>
  </tr>
</table>
<!-- /downloads -->

> [!NOTE]
> **This is a fork** of [Parazeya/hs-tracker](https://github.com/Parazeya/hs-tracker),
> which is where the app comes from and where to report anything that is not
> about the additions below. It adds a skin — *Modern*, flat and neutral
> instead of the game's pixel art — and in-app updates, and it releases for
> Windows only; the Linux packages are upstream's.
>
> Versions here carry a suffix: `1.1.2-tc.1` is this fork's first build on top of
> upstream's `1.1.2`. Both projects reached 1.0.6 on the same day, which is what
> the suffix is for.

> [!NOTE]
> **How soon does it work when a new season starts?**
>
> About an hour, an hour and a half. That is how long it takes to rebuild the
> item tables and cut a release.

![The overlay over a running game](screenshots/overlay.png)

## What it does

| | |
| --- | --- |
| **Overlay** | A small always-on-top panel with the run so far. Lock it and it becomes click-through, so it never eats a click meant for the game. |
| **Statistics** | Gold, xp and kills per hour, drops by rarity, magic find, bosses and chests, what rolls better in the zone you are standing in, and the Satanic Zone with a countdown to the next rotation. |
| **Alerts** | One page: a sound per rarity, the grade a drop must reach, named lists of specific items with sounds of their own, the satanic zone rotating, and the announcement — all set side by side rather than three tabs apart. Alerts fire the moment an item hits the ground. |
| **Announcement** | A drop lands and the game's own loot pillar plays over the screen, wherever you have put it. It can follow the rarity switches or simply announce whatever your custom filter lists. |
| **Satanic Zone** | The rotation gets a chime and an announcement of its own — a rift that opens across the screen with the new zone and the buffs it rolled, drawn so it is never mistaken for a drop. Tick the buffs worth leaving a fight for and the rest pass in silence; tick none and every rotation is announced. |
| **Items** | Every named item, its drop chance, and the places it rolls better in. Search by name, rarity or kind. |
| **Runs** | Every finished session kept — the rates, the finds, where the time went. **Copy card** turns one into a picture you can paste into a chat. |
| **Pause** | By hand, or by itself after five quiet minutes, so a break does not end up in the per-hour figures. |
| **OBS** | The announcement window can stay on screen between drops, so OBS can capture it as a window source. |
| **Backup** | Export every setting — rarities, grades, the announcement, every filter and list, and the sound files themselves — to one file, and read it back on another machine. |
| **About** | The version you are running, who made it, and a button that asks GitHub whether a newer release is out. |
| **Discord** | Optional: while the game is open, your friends see the zone, the drops and the timer. |

## Where the numbers come from

It listens to the game's own network traffic, with the same packet-capture
library `tcpdump` and Wireshark use — Npcap on Windows, libpcap on Linux. The
game already tells its server about every drop, every deposit and every save.
This reads that conversation going past and counts what is in it.

It never touches the game. Nothing is injected, no memory is read, no file of
the game's is opened, and nothing about it is modified. What it does with the
game process is ask the operating system two things — whether it is running, and
which servers it holds connections to — which are the two questions Task Manager
and `netstat` already answer for anyone who asks them.

The capture is narrowed to the game's own servers: the app asks the operating
system which addresses the game is connected to and listens to those, so the
rest of what your machine is doing is never looked at. There are setups where
that cannot work — a route optimiser such as ExitLag redirects the game's
packets below the level Windows reports them at, so the addresses it names are
not the ones on the wire, and nothing is counted at all. **Read every
connection** in Settings takes the filter off for those machines. That widens
what is read on your own machine and changes nothing else.

Nothing is sent anywhere; every number stays where it was counted. Two things
leave the machine, both only if you ask for them: the **About** section's update
check, which asks GitHub for the newest release when you press the button, and
Discord rich presence, which hands the zone, the drop count and the session
clock to the Discord client while it is switched on. It is off until you turn it
on.

## Showcase

The app is a small set of pages behind the overlay: what happened this session,
what to shout about when it drops, what you are hunting, and the switches.

| Statistics | Alerts |
| --- | --- |
| Rates per hour, drops by rarity and grade, magic find, bosses and chests, and the Satanic Zone with its countdown. | One page in two columns: each rarity's sound and volume on the left with the announcement under it, the custom filter and its lists on the right. |
| ![Statistics](screenshots/statistics.png) | ![Alerts](screenshots/sound-filter.png) |

| Items | Shopping List |
| --- | --- |
| Every named item, what it takes to get one, and where it is worth farming. Cards or a table, whichever reads better. | The items you are actually after, so a drop you have been waiting weeks for is not one line among forty. |
| ![Items](screenshots/items.png) | ![Shopping list](screenshots/shopping-list.png) |

### The overlay, locked and not

Locked, it is click-through: the mouse goes to the game and only the numbers are
drawn. Unlocked, it grows a frame, a grip to drag it by and the buttons that
reset or hide it — and that is the one state in which it can take a click the
game was meant to have.

| Locked | Unlocked |
| --- | --- |
| ![Overlay, locked](screenshots/overlay-locked.png) | ![Overlay, unlocked](screenshots/overlay-unlocked.png) |

Settings is the last page, and the least interesting: the four or five things
people change, with the rest a click away under **More settings**.

![Settings](screenshots/settings.png)

### Skins

**Theme** in Settings changes what every window is wearing, without a restart.
Five of them, in two families.

*Hero Siege* is the game's own art — nine-sliced panels, painted buttons and the
game's pixel face — and *Ebontharn* is the same art in the season's colours.

The other three take the art off and let CSS draw the chrome, which is the same
idea reached twice: *Plain dark* and *Plain light* are upstream's, on shadcn's
zinc palette with a violet accent, and *Modern* is this fork's.

*Modern* is the one with no game art anywhere in it. Hairline borders, a radius,
flat surfaces on Tailwind's zinc ramp — the neutrals
[shadcn/ui](https://ui.shadcn.com) is built from — and the system's own UI font
in place of the pixel one. The sprites that are genuinely icons become line
icons, the figures are set in tabular digits so a column of them stops jittering
as it counts, and the eight crimson headings go neutral so the red is left
meaning a Satanic drop and a thing that has gone wrong. It is for the second
monitor: nothing about it is prettier, and all of it is easier to read.

| Statistics | Alerts |
| --- | --- |
| ![Statistics, modern skin](screenshots/modern-statistics.png) | ![Alerts, modern skin](screenshots/modern-alerts.png) |

| Items | Watchlist |
| --- | --- |
| ![Items, modern skin](screenshots/modern-items.png) | ![Watchlist, modern skin](screenshots/modern-watchlist.png) |

| Runs | Settings |
| --- | --- |
| ![Runs, modern skin](screenshots/modern-runs.png) | ![Settings, modern skin](screenshots/modern-settings.png) |

The overlay and the two windows that only appear over the game follow it too —
the pinned panel, the drop ticker, and the pillar the app throws up when
something worth it lands.

![The overlay, modern skin](screenshots/modern-overlay.png)

| The loot pillar | A Satanic Zone rotation |
| --- | --- |
| ![The loot pillar, modern skin](screenshots/modern-flourish.png) | ![A rotation, modern skin](screenshots/modern-satanic-zone.png) |

![The drop ticker, modern skin](screenshots/modern-ticker.png)


## Install

### Windows

Run the installer. It installs for the current user, so Windows never asks for
administrator rights.

HS Tracker needs [Npcap](https://npcap.com) to read the game's traffic. The
installer checks for it and opens the download page when it is missing. Install
it with the options it comes with, and in particular leave *Restrict Npcap
driver's access to Administrators only* unticked — with that on, HS Tracker is
refused the adapter and counts nothing.

Everything the app writes lives next to the executable, so the folder can be
copied to another machine as it is.

### Linux

```bash
sudo apt install ./HS\ Tracker_*_amd64.deb      # Debian, Ubuntu, Mint …
sudo dnf install ./HS\ Tracker-*.x86_64.rpm     # Fedora
```

The `.deb` is built on Ubuntu 22.04 and runs on it and on anything newer. The
`.rpm` is built on Fedora and wants glibc 2.39 or later, so it is for a current
Fedora rather than for RHEL and its rebuilds — those do not carry the WebKitGTK
4.1 this is drawn with either way. Everywhere else, the AppImage.

Either package grants the app the right to capture during installation. Settings
live in `~/.config/hs-tracker`.

The AppImage does not carry the wayland and xcb libraries it used to. They have
to match the compositor and the graphics driver of the machine it is running on,
and the copies it shipped stopped the host's own driver loading on Fedora and
Arch — four `EGL_BAD_PARAMETER` lines and no window. Every desktop that has GTK
has them already, so this costs nothing in practice; a machine built with no
wayland libraries at all is the one place the AppImage now needs something from
its host.

**Install a package if you want the numbers.** The AppImage runs anywhere, and
it is the one form that cannot be given the capture right — not by `setcap`, and
not by anything else.

There are two files you could try to grant it to, and neither one works.

Grant it to the `.AppImage` and nothing happens. Linux recomputes capabilities
at every `execve`, and the AppImage runtime execs the real binary out of the
mount it just made, so the right is gone before the app starts. The app runs
perfectly well and still counts nothing.

Grant it to the extracted binary instead and the app stops starting at all. A
binary carrying a capability is a privileged one, and the loader stops trusting
both of the ways an AppImage finds the libraries bundled inside it: the
`LD_LIBRARY_PATH` its own `AppRun` sets is dropped from the environment, and the
`$ORIGIN/../lib` runpath built into the binary is refused as well. It gets as
far as the first library the machine does not already have of its own:

```
hs-tracker: error while loading shared libraries: libwebkit2gtk-4.1.so.0:
cannot open shared object file: No such file or directory
```

And in ordinary use there is no file to try it on at all: the binary lives on a
mount that exists only while the app is running. A package has none of this —
its binary uses the distribution's own libpcap from a directory the loader
trusts however the program was started.

`sudo` does work, because nothing is granted at exec time and the library path
survives — but settings, runs and sounds are then written to root's home instead
of yours:

```bash
chmod +x ./HS\ Tracker_*.AppImage               # a fresh download has no execute bit
./HS\ Tracker_*.AppImage --appimage-extract     # gives ./squashfs-root
sudo ./squashfs-root/AppRun
```

## Using it

The tray icon is the control centre: click it to hide or bring back whatever was
last on screen. The overlay carries its own strip of icons down the right-hand
edge for the same things.

| | |
| --- | --- |
| Show / hide | tray click, or `Ctrl+Shift+O` |
| Overlay ⇄ dashboard | **Compact mode** in the sidebar, **Dashboard** in the overlay menu |
| Lock the overlay | the lock icon on hover, or `Ctrl+Shift+L` |
| Reset the session | the overlay button, the tray, or `Ctrl+Shift+R` |
| Pause the session | the clock in Statistics, the tray, or `Ctrl+Shift+P` |

Counting starts once the game reports your character, a moment after you enter a
zone. Gold, experience and kills only travel when the game saves, so between
saves those three sit still while drops keep arriving — that is the game, not a
stuck counter.

## Streaming it

The app draws four windows. Each is transparent and can be captured on its own:

| Window | What it is | On screen |
| --- | --- | --- |
| `HS Tracker — Overlay` | the compact panel | while you are in compact mode |
| `HS Tracker Ticker` | drop names, under the overlay | for a few seconds after a drop |
| `HS Tracker Flourish` | the announcement for a big drop, and for the zone rotating | while it plays |
| `HS Tracker` | the dashboard | while you are in dashboard mode |

### Capturing a window

1. Add a **Window Capture** and pick the window from the list.
2. Set **Capture Method** to **Windows 10 (1903 and up)** — that is the one that
   keeps the transparency.
3. Set **Window Match Priority** to **Window title must match**.

## If something does not work

**Send the log.** The app writes what goes wrong to `hs-tracker.log` beside its
settings — panics, anything a panel throws, and, when nothing is being counted,
what the capture actually saw. **About** gives the path and a button that opens
the folder.

It is meant to be pasted into a chat, so it is worth knowing what is in it:
which version started, which Windows and which WebView2, where the app is
installed, the names of your network adapters, and the addresses of the game's
servers it is filtering on. No account name, no character, nothing the game
said. The install path is the one line that can carry your Windows username, if
that is where you put it.

`debug-capture.jsonl`, in the same folder, is a different thing. It is the
packet log from Settings, off unless you switch it on, and it holds what the
game actually sent: your account id, your character, your addresses. Do not
paste that one anywhere you would not paste your account name.

**The numbers stay at zero.** First, the app has to be allowed to read network
traffic: on Windows that means Npcap, on Linux the packaged install grants it
and an AppImage cannot be granted it at all — see above. Npcap's own installer has a
box marked *Restrict Npcap driver's access to Administrators only* — ticked, it
refuses HS Tracker the adapter, and the app says so rather than claiming Npcap
is missing.

If the capture is running and still nothing is counted, the dashboard says so
after ninety seconds and the log says what it saw. Three causes account for
nearly all of it:

- **A route optimiser.** ExitLag and its kind redirect the game's packets, so
  the connections Windows reports are not the ones on the wire. Turn on **Read
  every connection** — the banner offers the switch where you are standing.
- **A VPN that encrypts.** If the game only reaches its server through a tunnel,
  what leaves your machine is encrypted and there is nothing to read from
  outside. A VPN that installs an ordinary network adapter — OpenVPN, WireGuard
  — can be read; a VPN connection set up in Windows' own settings cannot.
- **The game is at the login screen.** Gold, experience and kills travel with a
  save, and a save wants a character in the world. The log says outright when
  everything it heard was encrypted web traffic, which is what that looks like.

**The satanic zone alert came late, or not until I moved.** It arrives when the
game next asks the server where the zone is, and the game asks as part of saving.
Playing saves constantly, so while you are killing things the alert lands within
seconds of the rotation. Standing still saves nothing, so nothing asks — and the
rotation waits. Timed here across two real rotations: **thirteen seconds while
playing, twenty-four minutes standing away from the keyboard.**

Entering or reloading a zone makes the game ask straight away, which is why
moving appears to summon the alert. This one is not fixable from here: the app
reads the game's traffic and has no way to ask the server anything itself.

Zones rotate on the half hour, at :00 and :30 — Statistics counts down to the
next one, and that countdown is right whether or not the alert has arrived yet.

### On Linux

**The game covers the overlay.** Run Hero Siege **windowed** or **borderless**
rather than in exclusive fullscreen, and the overlay will stay put.

This is not something the app can win. Every Linux desktop puts the *active
fullscreen window* in a layer above the one that holds always-on-top windows, so
while the game is fullscreen no overlay of any kind can sit on top of it —
Discord's overlay fails there the same way. Asking for always-on-top is honoured
and still loses.

If you would rather keep exclusive fullscreen, KDE can settle it from its side:
*System Settings → Window Management → Window Rules*, add a rule matching Hero
Siege with the property **Fullscreen → Force → No**. The game still fills the
screen; it just stops claiming the top layer.

**There is no overlay at all.** A Wayland session cannot host one: an application
there may not place a window above another program's fullscreen window, read the
pointer outside itself or take global hotkeys. Settings offers **Enable the
overlay — restart through XWayland**, which brings all of it back and is
remembered for every later start. Hero Siege runs through XWayland too when it
runs through Proton, so the two meet in one X server.

**An NVIDIA card, and no window appears — just the tray icon.** WebKitGTK, which
draws every window here, composites through a DMA-BUF renderer that the
proprietary NVIDIA driver does not survive: its web process dies and the desktop
reports a crashed `WebKitWebProcess`. The app switches that renderer off by
itself when it finds the driver on the machine, so an up-to-date build should
simply work. If a window still refuses to appear, the heavier escape hatch is:

```bash
WEBKIT_DISABLE_COMPOSITING_MODE=1 hs-tracker
```

## Development

See [DEVELOPING.md](DEVELOPING.md).

## Credits

- The overlay is skinned with Hero Siege sprites. Hero Siege is © Panic Art
  Studios. This project is not affiliated with or endorsed by them.
- The protocol work builds on
  [hero-siege-stats](https://github.com/GuilhermeFaga/hero-siege-stats) and
  [Hero-Siege-Companion](https://github.com/DemonSkye/Hero-Siege-Companion).
- Item names, rarities and grades are generated from Hero siege datamined data

Released under the [MIT license](LICENSE).

<p align="center">
  <img src="src-tauri/icons/discord/cover.png" width="720" alt="HS Tracker — session tracker for Hero Siege">
</p>

<p align="center">
  Gold, experience, kills and every drop — counted while you farm,
  <br>in the game's own skin.
</p>

<p align="center">
  <a href="../../releases"><b>➡️ Download for Windows &amp; Linux ⬅️</b></a>
</p>

![The overlay over a running game](screenshots/overlay.png)

## What it does

| | |
| --- | --- |
| **Overlay** | A small always-on-top panel with the run so far. Lock it and it becomes click-through, so it never eats a click meant for the game. |
| **Statistics** | Gold, xp and kills per hour, drops by rarity, magic find, bosses and chests, what rolls better in the zone you are standing in, and the Satanic Zone with a countdown to the next rotation. |
| **Sound alerts** | Its own sound per rarity, and named lists of specific items with sounds of their own. Alerts fire the moment an item hits the ground. |
| **Drop flourish** | Optional: something SS-grade drops and the game's own loot pillar plays over the screen, wherever you have put it. |
| **Items** | Every named item, its drop chance, and the places it rolls better in. Search by name, rarity or kind. |
| **Runs** | Every finished session kept — the rates, the finds, where the time went. **Copy card** turns one into a picture you can paste into a chat. |
| **Pause** | By hand, or by itself after five quiet minutes, so a break does not end up in the per-hour figures. |
| **OBS** | Optional: the overlay served as a page on your own machine, to add in OBS as a Browser Source — transparent, and the same one you see. |
| **About** | The version you are running, who made it, and a button that asks GitHub whether a newer release is out. |
| **Discord** | Optional: while the game is open, your friends see the zone, the drops and the timer. |

It reads the game's network traffic. It never touches the game process, never
injects anything, and sends nothing anywhere — everything stays on your machine.
The one exception is the **About** section's update check, which asks GitHub for
the newest release and only when you press the button.

## Showcase

| Statistics | Sound Filter |
| --- | --- |
| ![Statistics](screenshots/statistics.png) | ![Sound Filter](screenshots/sound-filter.png) |

| Sounds | Shopping List |
| --- | --- |
| ![Sounds](screenshots/sounds.png) | ![Shopping list](screenshots/shopping-list.png) |

## Install

### Windows

Run the installer. It installs for the current user, so Windows never asks for
administrator rights.

HS Tracker needs [Npcap](https://npcap.com) to read the game's traffic. The
installer checks for it and offers to fetch it when it is missing — its defaults
are fine.

Everything the app writes lives next to the executable, so the folder can be
copied to another machine as it is.

### Linux

```bash
sudo apt install ./HS\ Tracker_*_amd64.deb      # Debian, Ubuntu, Mint …
sudo dnf install ./HS\ Tracker-*.x86_64.rpm     # Fedora
```

Either package grants the app the right to capture during installation. Settings
live in `~/.config/hs-tracker`.

The AppImage runs anywhere but cannot carry that right, so it needs one line by
hand:

```bash
./HS\ Tracker_*.AppImage --appimage-extract     # gives ./squashfs-root
sudo setcap cap_net_raw,cap_net_admin=eip squashfs-root/usr/bin/hs-tracker
./squashfs-root/AppRun
```

## Using it

The tray icon is the control centre: click it to hide or bring back whatever was
last on screen. Right-clicking the overlay opens the same menu.

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
| `HS Tracker Flourish` | the announcement for a big drop | while it plays |
| `HS Tracker` | the dashboard | while you are in dashboard mode |

### Capturing a window

1. Add a **Window Capture** and pick the window from the list.
2. Set **Capture Method** to **Windows 10 (1903 and up)** — that is the one that
   keeps the transparency.
3. Set **Window Match Priority** to **Window title must match**.

That third step matters. On any other setting OBS falls back to *another window
of the same type* when it cannot find the one you chose, and every window here is
the same type — so you get the dashboard instead of the overlay.

A window that is not on screen cannot be captured at all. The ticker and the
announcement come and go by design: their sources sit empty and fill when a drop
happens, which is what you want. The overlay is a different matter — it is hidden
while the dashboard is up, so its source stays empty until you switch back to
compact mode. For the announcement there is **Keep its window on screen so OBS
can capture it** in Settings, which leaves it there drawing nothing.

### Or a browser source

If you would rather these were on the stream and not on your screen, switch on
**Serve the overlay to OBS** in Settings. **About** then lists an address per
view — overlay, dashboard, announcement — and the size to give the source.

Add them as **Browser Sources**. The pages are the app's own, transparent, served
on `127.0.0.1` and nowhere else, and they only ever show what the app already
shows.

## If something does not work

**The numbers stay at zero.** The app has to be allowed to read network traffic:
on Windows that means Npcap, on Linux the packaged install does it for you and an
AppImage needs the `setcap` line above.

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
- Item names, rarities and grades are generated from
  [hero-siege-helper](https://hero-siege-helper.vercel.app)'s datamined data and
  are not redistributed here.

Released under the [MIT license](LICENSE).

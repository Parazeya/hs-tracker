# Linux: Arch Linux and Manjaro

Use a native package when tracking live game statistics. An AppImage can show
the dashboard but its launcher cannot retain the file capability required for
packet capture. Running the whole GUI with sudo is unnecessary with a native
package and writes settings under the wrong account.

## Native Arch/Manjaro build

Build on the target distribution and repository snapshot. The QA package built
on current Arch is an Arch artifact; Manjaro can lag Arch's glibc and WebKit ABI,
so that binary is not a universal Manjaro download. For Manjaro verification,
run the build and packaging commands on that Manjaro installation.

Install the build and runtime dependencies as usual through pacman:

```sh
sudo pacman -S --needed base-devel rust nodejs npm gtk3 webkit2gtk-4.1 libpcap openssl librsvg
# Optional: enables the system tray; the dashboard also works without it.
sudo pacman -S --needed libappindicator
npm ci
npm run build
cargo build --release --features tauri/custom-protocol --manifest-path src-tauri/Cargo.toml --locked
bash scripts/package-arch.sh
sudo pacman -U dist-linux/hs-tracker-local-*.pkg.tar.zst
hs-tracker
```

The package installs `/usr/bin/hs-tracker` and grants only `cap_net_raw` to that
binary. Start Tracker as your normal user. A manual binary needs the same grant
after each rebuild or replacement:

```sh
sudo setcap cap_net_raw=ep /absolute/path/to/hs-tracker
getcap /absolute/path/to/hs-tracker
```

Debian and RPM packages are built for different libpcap sonames. Do not unpack
a `.deb` and use its binary on Arch: Debian expects `libpcap.so.0.8`, whereas
Arch supplies `libpcap.so.1`. The packaging script checks this dependency.

## AppImage startup and display backends

If an AppImage reports a missing `fusermount` or FUSE setup, install `fuse2`, or
run it without mounting:

```sh
APPIMAGE_EXTRACT_AND_RUN=1 ./HS.Tracker_*.AppImage
```

This addresses startup only; it does not grant packet-capture rights.

The AppImage GTK hook must preserve `GDK_BACKEND`. Older published images
unconditionally forced X11, which crashes GTK in a Wayland session without
XWayland and also overrides the user's backend selection. The local Docker
build and the CI artifact build now apply the same cleanup. Native Wayland can
show the dashboard; the always-on-top overlay requires X11/XWayland. No sandbox
disable flags are needed for the verified cases.

Global hotkeys are another separate limitation: the current hotkey dependency
uses X11 on Linux. The native Wayland dashboard check does not establish that
global shortcuts work in a session without XWayland.

When reporting a failure, include the terminal output and
`~/.config/hs-tracker/hs-tracker.log`, plus your desktop, GPU/driver and whether
the session uses Wayland or X11. Check the log for `the interface is up`:
starting the process alone does not establish that WebKit rendered the UI.

## Verification performed on 2026-11-15

An isolated Arch x86_64 VM ran the actual release build under Xvfb/Openbox and
native Wayland under Weston 15 with no X server. GTK 3.24.52, WebKitGTK 2.54.2
and libpcap 1.11.0 were used. No tray indicator library was installed initially.

- Without an indicator library: the compiled dashboard rendered; closing its
  window exited cleanly, including when compact mode had been saved previously.
  Switching to compact mode and pressing Alt+F4 restored the dashboard instead
  of leaving a hidden process; closing that dashboard then exited cleanly.
- Without capture capability: the visible dashboard explained the missing right.
- With only `cap_net_raw`, running as uid 1000: real loopback TCP traffic was
  captured over both IPv4 and IPv6. Two identical gold messages counted twice
  in total, giving 200 rather than 100 or 400.
- A live fixture named `wine64`, whose first game argument used a Windows
  backslash path, was detected and its traffic counted. This exercises the
  actual Linux process table and capture path; it is not a real Proton game run.
- The public v1.1.15 AppImage (SHA256
  `121e175c33f0ca8fa96e7dab566acd7a61bcdd4e4d8f8888b2c7e5f85fbbafee`)
  exited with a GTK initialization panic in pure Wayland. The same image,
  repacked with the GTK hook fix, rendered its dashboard there without FUSE.
- A native Arch package was built, installed through pacman, and its installed
  binary passed the IPv6 and Wine-shaped traffic checks as a normal user. The
  package postinstall supplied its capture capability.

Installing the indicator library afterward also allowed startup, but a visible
tray icon was not verified: the headless compositor has no tray host, and the
indicator library logged a session-bus autospawn restriction under `AT_SECURE`
when the executable carried a file capability. Treat tray behavior on a real
desktop as a separate outstanding check.

These results cover an Arch VM and software rendering. Real Steam/Proton game
sessions, Manjaro itself, KDE/GNOME and physical AMD/NVIDIA/Intel GPUs still need
their own verification; the VM results do not establish compatibility for all
those combinations.

#!/usr/bin/env bash
# Package a locally built Arch/Manjaro binary without requiring AppImage/FUSE.
# Run as a normal user on Arch, after npm run build and cargo build --release
# --features tauri/custom-protocol --manifest-path src-tauri/Cargo.toml.
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
binary="$(readlink -f "${1:-$root/src-tauri/target/release/hs-tracker}")"
out="${2:-$root/dist-linux}"
command -v makepkg >/dev/null || { echo 'makepkg is required; run this on Arch/Manjaro' >&2; exit 1; }
[ "$(id -u)" != 0 ] || { echo 'Run makepkg as a normal user, not root' >&2; exit 1; }
[ -f "$binary" ] || { echo "No binary: $binary" >&2; exit 1; }
# Debian builds ask for libpcap.so.0.8 and cannot use Arch's libpcap.so.1.
readelf -d "$binary" | grep -F '[libpcap.so.1]' >/dev/null || {
  echo 'Expected an Arch-built binary linked to libpcap.so.1' >&2; exit 1;
}
version="$(node -p "require(process.argv[1]).version" "$root/package.json")"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
cp "$binary" "$work/hs-tracker"
cp "$root/src-tauri/icons/128x128.png" "$work/hs-tracker.png"
cp "$root/LICENSE" "$work/LICENSE"
cat >"$work/hs-tracker.desktop" <<'DESKTOP'
[Desktop Entry]
Type=Application
Name=HS Tracker
Exec=hs-tracker
Icon=hs-tracker
Categories=Game;Utility;
Terminal=false
DESKTOP
cat >"$work/hs-tracker.install" <<'INSTALL'
post_install() {
  if ! setcap cap_net_raw=ep /usr/bin/hs-tracker; then
    echo 'HS Tracker needs cap_net_raw for capture; the dashboard will explain how to grant it.'
  fi
}
post_upgrade() { post_install; }
INSTALL
cat >"$work/PKGBUILD" <<PKGBUILD
pkgname=hs-tracker-local
pkgver=$version
pkgrel=1
pkgdesc='HS Tracker local build for Arch Linux and Manjaro'
arch=('x86_64')
url='https://github.com/Parazeya/hs-tracker'
license=('MIT')
depends=('gtk3' 'webkit2gtk-4.1' 'libpcap' 'openssl' 'libcap' 'librsvg')
optdepends=('libappindicator: system tray icon')
provides=('hs-tracker')
conflicts=('hs-tracker')
options=('!strip')
install=hs-tracker.install
source=('hs-tracker' 'hs-tracker.png' 'hs-tracker.desktop' 'LICENSE')
sha256sums=('$(sha256sum "$work/hs-tracker" | cut -d' ' -f1)' '$(sha256sum "$work/hs-tracker.png" | cut -d' ' -f1)' '$(sha256sum "$work/hs-tracker.desktop" | cut -d' ' -f1)' '$(sha256sum "$work/LICENSE" | cut -d' ' -f1)')
package() {
  install -Dm755 hs-tracker "\$pkgdir/usr/bin/hs-tracker"
  install -Dm644 hs-tracker.png "\$pkgdir/usr/share/icons/hicolor/128x128/apps/hs-tracker.png"
  install -Dm644 hs-tracker.desktop "\$pkgdir/usr/share/applications/hs-tracker.desktop"
  install -Dm644 LICENSE "\$pkgdir/usr/share/licenses/\$pkgname/LICENSE"
}
PKGBUILD
mkdir -p "$out"
out="$(readlink -f "$out")"
(cd "$work" && PKGDEST="$out" makepkg --force --noconfirm)
echo "Arch package written to $out; install with sudo pacman -U <package.pkg.tar.zst>"

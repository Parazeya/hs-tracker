#!/usr/bin/env bash
# Runs inside the image. /src is the checkout, mounted read-only; /out is where
# the packages are left.
#
# The source is copied rather than built in place, and node_modules is never
# shared with the host: npm resolves platform-specific optional packages, so a
# tree installed on Windows carries @rollup/rollup-win32 and no Linux binary at
# all. Copying costs a second and removes the whole class of problem.
set -euo pipefail

BUNDLES="${BUNDLES:-deb}"

# An .rpm is only built where an .rpm will run.
#
# Tauri writes the archive itself rather than calling rpmbuild, so this image
# will cheerfully produce one — and the binary inside it is linked against
# Debian's libpcap. On Fedora the package installs, satisfies its declared
# dependency, and then dies in the loader looking for libpcap.so.0.8. Three
# releases shipped that way before a player pasted the error.
#
# The check is here rather than in build-linux.mjs so that running the image by
# hand cannot get round it.
if [[ ",${BUNDLES}," == *",rpm,"* && "${PKG_FAMILY:-deb}" != "rpm" ]]; then
  echo "refusing to bundle an .rpm in a ${PKG_FAMILY:-deb} image:" >&2
  echo "  the binary would be linked against Debian's libpcap and would not" >&2
  echo "  start on Fedora. Use docker/Dockerfile.fedora - npm run deb -- --rpm" >&2
  echo "  does that for you." >&2
  exit 2
fi

echo "==> copying the checkout"
rsync -a --delete \
  --exclude node_modules --exclude dist --exclude target \
  --exclude .git --exclude linux-packages \
  /src/ /build/
cd /build

echo "==> npm ci"
npm ci --no-audit --no-fund

echo "==> cargo test"
cargo test --manifest-path src-tauri/Cargo.toml

# /target is a volume that outlives the run and `find` below takes whatever
# it holds, so a package left by an older version rides along into /out.
# Bundling is seconds; the compile it depends on is not touched.
rm -rf "${CARGO_TARGET_DIR}/release/bundle"

echo "==> bundling: ${BUNDLES}"
tauri build --bundles "${BUNDLES}"

# The AppImage carries libraries that have to come from the machine it runs on,
# and shadowing the host's wayland/xcb stack with ubuntu:22.04 copies is what
# made it abort with EGL_BAD_PARAMETER on Fedora and Arch. trim-appimage.sh
# takes them back out. The .deb and .rpm bundle no libraries at all and never
# had the problem, so this touches only the AppImage.
if [[ ",${BUNDLES}," == *",appimage,"* ]]; then
  # Only the Ubuntu image carries the script and squashfs-tools. Asking the
  # Fedora one for an AppImage would otherwise die on a missing file.
  [ -x /usr/local/bin/trim-appimage.sh ] || {
    echo "this image cannot finish an AppImage: trim-appimage.sh is not in it." >&2
    echo "  AppImages are built by docker/Dockerfile - npm run deb -- --appimage." >&2
    exit 2
  }
  echo "==> trimming the AppImage"
  while IFS= read -r -d '' img; do
    echo "    $(basename "$img")"
    /usr/local/bin/trim-appimage.sh "$img"
    # The bundler signed the AppImage it wrote, and trimming wrote a different
    # one. Sign what actually leaves here, or the updater refuses it.
    if [ -n "${TAURI_SIGNING_PRIVATE_KEY:-}" ]; then
      tauri signer sign "$img" >/dev/null
    fi
  done < <(find "${CARGO_TARGET_DIR}/release/bundle" -type f -name '*.AppImage' -print0)
fi

echo "==> collecting"
mkdir -p /out
found=0
while IFS= read -r -d '' f; do
  cp -f "$f" /out/
  echo "    $(basename "$f")"
  found=1
done < <(find "${CARGO_TARGET_DIR}/release/bundle" -type f \( -name '*.deb' -o -name '*.rpm' -o -name '*.AppImage' -o -name '*.sig' \) -print0)
[ "$found" = 1 ] || { echo "nothing was produced" >&2; exit 1; }

# The mount is root-owned inside; hand the files back to whoever owns /out.
if [ -n "${HOST_UID:-}" ]; then
  chown "${HOST_UID}:${HOST_GID:-$HOST_UID}" /out/* 2>/dev/null || true
fi
echo "==> done"

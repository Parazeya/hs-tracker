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

echo "==> copying the checkout"
rsync -a --delete \
  --exclude node_modules --exclude dist --exclude target \
  --exclude .git --exclude linux-packages \
  /src/ /build/
cd /build

echo "==> npm ci"
npm ci --no-audit --no-fund

echo "==> front end"
npm run build

echo "==> cargo test"
cargo test --manifest-path src-tauri/Cargo.toml

echo "==> bundling: ${BUNDLES}"
tauri build --bundles "${BUNDLES}"

echo "==> collecting"
mkdir -p /out
found=0
while IFS= read -r -d '' f; do
  cp -f "$f" /out/
  echo "    $(basename "$f")"
  found=1
done < <(find "${CARGO_TARGET_DIR}/release/bundle" -type f \( -name '*.deb' -o -name '*.rpm' -o -name '*.AppImage' \) -print0)
[ "$found" = 1 ] || { echo "nothing was produced" >&2; exit 1; }

# The mount is root-owned inside; hand the files back to whoever owns /out.
if [ -n "${HOST_UID:-}" ]; then
  chown "${HOST_UID}:${HOST_GID:-$HOST_UID}" /out/* 2>/dev/null || true
fi
echo "==> done"

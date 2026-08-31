#!/usr/bin/env bash
# Take out of the AppImage the libraries that have to come from the machine it
# runs on, then pack it up again. Runs inside the image, after `tauri build`.
#
# WHY THIS EXISTS
#
# linuxdeploy copies every dependency it walks into usr/lib/, and the compiled
# AppRun.wrapped then exports all ten bundle directories as LD_LIBRARY_PATH.
# That path applies to every library the process ever loads - including the
# host's own graphics driver, which the bundle deliberately does NOT ship:
# libEGL, libGL, libgbm, libdrm, libX11 and libxcb.so.1 are all correctly left
# to the host. So the host's Mesa ends up taking 15 of its 39 dependencies from
# inside the AppImage, and one of them is fatal.
#
# The bundled libwayland-client.so.0 is wayland 1.20.0, which is what
# ubuntu:22.04 has. Mesa from 1.23 on calls wl_display_create_queue_with_name,
# and four other symbols are missing besides. What that turns into depends on
# how the host links:
#
#   Debian/Ubuntu (lazy binding)  libEGL_mesa loads and dies mid-call.
#   Fedora (-z now)               the dlopen fails outright, libglvnd is left
#                                 with no vendor, and eglGetDisplay() answers
#                                 EGL_BAD_PARAMETER. WebKit prints
#                                 "Could not create default EGL display:
#                                 EGL_BAD_PARAMETER. Aborting..." once per web
#                                 process, and no window ever appears.
#
# That is the bug players reported from KDE. It needs a host that is both
# wayland >= 1.23 and -z now - Fedora, RHEL's family, Arch - which is why it
# looked like "works badly for some users" and why it was never new to any one
# version. It was never the app, the compile or WebKitGTK: the .rpm is the same
# compile, bundles no libraries at all, and draws on the same machine where the
# AppImage aborts four times.
#
# No environment variable reaches it. WEBKIT_DISABLE_DMABUF_RENDERER=1,
# WEBKIT_DISABLE_COMPOSITING_MODE=1 and LIBGL_ALWAYS_SOFTWARE=1 were each
# measured and each still aborts: the failure is in the loader, before any
# renderer is chosen.
#
# WHAT COMES OUT, AND WHY THESE
#
# Two families, on one principle - a library that has to agree with something
# outside the AppImage cannot be carried inside it.
#
#   libwayland-*              must match the compositor running the session.
#   libxcb-randr/render/shm   libxcb.so.1 itself is already left to the host,
#                             so bundling its siblings guarantees a split xcb
#                             family.
#
# Removing libwayland-client.so.0 on its own is not enough, and is worse than
# doing nothing: measured, the EGL abort goes away and the app then draws
# nothing at all for 20 seconds - a loud failure becomes a silent blank window,
# and the app's own soft-render breadcrumb learns the wrong lesson from it. All
# four wayland libraries have to go together.
#
# libwebkit2gtk-4.1.so.0 keeps hard DT_NEEDED entries on libwayland-client.so.0
# and libwayland-server.so.0, so the host must supply them. Every GTK desktop
# does - libgtk-3 depends on libwayland-client0 - but a deliberately X11-only
# machine would not, and that is the one case this trade costs anything.
set -euo pipefail

img="$(readlink -f "${1:-}")"   # the working directory changes below
[ -f "$img" ] || { echo "trim-appimage: no such file: ${1:-}" >&2; exit 1; }

# Host-owned, every one of them. A list rather than a pattern, so that a library
# joining the bundle later cannot quietly join this set too.
HOST_OWNED=(
  libwayland-client.so.0
  libwayland-server.so.0
  libwayland-egl.so.1
  libwayland-cursor.so.0
  libxcb-randr.so.0
  libxcb-render.so.0
  libxcb-shm.so.0
)

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
cd "$work"

# --appimage-offset asks the runtime where its own squashfs starts. The bytes
# before it are the runtime, and they are reused as they are, so nothing has to
# be downloaded to pack the image up again.
offset="$("$img" --appimage-offset)"
[ "$offset" -gt 0 ] 2>/dev/null || { echo "trim-appimage: could not read the offset" >&2; exit 1; }

# Match what appimagetool used, compressor and block size both: a file packed
# with a compressor the runtime does not carry mounts here and on nobody else's
# machine.
#
# The image ends up about 0.4 MB larger all the same, and that is expected.
# Measured on 1.0.4: the seven libraries are only 86 KB of the archive, while
# repacking with this image's mksquashfs 4.5 costs 500 KB against whatever
# appimagetool used - 0.3% of 138 MB, and not worth carrying a newer
# squashfs-tools to recover.
sb="$(unsquashfs -s -o "$offset" "$img")"
comp="$(printf '%s\n' "$sb" | sed -n 's/^Compression[[:space:]][[:space:]]*//p' | head -1)"
block="$(printf '%s\n' "$sb" | sed -n 's/^Block size[[:space:]][[:space:]]*//p' | head -1)"
[ -n "$comp" ] || comp=gzip
[ -n "$block" ] || block=131072
# appimagetool asks zstd for its top level; mksquashfs would default to 15.
# Written as an `if` because an `&&` that came out false would be the script's
# last word under `set -e`.
extra=()
if [ "$comp" = zstd ]; then extra=(-Xcompression-level 22); fi

"$img" --appimage-extract >/dev/null

removed=0
for lib in "${HOST_OWNED[@]}"; do
  # every bundle directory, not only usr/lib - the loader searches all ten
  while IFS= read -r f; do
    rm -f "$f"
    echo "    removed ${f#squashfs-root/}"
    removed=$((removed + 1))
  done < <(find squashfs-root -name "$lib" -not -type d 2>/dev/null)
done

# Nothing found means linuxdeploy stopped bundling these of its own accord and
# this script has become dead weight. Say so rather than silently repacking.
if [ "$removed" = 0 ]; then
  echo "trim-appimage: none of the host-owned libraries were in the bundle;" >&2
  echo "  linuxdeploy's behaviour has changed and this script can go." >&2
  exit 1
fi

head -c "$offset" "$img" > runtime
mksquashfs squashfs-root fs.squashfs -root-owned -noappend -no-progress -quiet -b "$block" -comp "$comp" "${extra[@]}" >/dev/null
cat runtime fs.squashfs > trimmed
chmod +x trimmed
mv -f trimmed "$img"

echo "    $removed libraries out; repacked -comp $comp -b $block, $(stat -c%s "$img") bytes"

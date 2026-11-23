#!/usr/bin/env bash
# linuxdeploy's GTK hook forces X11 even on a session without XWayland and
# overrides both the user's GDK_BACKEND and the app's backend restart choice.
# Keep GTK's normal selection; HS Tracker already handles its saved choice.
set -euo pipefail

root="${1:?usage: fix-appimage-backend.sh EXTRACTED_APPDIR}"
hook="$root/apprun-hooks/linuxdeploy-plugin-gtk.sh"
[ -f "$hook" ] || { echo "AppImage GTK hook is missing: $hook" >&2; exit 1; }

if grep -Eq '^export GDK_BACKEND=x11([[:space:]#]|$)' "$hook"; then
  sed -i -E 's/^export GDK_BACKEND=x11([[:space:]#].*)?$/# GTK backend is selected by HS Tracker and the user./' "$hook"
  echo "    removed linuxdeploy's unconditional X11 backend override"
fi

# A plugin update must not silently restore this failure through new syntax.
if grep -Eq '^[[:space:]]*(export[[:space:]]+)?GDK_BACKEND=' "$hook"; then
  echo "AppImage GTK hook still assigns GDK_BACKEND; review its new format" >&2
  exit 1
fi

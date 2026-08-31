#!/bin/sh
# HS Tracker reads the game's traffic through libpcap, which needs raw socket
# rights. Granting them to the binary keeps the app out of root — without this
# the capture fails and the overlay reports that it cannot listen.
#
# This is for the packages only, and an AppImage cannot be given the same right
# by any route. Granting it to the .AppImage file achieves nothing - the right
# is recomputed away at the execve of the binary inside, so the app starts and
# still cannot capture. Granting it to that inner binary instead loads it as
# AT_SECURE, and the loader then refuses both ways the bundle is found - the
# LD_LIBRARY_PATH its own AppRun sets, and the $ORIGIN/../lib runpath in the
# binary - so the app stops starting at all. Both measured in a container; the
# README and the app itself say so too. On an AppImage, run it with sudo.
#
# Only cap_net_raw is asked for. cap_net_admin buys promiscuous mode, which the
# capture never turns on, and the `i` of `=eip` does nothing without ambient
# capabilities — asking for either was asking for more than the job needs.
#
# The result is verified and printed. This used to send its own failure to
# /dev/null and exit 0 regardless, while the README told the user the package
# had granted the right: the one message that would have explained an app that
# counts nothing was the one being thrown away.
set -e

granted=""
for bin in /usr/bin/hs-tracker "/usr/bin/HS Tracker"; do
    [ -x "$bin" ] || continue
    if setcap cap_net_raw=ep "$bin"; then
        granted="$(getcap "$bin" 2>/dev/null || echo "$bin")"
        echo "HS Tracker: $granted"
    else
        echo "HS Tracker: could not grant cap_net_raw to $bin." >&2
        echo "  Without it the app cannot read the game's traffic. Grant it with:" >&2
        echo "    sudo setcap cap_net_raw=ep '$bin'" >&2
    fi
done

if [ -z "$granted" ]; then
    echo "HS Tracker: no installed binary found to grant capture rights to." >&2
fi

# Never fail the package install over this: the app runs, it just cannot count.
exit 0

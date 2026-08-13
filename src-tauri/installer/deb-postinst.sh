#!/bin/sh
# HS Tracker reads the game's traffic through libpcap, which needs raw socket
# rights. Granting them to the binary keeps the app out of root — without this
# the capture fails and the overlay reports that it cannot listen.
#
# An AppImage cannot do this for itself: there the user runs the same setcap
# line by hand once.
set -e

for bin in /usr/bin/hs-tracker "/usr/bin/HS Tracker"; do
    if [ -x "$bin" ]; then
        setcap cap_net_raw,cap_net_admin=eip "$bin" 2>/dev/null || \
            echo "HS Tracker: could not grant cap_net_raw; run setcap by hand or start it as root" >&2
    fi
done

exit 0

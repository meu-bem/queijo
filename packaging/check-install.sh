#!/bin/sh
# What must be true after Queijo's package is installed. Run inside the container by
# scripts/distro-test.sh.
set -eu
fail() { echo "FAIL: $*" >&2; exit 1; }

queijo --version || fail "queijo does not start"

# The libraries it opens at run time are the package's business; the ones it is linked
# against must all be there.
if ldd /usr/bin/queijo | grep -q 'not found'; then
    ldd /usr/bin/queijo | grep 'not found'
    fail "a library is missing"
fi

for file in \
    /usr/share/applications/queijo.desktop \
    /usr/share/applications/queijo-settings.desktop \
    /usr/share/icons/hicolor/256x256/apps/queijo.png \
    /usr/share/gnome-shell/extensions/queijo@queijo.app/extension.js \
    /usr/share/gnome-shell/extensions/queijo-gnome45@queijo.app/extension.js; do
    [ -f "$file" ] || fail "$file was not installed"
done

doctor=$(QUEIJO_NO_NOTIFY=1 queijo --doctor 2>&1) || fail "--doctor failed"
echo "$doctor" | sed -n 1,4p
# Which libraries `--doctor` thinks are missing, with no display: those for Wayland and X11
# are not asked about, so only the common ones count.
if echo "$doctor" | grep -E '^  lib.*MISSING'; then
    fail "the package did not bring a library that --doctor looks for"
fi
echo "ok"

#!/bin/sh
# Installs and enables the Queijo helper GNOME Shell extension from a source checkout.
# There are two, because GNOME 45 changed how extensions are written:
#   queijo@queijo.app             GNOME 42 to 44
#   queijo-gnome45@queijo.app     GNOME 45 and later
# The one for the GNOME you run is picked; give `legacy` or `gnome45` to choose.
# On Wayland the Shell only picks up new extensions at login, so log out and
# back in afterwards.
set -e
here=$(cd "$(dirname "$0")" && pwd)

choice="${1:-}"
if [ -z "$choice" ]; then
    major=$(gnome-shell --version 2>/dev/null | sed -n 's/^GNOME Shell \([0-9][0-9]*\).*/\1/p')
    if [ -z "$major" ]; then
        echo "Could not tell which GNOME Shell this is; run: $0 legacy   or   $0 gnome45" >&2
        exit 1
    fi
    if [ "$major" -ge 45 ]; then choice=gnome45; else choice=legacy; fi
fi
case "$choice" in
    legacy) UUID=queijo@queijo.app; SRC="$here/$UUID" ;;
    gnome45) UUID=queijo-gnome45@queijo.app; SRC="$here/esm/$UUID" ;;
    *) echo "usage: $0 [legacy|gnome45]" >&2; exit 1 ;;
esac

DEST="$HOME/.local/share/gnome-shell/extensions/$UUID"
rm -rf "$DEST"
mkdir -p "$DEST"
cp "$SRC/"* "$DEST/"
CURRENT=$(gsettings get org.gnome.shell enabled-extensions)
case "$CURRENT" in
    *"'$UUID'"*) ;;
    "@as []") gsettings set org.gnome.shell enabled-extensions "['$UUID']" ;;
    *) gsettings set org.gnome.shell enabled-extensions "${CURRENT%]}, '$UUID']" ;;
esac
echo "Installed $UUID. Log out and back in to load it."

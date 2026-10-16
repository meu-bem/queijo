#!/bin/sh
# Installs and enables the Queijo helper GNOME Shell extension (GNOME 42).
# On Wayland the Shell only picks up new extensions at login, so log out and
# back in afterwards.
set -e
UUID=queijo@meu-bem.github.io
DEST="$HOME/.local/share/gnome-shell/extensions/$UUID"
rm -rf "$DEST"
mkdir -p "$DEST"
cp "$(dirname "$0")/$UUID/"* "$DEST/"
CURRENT=$(gsettings get org.gnome.shell enabled-extensions)
case "$CURRENT" in
    *"'$UUID'"*) ;;
    "@as []") gsettings set org.gnome.shell enabled-extensions "['$UUID']" ;;
    *) gsettings set org.gnome.shell enabled-extensions "${CURRENT%]}, '$UUID']" ;;
esac
echo "Installed $UUID. Log out and back in to load it."

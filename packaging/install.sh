#!/bin/sh
# Installs Queijo from the unpacked release archive.
#
#   ./install.sh                         for the current user (~/.local)
#   sudo PREFIX=/usr/local ./install.sh  for everyone
#   ./install.sh --uninstall             removes what it installed
#
# Then enable the GNOME Shell extension (optional: window picker and the
# recording indicator) and log out and back in:
#   gnome-extensions enable queijo@queijo.app
set -eu

here=$(cd "$(dirname "$0")" && pwd)
prefix="${PREFIX:-$HOME/.local}"
ext=queijo@queijo.app

refresh_caches() {
    if command -v gtk-update-icon-cache >/dev/null 2>&1 && [ -d "$prefix/share/icons/hicolor" ]; then
        gtk-update-icon-cache -q -f -t "$prefix/share/icons/hicolor" || true
    fi
    if command -v update-desktop-database >/dev/null 2>&1 && [ -d "$prefix/share/applications" ]; then
        update-desktop-database -q "$prefix/share/applications" || true
    fi
}

if [ "${1:-}" = "--uninstall" ]; then
    rm -f "$prefix/bin/queijo" "$prefix/share/applications/queijo.desktop" \
        "$prefix/share/applications/queijo-settings.desktop"
    rm -rf "$prefix/share/gnome-shell/extensions/$ext"
    for dir in "$prefix"/share/icons/hicolor/*/apps; do
        rm -f "$dir/queijo.png"
    done
    refresh_caches
    echo "Queijo was removed from $prefix"
    exit 0
fi

mkdir -p "$prefix/bin" "$prefix/share"
install -m 755 "$here/bin/queijo" "$prefix/bin/queijo"
cp -r "$here/share/." "$prefix/share/"

# The launcher entries must find the program even if $prefix/bin isn't on PATH.
for entry in queijo queijo-settings; do
    sed -i "s|^Exec=queijo|Exec=$prefix/bin/queijo|" "$prefix/share/applications/$entry.desktop"
done

refresh_caches

echo "Queijo was installed in $prefix"
echo "Optional: gnome-extensions enable $ext   (then log out and back in)"

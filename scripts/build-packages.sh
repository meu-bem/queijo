#!/bin/sh
# Builds everything a release ships, into dist/:
#   queijo_<version>-1_<arch>.deb             Debian/Ubuntu package
#   queijo-<version>-x86_64-linux.tar.gz      archive with install.sh
#   queijo-gnome-extension-<version>.zip      the GNOME Shell extension alone
#   SHA256SUMS
# Needs: cargo, cargo-deb, zip.
set -eu
cd "$(dirname "$0")/.."

version=$(scripts/version.sh)
ext=queijo@queijo.app
rm -rf dist
mkdir -p dist

cargo build --release --locked
scripts/debian-changelog.sh "$version" > target/debian-changelog
cargo deb --no-build --output dist/

stage="target/archive/queijo-$version-x86_64-linux"
rm -rf target/archive
mkdir -p "$stage/bin" "$stage/share/applications" "$stage/share/gnome-shell/extensions/$ext"
install -m 755 target/release/queijo "$stage/bin/queijo"
install -m 644 packaging/*.desktop "$stage/share/applications/"
for dir in assets/icons/*x*; do
    size=$(basename "$dir")
    mkdir -p "$stage/share/icons/hicolor/$size/apps"
    install -m 644 "$dir/queijo.png" "$stage/share/icons/hicolor/$size/apps/queijo.png"
done
install -m 644 "gnome-extension/$ext/extension.js" "gnome-extension/$ext/metadata.json" \
    "$stage/share/gnome-shell/extensions/$ext/"
install -m 644 README.md LICENSE CHANGELOG.md "$stage/"
install -m 755 packaging/install.sh "$stage/install.sh"
tar -C target/archive -czf "dist/queijo-$version-x86_64-linux.tar.gz" "queijo-$version-x86_64-linux"

(cd "gnome-extension/$ext" && zip -q -X "$OLDPWD/dist/queijo-gnome-extension-$version.zip" extension.js metadata.json)

(cd dist && sha256sum -- * > SHA256SUMS)
ls -l dist

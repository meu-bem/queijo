#!/bin/sh
# Builds everything a release ships, into dist/, for each architecture:
#   queijo_<version>-1_<amd64|arm64>.deb             Debian, Ubuntu and their relatives
#   queijo-<version>-1.<x86_64|aarch64>.rpm          Fedora, openSUSE, RHEL and their relatives
#   queijo-<version>-<x86_64|aarch64>-linux.tar.gz   any distribution, with install.sh
# and once:
#   queijo-gnome-extension-<version>-gnome42.zip     the GNOME Shell extension for GNOME 42 to 44
#   queijo-gnome-extension-<version>-gnome45.zip     the one for GNOME 45 and later
#   PKGBUILD, .SRCINFO                               for the Arch User Repository (see scripts/aur.sh)
#   SHA256SUMS
#
# The binary is built against glibc 2.28 with zig as the linker, so it runs on every
# distribution at least that new (Debian 10, RHEL 8 and the clones, Ubuntu 20.04 on), not
# only the one it was built on. The graphics libraries are opened at run time and are not
# linked, so glibc is the only thing that sets how old a system can be.
#
# Needs: cargo, cargo-zigbuild and zig (`pip install ziglang`), cargo-deb, cargo-generate-rpm,
# zip, readelf, and the Rust target of each architecture (`rustup target add ...`).
# QUEIJO_ARCHES="x86_64" builds only that one (the default is both).
set -eu
cd "$(dirname "$0")/.."

version=$(scripts/version.sh)
ext=queijo@queijo.app
ext_module=queijo-gnome45@queijo.app
arches="${QUEIJO_ARCHES:-x86_64 aarch64}"
max_glibc=2.28
rm -rf dist
mkdir -p dist
mkdir -p target
scripts/debian-changelog.sh "$version" > target/debian-changelog

for arch in $arches; do
    triple="$arch-unknown-linux-gnu"
    bin="target/$triple/release/queijo"
    cargo zigbuild --release --locked --target "$triple.$max_glibc"

    # Whatever the toolchain did, the binary must not ask for a newer glibc than we promise.
    newest=$(readelf -V "$bin" | grep -o 'GLIBC_[0-9][0-9.]*' | sed 's/GLIBC_//' | sort -Vu | tail -1)
    if [ "$(printf '%s\n%s\n' "$newest" "$max_glibc" | sort -V | tail -1)" != "$max_glibc" ]; then
        echo "The $arch binary needs glibc $newest, newer than the $max_glibc this release promises" >&2
        exit 1
    fi
    echo "$arch: needs glibc $newest at most"

    case "$arch" in
        x86_64) deb_arch=amd64 ;;
        aarch64) deb_arch=arm64 ;;
    esac
    cargo deb --no-build --no-strip --target "$triple" --output "dist/queijo_${version}-1_${deb_arch}.deb"
    cargo generate-rpm --target "$triple" --payload-compress gzip --output "dist/queijo-${version}-1.${arch}.rpm"

    stage="target/archive/queijo-$version-$arch-linux"
    rm -rf "$stage"
    mkdir -p "$stage/bin" "$stage/share/applications" "$stage/share/gnome-shell/extensions/$ext" \
        "$stage/share/gnome-shell/extensions/$ext_module"
    install -m 755 "$bin" "$stage/bin/queijo"
    install -m 644 packaging/*.desktop "$stage/share/applications/"
    for dir in assets/icons/*x*; do
        size=$(basename "$dir")
        mkdir -p "$stage/share/icons/hicolor/$size/apps"
        install -m 644 "$dir/queijo.png" "$stage/share/icons/hicolor/$size/apps/queijo.png"
    done
    install -m 644 "gnome-extension/$ext/extension.js" "gnome-extension/$ext/metadata.json" \
        "$stage/share/gnome-shell/extensions/$ext/"
    install -m 644 "gnome-extension/esm/$ext_module/extension.js" "gnome-extension/esm/$ext_module/metadata.json" \
        "$stage/share/gnome-shell/extensions/$ext_module/"
    install -m 644 README.md LICENSE CHANGELOG.md packaging/dependencies.md "$stage/"
    install -m 755 packaging/install.sh "$stage/install.sh"
    tar -C target/archive -czf "dist/queijo-$version-$arch-linux.tar.gz" "queijo-$version-$arch-linux"
done

(cd "gnome-extension/$ext" && zip -q -X "$OLDPWD/dist/queijo-gnome-extension-$version-gnome42.zip" extension.js metadata.json)
(cd "gnome-extension/esm/$ext_module" && zip -q -X "$OLDPWD/dist/queijo-gnome-extension-$version-gnome45.zip" extension.js metadata.json)

scripts/aur.sh "$version"

(cd dist && sha256sum -- * > SHA256SUMS)
ls -l dist

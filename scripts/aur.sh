#!/bin/sh
# Writes dist/PKGBUILD (from packaging/aur/PKGBUILD.in, with the version and the checksums of
# the archives in dist/) and dist/.SRCINFO, for the Arch User Repository's `queijo-bin`.
# The .SRCINFO is made by makepkg itself, in an Arch container, so it needs docker; without it
# the PKGBUILD is still written and the .SRCINFO is left for whoever publishes it.
#   scripts/aur.sh VERSION
set -eu
cd "$(dirname "$0")/.."
version="${1:?usage: aur.sh VERSION}"

sum() {
    file="dist/queijo-$version-$1-linux.tar.gz"
    if [ -f "$file" ]; then sha256sum "$file" | cut -d' ' -f1; else echo SKIP; fi
}
sed -e "s/@VERSION@/$version/" \
    -e "s/@SHA256_X86_64@/$(sum x86_64)/" \
    -e "s/@SHA256_AARCH64@/$(sum aarch64)/" \
    packaging/aur/PKGBUILD.in > dist/PKGBUILD

if command -v docker >/dev/null 2>&1; then
    docker run --rm -v "$PWD/dist/PKGBUILD:/in/PKGBUILD:ro" archlinux:latest sh -c '
        useradd -m builder && cp /in/PKGBUILD /home/builder/ && chown builder /home/builder/PKGBUILD &&
        su builder -c "cd /home/builder && makepkg --printsrcinfo"' > dist/.SRCINFO
else
    echo "docker is not available: dist/.SRCINFO was not made (makepkg --printsrcinfo makes it)" >&2
fi

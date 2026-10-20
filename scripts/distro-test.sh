#!/bin/sh
# Installs Queijo's package for one distribution in a container, the way a user would, and
# checks that it runs: the files are there, no library is missing, `--doctor` answers.
# Needs docker and the packages in dist/ (scripts/build-packages.sh).
#   scripts/distro-test.sh debian:12
#   scripts/distro-test.sh archlinux:latest
set -eu
cd "$(dirname "$0")/.."
image="${1:?usage: distro-test.sh IMAGE}"
version=$(scripts/version.sh)

case "$image" in
    debian:* | ubuntu:*)
        install='apt-get update -qq >/dev/null && DEBIAN_FRONTEND=noninteractive apt-get install -y -qq /dist/queijo_*_amd64.deb >/dev/null' ;;
    fedora:* | rockylinux:* | almalinux:*)
        install='dnf install -y -q /dist/queijo-*.x86_64.rpm' ;;
    opensuse/*)
        install='zypper --non-interactive --quiet --no-gpg-checks install --allow-unsigned-rpm /dist/queijo-*.x86_64.rpm' ;;
    archlinux:*)
        # The AUR's package, built by makepkg from the PKGBUILD, with the archive where it
        # would have been downloaded to.
        install="pacman -Syu --noconfirm --needed sudo base-devel >/dev/null &&
            useradd -m builder && echo 'builder ALL=(ALL) NOPASSWD: ALL' > /etc/sudoers.d/builder &&
            mkdir /home/builder/pkg && cp /dist/PKGBUILD /home/builder/pkg/ &&
            cp /dist/queijo-$version-x86_64-linux.tar.gz /home/builder/pkg/queijo-bin-$version-x86_64.tar.gz &&
            chown -R builder /home/builder/pkg &&
            su builder -c 'cd /home/builder/pkg && makepkg -si --noconfirm >/dev/null 2>&1 || { makepkg -si --noconfirm; exit 1; }'" ;;
    *) echo "no way to install on $image" >&2; exit 1 ;;
esac

docker run --rm -v "$PWD/dist:/dist:ro" -v "$PWD/packaging/check-install.sh:/check.sh:ro" "$image" \
    sh -c "set -e; $install; sh /check.sh"

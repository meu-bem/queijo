#!/bin/sh
# Writes the Debian-format changelog for one version, from its CHANGELOG.md
# section, so there is a single history to maintain.
#   scripts/debian-changelog.sh 0.1.0 > target/debian-changelog
set -eu
here=$(dirname "$0")
version="${1:?usage: debian-changelog.sh VERSION}"
maintainer=$(sed -n 's/^authors = \["\(.*\)"\]/\1/p' "$here/../Cargo.toml" | head -n 1)
date=$(sed -n "s/^## \[$version\] - \(.*\)$/\1/p" "$here/../CHANGELOG.md" | head -n 1)
stamp=$(date -u -R -d "${date:-now}")

printf 'queijo (%s-1) unstable; urgency=medium\n\n' "$version"
"$here/release-notes.sh" "$version" | sed -e '/^[[:space:]]*$/d' -e 's/^### \(.*\)/  [ \1 ]/' \
    -e 's/^- /  * /' -e 's/^  \([^ *\[]\)/    \1/' -e 's/^\([^ ]\)/  \1/'
printf '\n -- %s  %s\n' "$maintainer" "$stamp"

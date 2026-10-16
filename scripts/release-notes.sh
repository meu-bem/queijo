#!/bin/sh
# Prints the CHANGELOG section for one version: the text of its release notes.
# Fails when there is none, so a release can't go out undocumented.
set -eu
version="${1:?usage: release-notes.sh VERSION}"
file="$(dirname "$0")/../CHANGELOG.md"
notes=$(awk -v v="$version" '
    index($0, "## [" v "]") == 1 { found = 1; next }
    found && index($0, "## [") == 1 { exit }
    found { print }
' "$file")
if [ -z "$(printf '%s' "$notes" | tr -d '[:space:]')" ]; then
    echo "CHANGELOG.md has no entry for version $version" >&2
    exit 1
fi
printf '%s\n' "$notes"

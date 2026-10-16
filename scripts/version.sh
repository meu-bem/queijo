#!/bin/sh
# Prints the version from Cargo.toml, the single source of truth.
set -eu
sed -n 's/^version = "\(.*\)"$/\1/p' "$(dirname "$0")/../Cargo.toml" | head -n 1

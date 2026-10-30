#!/bin/sh
# Sets the workspace package version (every crate uses `version.workspace = true`) and refreshes
# Cargo.lock. Usage: scripts/set-version.sh 1.2.3   (a leading "v", as in a tag, is dropped)
set -eu

cd "$(dirname "$0")/.."
version="${1:-}"
version="${version#v}"

if ! printf '%s\n' "$version" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?$'; then
    echo "set-version: '$version' is not a semantic version like 1.2.3 or 1.2.3-rc.1" >&2
    exit 2
fi

# only the `version` line of the [workspace.package] table
sed -i "/^\[workspace\.package\]/,/^\[/ s/^version = \".*\"/version = \"$version\"/" Cargo.toml

if ! grep -A3 '^\[workspace\.package\]' Cargo.toml | grep -qx "version = \"$version\""; then
    echo "set-version: could not set the version in Cargo.toml" >&2
    exit 1
fi

cargo update --workspace --offline >/dev/null 2>&1 || cargo update --workspace
echo "version set to $version"

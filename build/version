#!/bin/sh

if ! version="$(git describe --match='v*' --candidates=1 2>/dev/null)"; then
  echo "fatal: no version tag found by 'git describe'" >&2 ; exit 1
fi
# Remove `v` prefix from version.
version=${version#v}

echo $version

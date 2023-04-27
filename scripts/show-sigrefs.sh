#!/bin/bash
set -e

if [ "$#" -lt 2 ]; then
  printf "usage: %s <rid> <nid>\n" "$(basename "$0")"
  exit 1
fi

RAD_HOME=${RAD_HOME:-"$HOME/.radicle"}
REPO=$(echo "$1" | sed 's/^rad://')
REMOTE=$2

cd $RAD_HOME/storage/$REPO

sigrefs=$(git rev-parse "refs/namespaces/$REMOTE/refs/rad/sigrefs")
signed=$(git show "$sigrefs:refs")
actual=$(git for-each-ref "refs/namespaces/$REMOTE/refs/**" --format="%(objectname) %(refname)")

# Strip namespace prefix.
actual=$(echo "$actual" | sed "s@refs/namespaces/$REMOTE/@@")
# Remove `sigrefs` itself.
actual=$(echo "$actual" | grep -v "refs/rad/sigrefs$")

diff <(echo "$signed") <(echo "$actual") --color=always -y --minimal -W 240

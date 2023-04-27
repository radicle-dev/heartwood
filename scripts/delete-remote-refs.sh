#!/bin/sh

set -e

if [ "$#" -lt 2 ]; then
  printf "usage: %s <rid> <nid>\n" "$(basename "$0")"
  exit 1
fi

RAD_HOME=${RAD_HOME:-"$HOME/.radicle"}
REPO=$(echo "$1" | sed 's/rad://')
REMOTE=$2

cd $RAD_HOME/storage/$REPO

refs=$(git for-each-ref --format="%(refname)")
pattern="refs/namespaces/$2/refs/*"

for ref in $refs; do
  case "$ref" in
    $pattern)
      git update-ref -d "$ref"
      printf 'Deleted %s\n' "$ref"
      ;;
  esac
done

git gc

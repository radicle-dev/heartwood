#!/bin/sh
set -e

if [ "$#" -lt 1 ]; then
  printf "usage: %s <patch-id>\n" "$(basename "$0")"
  exit 1
fi

rad patch checkout $1
git rebase master --autosquash
rad patch update $1 --message "Rebased on master"
git checkout master

#!/bin/sh
set -e

DB="$(rad path)/node/node.db"

if command -v sqlite3 >/dev/null 2>&1; then
  if [ -f "$DB" ]; then
    printf "Clearing 'refs' table from %s.. " "$DB"
    sqlite3 "$DB" "DELETE FROM refs;"
    echo "done."
  else
    echo "fatal: database file does not exist"
    exit 1
  fi
else
  echo "fatal: sqlite3 is not installed"
  exit 1
fi

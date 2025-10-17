#! /usr/bin/env bash
set -euo pipefail

FILE=$1

while ! typos "$FILE"; do \
    exec < /dev/tty; \
    echo ""; \
    printf "%sTypos found.%s (e)dit, (c)ontinue, or (a)bort? [E/c/a] " "${WARN}" "${NORMAL}"; \
    read -r response; \
    case "$response" in \
        [cC]*) exit 0 ;; \
        [aA]*) exit 1 ;; \
        *) ${EDITOR:-nano} "$FILE" ;; \
    esac; \
done

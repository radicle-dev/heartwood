#! /usr/bin/env bash
set -e

HOOK_SCRIPT=$1
HOOKS=$2

read -r -p "Overwrite existing hooks '${HOOKS}'? [y/N] " confirm
[[ "$confirm" == "y" ]] || exit 1

for hook in $HOOKS; do
    if [ -f ".git/hooks/$hook" ]; then
      rm ".git/hooks/$hook"
    fi
    cp "$HOOK_SCRIPT" ".git/hooks/$hook"
    chmod +x ".git/hooks/$hook"
done
echo ""
echo "${SUCCESS}Hooks installed: ${HOOKS}${NORMAL}"

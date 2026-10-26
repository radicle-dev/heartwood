#! /usr/bin/env sh
set -e

HOOK_SCRIPT=$1
HOOKS=$2

TEMPLATE="$HOOK_SCRIPT"
OUTDATED=""
MISSING=0
TOTAL=0

for hook in $HOOKS; do
    TOTAL=$((TOTAL + 1))
    if [ ! -f ".git/hooks/$hook" ]; then
        MISSING=$((MISSING + 1))
        OUTDATED="$OUTDATED $hook"
    elif ! cmp -s "$TEMPLATE" ".git/hooks/$hook"; then
        OUTDATED="$OUTDATED $hook"
    fi
done

if [ "$MISSING" -eq "$TOTAL" ] && [ "$TOTAL" -gt 0 ]; then
    echo ""
    echo "${HINT}No git hooks are installed. Run 'just install-hooks' to install them.${NORMAL}"
    echo ""
elif [ -n "$OUTDATED" ]; then
    echo ""
    echo "${WARN}WARNING: The following git hooks are missing or out of date:${NORMAL}"
    echo ""
    for hook in $OUTDATED; do
        printf '\t%s\n' "$hook"
    done
    echo ""
    echo "${HINT}Check them with 'diff $HOOK_SCRIPT .git/hooks/<hook name>' then run 'just install-hooks'${NORMAL}"
    echo ""
fi

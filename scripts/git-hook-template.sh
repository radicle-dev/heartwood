#! /usr/bin/env sh
set -euo pipefail

readonly HOOK="${HOOK:-$(basename "$0")}"

case "$HOOK" in
    pre-commit|pre-push|post-checkout|commit-msg) ;;
    *)
        echo "Unknown hook '${HOOK}'."
        exit 1
        ;;
esac

readonly BASE_BRANCH="main"

# Check which sensitive files were modified compared to the base branch.
CHANGED_FILES=$(git diff --name-only "$BASE_BRANCH" -- \
    justfile build.rs rust-toolchain.toml)

if [ -n "$CHANGED_FILES" ]
then
    echo "⚠️ WARNING: Sensitive files have been modified relative to $BASE_BRANCH."
    echo "Executing this hook may run arbitrary code from the modified files."
    echo ""

    git --no-pager diff "$BASE_BRANCH" -- justfile build.rs rust-toolchain.toml

    # Read from /dev/tty because stdin is not attached to the terminal in Git hooks.
    exec < /dev/tty

    printf "⚠️ Do you want to continue executing the '%s' hook? [y/N] " "$HOOK"
    read -r response
    case "$response" in
        [yY][eE][sS]|[yY])
            echo "Continuing with '${HOOK}' hook…"
            ;;
        *)
            echo "Skipping '${HOOK}' hook."
            exit 0
            ;;
    esac
fi

# Execute the appropriate just recipe based on the hook name.
if [ "$HOOK" = "commit-msg" ]
then
    just "$HOOK" "$1"
else
    just "$HOOK"
fi

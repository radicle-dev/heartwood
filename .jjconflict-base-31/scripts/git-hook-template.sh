#! /usr/bin/env bash
set -euo pipefail

readonly HOOK="${HOOK:-$(basename "$0")}"

if ! [[ "$HOOK" =~ ^(pre-(commit|push)|post-checkout|commit-msg)$ ]]
then
    echo "Unknown hook '${HOOK}'."
    exit 1
fi

readonly SENSITIVE_FILES=("justfile" "build.rs" "rust-toolchain.toml")
readonly BASE_BRANCH="master"

# Check which files were modified compared to the base branch.
mapfile -t CHANGED_FILES < <(comm -12 \
    <(git diff --name-only "${BASE_BRANCH}" | sort) \
    <(IFS=$'\n'; echo "${SENSITIVE_FILES[*]}" | sort) \
)

if [ ${#CHANGED_FILES[@]} -gt 0 ]
then
    echo "⚠️ WARNING: Sensitive files have been modified relative to $BASE_BRANCH."
    echo "Executing this hook may run arbitrary code from the modified files."
    echo ""

    git --no-pager diff "$BASE_BRANCH" -- "${SENSITIVE_FILES[@]}"

    # Read from /dev/tty because stdin is not attached to the terminal in Git hooks.
    exec < /dev/tty

    read -r -p "⚠️ Do you want to continue executing the '${HOOK}' hook? [y/N] " response
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

#! /usr/bin/env bash
set -euo pipefail

readonly HOOK_NAME="$(basename "$0")"

if ! [[ "$HOOK_NAME" =~ ^(pre-(commit|push)|post-checkout)$ ]]
then
    echo "Unknown hook '${HOOK_NAME}'."
    exit 1
fi

readonly SENSITIVE_FILES=("justfile" "build.rs" "rust-toolchain.toml")
readonly BASE_BRANCH="master"

# Check which files were modified compared to the base branch.
mapfile -t CHANGED_FILES < <(comm -12 \
    <(git diff --name-only master | sort) \
    <(IFS=$'\n'; echo "${SENSITIVE_FILES[*]}" | sort) \
)

if [ ${#CHANGED_FILES[@]} -gt 0 ]; then
    echo "⚠️ WARNING: Sensitive files have been modified relative to $BASE_BRANCH."
    echo "Executing these hooks may run arbitrary code from the modified files."
    echo ""

    git --no-pager diff "$BASE_BRANCH" -- "${SENSITIVE_FILES[@]}"

    # Read from /dev/tty because stdin is not attached to the terminal in git hooks.
    exec < /dev/tty

    read -r -p "⚠️ Do you want to continue executing the ${HOOK_NAME} hooks? [y/N] " response
    case "$response" in
        [yY][eE][sS]|[yY])
            echo "Continuing with ${HOOK_NAME} hooks..."
            ;;
        *)
            echo "Skipping ${HOOK_NAME} hooks."
            exit 0
            ;;
    esac
fi

# Execute the appropriate just recipe based on the hook name.
just "$HOOK_NAME"

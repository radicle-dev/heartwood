#!/usr/bin/env bash
set -e

HOOK_NAME=$(basename "$0")
SENSITIVE_FILES=("justfile" "build.rs" "rust-toolchain.toml")
CHANGED_FILES=()
BASE_BRANCH="master"

for file in "${SENSITIVE_FILES[@]}"; do
    # Check if the file differs between the base branch and the current working tree
    if git diff --name-only "$BASE_BRANCH" 2>/dev/null | grep -q "^${file}$"; then
        CHANGED_FILES+=("$file")
    fi
done

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

# Execute the appropriate just recipe based on the hook name
if [ "$HOOK_NAME" = "pre-commit" ]; then
    just pre-commit
elif [ "$HOOK_NAME" = "pre-push" ]; then
    just pre-push
elif [ "$HOOK_NAME" = "post-checkout" ]; then
    just post-checkout
else
    echo "⚠️ Unknown hook: $HOOK_NAME"
    exit 1
fi

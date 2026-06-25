#!/usr/bin/env bash
set -e

# Check for forbidden keywords in the given .rs files.
# Receives file arguments from run-scoped.sh.

if [ $# -eq 0 ]; then
    exit 0
fi

# Exclude build.rs files — they contain build-time defaults
# for domain-specific values that are injected via env vars.
ARGS=()
for arg in "$@"; do
    case "$arg" in
        */build.rs|build.rs) ;;
        *) ARGS+=("$arg") ;;
    esac
done
set -- "${ARGS[@]}"

if [ $# -eq 0 ]; then
    exit 0
fi

if rg --context=3 --fixed-strings 'radicle.dev' "$@"; then
    exit 1
fi

if rg --context=3 --fixed-strings 'radicle.xyz' "$@"; then
    exit 1
fi

if rg --context=3 --fixed-strings 'radicle.zulipchat.com' "$@"; then
    exit 1
fi

# For `git2::` we need to exclude raw.rs
FILTERED_GIT2=$(printf '%s\n' "$@" | grep '^crates/radicle/.*\.rs$' | grep -v 'crates/radicle/src/git/raw.rs' || true)
if [ -n "$FILTERED_GIT2" ]; then
    if echo "$FILTERED_GIT2" | xargs rg --context=3 --fixed-strings 'git2::'; then
        exit 1
    fi
fi

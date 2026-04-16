#! /usr/bin/env bash
set -e
echo "${CHECK}Checking for forbidden words in staged files...${NORMAL}"

# Get staged Rust files
STAGED_FILES=$(git diff --cached --name-only --diff-filter=ACMR | grep '\.rs$' || true)

if [ -n "$STAGED_FILES" ]; then
    if echo "$STAGED_FILES" | xargs rg --context=3 --fixed-strings 'radicle.xyz'; then
        exit 1
    fi
    
    if echo "$STAGED_FILES" | xargs rg --context=3 --fixed-strings 'radicle.zulipchat.com'; then
        exit 1
    fi

    # For `git2::` we need to exclude raw.rs
    FILTERED_GIT2=$(echo "$STAGED_FILES" | grep '^crates/radicle/.*\.rs$' | grep -v 'crates/radicle/src/git/raw.rs' || true)
    if [ -n "$FILTERED_GIT2" ]; then
        if echo "$FILTERED_GIT2" | xargs rg --context=3 --fixed-strings 'git2::'; then
            exit 1
        fi
    fi
fi

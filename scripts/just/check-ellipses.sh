#! /usr/bin/env bash
set -euo pipefail

# Replace '...' with '…' (U+2026) in markdown and Rust files, but ignore:
# - Git ranges with hex hashes, e.g. 20aa5dd...f2de534
# - Git ranges with ref paths, e.g. feature/1...rad/patches/f2de534
# - Standalone '...' lines used as output wildcards in CLI example tests
# - Wildcards for CLI example tests, i.e. [...]
patterns=('*.md' '*.rs')

# Checksum files before sed so we detect only changes it makes.
before=$(git ls-files -z "${patterns[@]}" | xargs -0 md5sum)

standalone_wildcard='^\s*\.\.\.\s*$'
git_hex_range='[0-9a-f]{4,}\.\.\.[0-9a-f]{4,}'
git_ref_range='\S*\/\S*\.\.\.\S*\/\S*'
bracket_wildcard='\[\s*\.\.\.\s*\]'

git ls-files -z "${patterns[@]}" | xargs -0 sed --follow-symlinks --in-place --regexp-extended \
    --expression "/${standalone_wildcard}/! { /${git_hex_range}/! { /${git_ref_range}/! { /${bracket_wildcard}/! s/\.\.\./…/g } } }"

after=$(git ls-files -z "${patterns[@]}" | xargs -0 md5sum)

changed=$(diff <(echo "$before") <(echo "$after") | grep '^>' | sed 's/^> [a-f0-9]*  //' || true)

if [ -n "$changed" ]; then
    echo "error: Replaced '...' with '…' (U+2026) in the following files:"
    echo "$changed"
    echo "Please commit these changes."
    exit 1
fi

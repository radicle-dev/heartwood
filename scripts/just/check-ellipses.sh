#! /usr/bin/env sh
set -euo pipefail

# Replace '...' with '…' (U+2026) in markdown, just and Rust files, but ignore:
# - Git ranges with hex hashes, e.g. 20aa5dd...f2de534
# - Git ranges with ref paths, e.g. feature/1...rad/patches/f2de534
# - Standalone '...' lines used as output wildcards in CLI example tests
# - Wildcards for CLI example tests, i.e. [...]
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' 0
trap 'exit 1' HUP INT TERM

# Keep NUL-delimited filenames so spaces and newlines are preserved.
git ls-files -z '*.md' '*.rs' '*justfile' > "$tmp/tracked"
# The child shell expands these variables.
# shellcheck disable=SC2016
xargs -0 sh -c '
    for file do
        if [ -f "$file" ] && [ ! -L "$file" ]; then
            printf "%s\000" "$file"
        fi
    done
' sh < "$tmp/tracked" > "$tmp/files"

if [ ! -s "$tmp/files" ]; then
    exit 0
fi

# Checksum files before sed so we detect only changes it makes.
xargs -0 md5sum < "$tmp/files" > "$tmp/before"

standalone_wildcard='^[[:space:]]*\.\.\.[[:space:]]*$'
git_hex_range='[0-9a-f]{4,}\.\.\.[0-9a-f]{4,}'
git_ref_range='[^[:space:]]*\/[^[:space:]]*\.\.\.[^[:space:]]*\/[^[:space:]]*'
bracket_wildcard='\[[[:space:]]*\.\.\.[[:space:]]*\]'

# Detect GNU vs BSD sed to apply the correct in-place flag
if sed --version >/dev/null 2>&1; then
    set -- -i -E
else
    set -- -i '' -E
fi

xargs -0 sed "$@" \
    -e "/${standalone_wildcard}/b" \
    -e "/${git_hex_range}/b" \
    -e "/${git_ref_range}/b" \
    -e "/${bracket_wildcard}/b" < "$tmp/files"

xargs -0 md5sum < "$tmp/files" > "$tmp/after"

changed=$(diff "$tmp/before" "$tmp/after" | grep '^>' | sed 's/^> [a-f0-9]*  //' || true)

if [ -n "$changed" ]; then
    echo "error: Replaced '...' with '…' (U+2026) in the following files:"
    echo "$changed"
    echo "Please commit these changes."
    exit 1
fi

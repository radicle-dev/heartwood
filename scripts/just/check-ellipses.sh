#!/usr/bin/env bash
set -euo pipefail

# Replace '...' with '…' (U+2026) in markdown, just and Rust files, but ignore:
# - Git ranges with hex hashes, e.g. 20aa5dd...f2de534
# - Git ranges with ref paths, e.g. feature/1...rad/patches/f2de534
# - Standalone '...' lines used as output wildcards in CLI example tests
# - Wildcards for CLI example tests, i.e. [...]
#
# Receives file arguments from run-scoped.sh.

if [ $# -eq 0 ]; then
    exit 0
fi

# Filter to regular files (exclude symlinks, directories, etc.)
files=()
for file in "$@"; do
    if [ -f "$file" ] && [ ! -L "$file" ]; then
        files+=("$file")
    fi
done

if [ ${#files[@]} -eq 0 ]; then
    exit 0
fi

# Checksum files before sed so we detect only changes it makes.
before=$(printf '%s\0' "${files[@]}" | xargs -0 md5sum)

standalone_wildcard='^[[:space:]]*\.\.\.[[:space:]]*$'
git_hex_range='[0-9a-f]{4,}\.\.\.[0-9a-f]{4,}'
git_ref_range='[^[:space:]]*\/[^[:space:]]*\.\.\.[^[:space:]]*\/[^[:space:]]*'
bracket_wildcard='\[[[:space:]]*\.\.\.[[:space:]]*\]'

# Detect GNU vs BSD sed to apply the correct in-place flag
if sed --version >/dev/null 2>&1; then
    sed_cmd=(sed -i -E)
else
    sed_cmd=(sed -i '' -E)
fi

printf '%s\0' "${files[@]}" | xargs -0 "${sed_cmd[@]}" \
    -e "/${standalone_wildcard}/b" \
    -e "/${git_hex_range}/b" \
    -e "/${git_ref_range}/b" \
    -e "/${bracket_wildcard}/b" \
    -e 's/\.\.\./…/g'

after=$(printf '%s\0' "${files[@]}" | xargs -0 md5sum)

changed=$(diff <(echo "$before") <(echo "$after") | grep '^>' | sed 's/^> [a-f0-9]*  //' || true)

if [ -n "$changed" ]; then
    echo "error: Replaced '...' with '…' (U+2026) in the following files:"
    echo "$changed"
    echo "Please commit these changes."
    exit 1
fi

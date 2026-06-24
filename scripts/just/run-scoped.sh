#!/usr/bin/env bash
set -euo pipefail

# run-scoped.sh — Run a command on scoped or all files.
#
# Usage: run-scoped.sh [--negate] [--name NAME] <file-regex> -- <command> [args...]
#
# Reads the CHECK_FILES environment variable (newline-separated file list):
#
#   unset     → all-files mode: find tracked files matching regex
#   empty     → skip: print message and exit 0
#   non-empty → scoped: filter by regex, run on matched files
#
# Options:
#   --negate       Invert the command's exit code (for checks that fail on match,
#                  e.g. rg). Does NOT invert skip (skip always exits 0).
#   --name NAME    Display name for output (default: first word of command).
#
# Debug:
#   CHECK_VERBOSE=1  Print the resolved file list and exact command before running.

negate=false
name=""

# Parse optional flags.
while [[ "${1:-}" == --* ]]; do
    case "$1" in
        --negate) negate=true; shift ;;
        --name)  name="$2"; shift 2 ;;
        --)      break ;;
        *)       echo "run-scoped.sh: unknown flag '$1'" >&2; exit 1 ;;
    esac
done

pattern="$1"; shift

# Skip the -- separator.
if [ "${1:-}" = "--" ]; then
    shift
fi

# Derive display name from command if not given.
if [ -z "$name" ]; then
    name="$1"
fi

if [ -z "${CHECK_FILES+x}" ]; then
    matched=$(git ls-files | grep -E "$pattern" || true)
    if [ -z "$matched" ]; then
        echo "⏭️  ${name}: no tracked files matching ${pattern}"
        exit 0
    fi
    count=$(echo "$matched" | wc -l | tr -d ' ')
    echo "🔄 ${name}: all files (${count} matching)"

    if [ "${CHECK_VERBOSE:-}" = "1" ]; then
        echo "[run-scoped] mode: all-files"
        echo "[run-scoped] pattern: '${pattern}' → ${count} files"
        echo "[run-scoped] exec: $* <files>"
    fi

    if [ "$negate" = true ]; then
        echo "$matched" | xargs "$@" && exit 1 || true
    else
        echo "$matched" | xargs "$@"
    fi

elif [ -z "$CHECK_FILES" ]; then
    echo "⏭️  ${name}: skipped (no staged files)"
    exit 0

else
    matched=$(echo "$CHECK_FILES" | grep -E "$pattern" || true)
    if [ -z "$matched" ]; then
        echo "⏭️  ${name}: skipped (no staged files matching ${pattern})"
        exit 0
    fi
    count=$(echo "$matched" | wc -l | tr -d ' ')
    echo "🔄 ${name}: ${count} staged file(s)"

    if [ "${CHECK_VERBOSE:-}" = "1" ]; then
        echo "[run-scoped] mode: scoped"
        echo "[run-scoped] pattern: '${pattern}' → matched ${count} of $(echo "$CHECK_FILES" | wc -l | tr -d ' ') staged files"
        echo "[run-scoped] files:"
        echo "$matched" | while IFS= read -r line; do echo "  $line"; done
        echo "[run-scoped] exec: $* <files>"
    fi

    if [ "$negate" = true ]; then
        echo "$matched" | xargs "$@" && exit 1 || true
    else
        echo "$matched" | xargs "$@"
    fi
fi

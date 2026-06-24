#!/usr/bin/env bash
# shellcheck disable=SC2030,SC2031
# Each test runs in a subshell for isolation; CHECK_FILES modifications
# are intentionally local.
set -euo pipefail

# Test harness for run-scoped.sh
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
RUNNER="${SCRIPT_DIR}/run-scoped.sh"
TMPDIR=$(mktemp -d)
PASS_FILE="$TMPDIR/.pass"
FAIL_FILE="$TMPDIR/.fail"
echo 0 > "$PASS_FILE"
echo 0 > "$FAIL_FILE"
trap 'rm -rf "$TMPDIR"' EXIT

# Set up a minimal git repo for all-files mode (runner uses git ls-files)
(
    cd "$TMPDIR"
    git init -q
    git config user.email "test@test.com"
    git config user.name "test"
    git config commit.gpgsign false
    touch foo.rs bar.rs baz.sh qux.nix readme.md
    git add .
    git commit -q -m "init"
)

inc_pass() { echo $(( $(cat "$PASS_FILE") + 1 )) > "$PASS_FILE"; }
inc_fail() { echo $(( $(cat "$FAIL_FILE") + 1 )) > "$FAIL_FILE"; }

assert_exit() {
    local desc="$1" expected="$2" actual="$3"
    if [ "$expected" = "$actual" ]; then
        inc_pass
    else
        echo "FAIL: $desc"
        echo "  expected exit: $expected"
        echo "  actual exit:   $actual"
        inc_fail
    fi
}

assert_output_contains() {
    local desc="$1" expected="$2" actual="$3"
    if echo "$actual" | grep -qF "$expected"; then
        inc_pass
    else
        echo "FAIL: $desc"
        echo "  expected to contain: $expected"
        echo "  actual output:       $actual"
        inc_fail
    fi
}

assert_output_not_contains() {
    local desc="$1" unexpected="$2" actual="$3"
    if ! echo "$actual" | grep -qF "$unexpected"; then
        inc_pass
    else
        echo "FAIL: $desc"
        echo "  expected NOT to contain: $unexpected"
        echo "  actual output:           $actual"
        inc_fail
    fi
}

# ─── Test 1: CHECK_FILES unset → all-files mode ───
echo "── Test 1: CHECK_FILES unset → all-files mode ──"
(
    cd "$TMPDIR"
    unset CHECK_FILES
    output=$("$RUNNER" '\.rs$' -- printf '%s\n' 2>&1)
    assert_exit "exits 0" "0" "$?"
    assert_output_contains "passes foo.rs" "foo.rs" "$output"
    assert_output_contains "passes bar.rs" "bar.rs" "$output"
    assert_output_not_contains "excludes baz.sh" "baz.sh" "$output"
    assert_output_contains "shows all-files mode" "all files" "$output"
)

# ─── Test 2: CHECK_FILES empty → skip ───
echo "── Test 2: CHECK_FILES empty → skip ──"
(
    cd "$TMPDIR"
    export CHECK_FILES=""
    output=$("$RUNNER" '\.rs$' -- printf '%s\n' 2>&1)
    assert_exit "exits 0" "0" "$?"
    assert_output_contains "shows skip" "skipped" "$output"
    assert_output_not_contains "does not run command" "foo.rs" "$output"
)

# ─── Test 3: CHECK_FILES non-empty, matches exist ───
echo "── Test 3: CHECK_FILES non-empty, matches exist ──"
(
    cd "$TMPDIR"
    export CHECK_FILES=$'foo.rs\nbaz.sh\nreadme.md'
    output=$("$RUNNER" '\.rs$' -- printf '%s\n' 2>&1)
    assert_exit "exits 0" "0" "$?"
    assert_output_contains "passes foo.rs" "foo.rs" "$output"
    assert_output_not_contains "excludes baz.sh" "baz.sh" "$output"
    assert_output_not_contains "excludes readme.md" "readme.md" "$output"
    assert_output_contains "shows staged count" "1 staged" "$output"
)

# ─── Test 4: CHECK_FILES non-empty, no matches → skip ───
echo "── Test 4: CHECK_FILES non-empty, no matches → skip ──"
(
    cd "$TMPDIR"
    export CHECK_FILES=$'baz.sh\nreadme.md'
    output=$("$RUNNER" '\.rs$' -- printf '%s\n' 2>&1)
    assert_exit "exits 0" "0" "$?"
    assert_output_contains "shows skip" "skipped" "$output"
)

# ─── Test 5: --negate inverts command exit code ───
echo "── Test 5: --negate inverts command exit code ──"
(
    cd "$TMPDIR"
    unset CHECK_FILES
    # Command exits 0 → negated to 1
    "$RUNNER" --negate '\.rs$' -- true 2>&1 && rc=$? || rc=$?
    assert_exit "true negated to 1" "1" "$rc"

    # Command exits 1 → negated to 0
    "$RUNNER" --negate '\.rs$' -- false 2>&1 && rc=$? || rc=$?
    assert_exit "false negated to 0" "0" "$rc"
)

# ─── Test 6: --negate with empty CHECK_FILES → skip (not negated) ───
echo "── Test 6: --negate with empty CHECK_FILES → skip (exit 0) ──"
(
    cd "$TMPDIR"
    export CHECK_FILES=""
    "$RUNNER" --negate '\.rs$' -- true 2>&1 && rc=$? || rc=$?
    assert_exit "skip is always 0, not negated" "0" "$rc"
)

# ─── Test 7: --name sets display name ───
echo "── Test 7: --name sets display name ──"
(
    cd "$TMPDIR"
    unset CHECK_FILES
    output=$("$RUNNER" --name "conflict markers" '\.rs$' -- printf '%s\n' 2>&1)
    assert_output_contains "uses custom name" "conflict markers" "$output"
)

# ─── Test 8: CHECK_VERBOSE=1 shows extra detail ───
echo "── Test 8: CHECK_VERBOSE=1 shows extra detail ──"
(
    cd "$TMPDIR"
    export CHECK_FILES=$'foo.rs\nbar.rs'
    export CHECK_VERBOSE=1
    output=$("$RUNNER" '\.rs$' -- printf '%s\n' 2>&1)
    assert_output_contains "shows pattern" "pattern" "$output"
    assert_output_contains "shows exec" "exec" "$output"
    unset CHECK_VERBOSE
)

# ─── Results ───
PASS=$(cat "$PASS_FILE")
FAIL=$(cat "$FAIL_FILE")
echo ""
echo "═══════════════════════════════"
echo "  PASS: $PASS  FAIL: $FAIL"
echo "═══════════════════════════════"
[ "$FAIL" -eq 0 ] || exit 1

default:
    @just --list

# Run pre-commit checks
[group('hooks')]
pre-commit: format-rust check-rust check-docs check-typos check-spelling check-scripts check-keywords format-nix
    @echo "✅ pre-commit passed"

# Format Rust code
[group('pre-commit')]
[group('pre-push')]
[group('format')]
[parallel]
format-rust: (verify-tool "cargo")
    @echo "Cargo fmt..."
    @cargo fmt --all

# Run cargo check
[group('pre-commit')]
[group('pre-push')]
[group('check')]
[parallel]
check-rust:
    @echo "Cargo check..."
    @cargo check --workspace --all-targets --all-features

# Check documentation for warnings
[group('pre-commit')]
[group('pre-push')]
[group('check')]
[parallel]
check-docs:
    @echo "Checking docs for warnings..."
    @RUSTDOCFLAGS="--deny warnings" cargo doc --workspace --all-features --no-deps

# Check for typos
[group('pre-commit')]
[group('pre-push')]
[group('check')]
[parallel]
check-typos: (verify-tool "typos" "typos-cli")
    @echo "Checking for spelling typos..."
    @typos

# Run codespell
[group('pre-commit')]
[group('pre-push')]
[group('check')]
[parallel]
check-spelling: (verify-tool "codespell")
    @echo "Checking for code typos..."
    @git ls-files -z | xargs -0 codespell -w

# Run shellcheck on all shell scripts
[group('pre-commit')]
[group('pre-push')]
[group('check')]
[parallel]
check-scripts: (verify-tool "shellcheck")
    @echo "Checking shell scripts..."
    @shellcheck scripts/*.sh

# Replicate the custom grep checks from flake.nix
[group('pre-commit')]
[group('pre-push')]
[group('check')]
[parallel]
check-keywords: (verify-tool "rg" "ripgrep")
    #! /usr/bin/env bash
    set -e
    echo "Checking for forbidden words in staged files..."

    # Get staged Rust files
    STAGED_FILES=$(git diff --cached --name-only --diff-filter=ACMR | grep '\.rs$' || true)

    if [ -n "$STAGED_FILES" ]; then
        ! echo "$STAGED_FILES" | xargs rg --context=3 --fixed-strings 'radicle.xyz'
        ! echo "$STAGED_FILES" | xargs rg --context=3 --fixed-strings 'radicle.zulipchat.com'

        # For `git2::` we need to exclude raw.rs
        FILTERED_GIT2=$(echo "$STAGED_FILES" | grep '^crates/radicle/.*\.rs$' | grep -v 'crates/radicle/src/git/raw.rs' || true)
        if [ -n "$FILTERED_GIT2" ]; then
            ! echo "$FILTERED_GIT2" | xargs rg --context=3 --fixed-strings 'git2::'
        fi
    fi

# Format Nix files
[group('pre-commit')]
[group('pre-push')]
[group('format')]
[parallel]
format-nix:
    #!/usr/bin/env bash
    if command -v alejandra >/dev/null 2>&1; then
        alejandra --check .
    else
        echo "⏭️ alejandra not found, skipping Nix formatting."
    fi

# Run pre-push checks
[group('hooks')]
pre-push: format-rust check-rust check-keywords check-docs check-spelling check-scripts check-typos format-nix lint-rust
    @echo "✅ pre-push passed"

# Run Clippy lints
[group('pre-push')]
lint-rust: (verify-tool "cargo")
    @echo "Cargo clippy..."
    @cargo clippy --workspace --all-targets --all-features -- --deny warnings

# Check if required tools are in PATH.
[private]
verify-tool tool package_name="":
    #!/usr/bin/env bash
    set -e
    if ! command -v {{tool}} >/dev/null 2>&1; then
        PKG="{{package_name}}"
        if [ -z "$PKG" ]; then
            PKG="{{tool}}"
        fi
        echo "❌ Missing required tool: {{tool}}"
        echo "💡 Use your systems package manager to install '$PKG'."
        exit 1
    fi

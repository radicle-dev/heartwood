hooks := "pre-commit pre-push post-checkout"
hook-script := "scripts/git-hook-template.sh"

bold_underlined := BOLD + UNDERLINE
WARN := "⚠️ " + YELLOW + bold_underlined
SUCCESS := "✅ " + GREEN + bold_underlined
ERROR := "❌ " + RED + bold_underlined
HINT := "💡 " + BOLD
CHECK := "🔄 " + BOLD

default: check-hooks
    @just --list

# Run post-checkout checks
[group('hooks')]
post-checkout:

# Run pre-commit checks
[group('hooks')]
pre-commit: format-rust check-rust check-docs check-typos check-spelling check-scripts check-keywords format-nix
    @echo ""
    @echo "{{SUCCESS}}pre-commit passed!{{NORMAL}}"
    @echo ""

# Format Rust code
[group('pre-commit')]
[group('pre-push')]
[group('format')]
[parallel]
format-rust: (verify-tool "cargo")
    @echo "{{CHECK}}Cargo fmt...{{NORMAL}}"
    @cargo fmt --all

# Run cargo check
[group('pre-commit')]
[group('pre-push')]
[group('check')]
[parallel]
check-rust:
    @echo "{{CHECK}}Cargo check...{{NORMAL}}"
    @cargo check --workspace --all-targets --all-features

# Check documentation for warnings
[group('pre-commit')]
[group('pre-push')]
[group('check')]
[parallel]
check-docs:
    @echo "{{CHECK}}Checking docs for warnings...{{NORMAL}}"
    @RUSTDOCFLAGS="--deny warnings" cargo doc --workspace --all-features --no-deps

# Check for typos
[group('pre-commit')]
[group('pre-push')]
[group('check')]
[parallel]
check-typos: (verify-tool "typos" "typos-cli")
    @echo "{{CHECK}}Checking for spelling typos...{{NORMAL}}"
    @typos

# Run codespell
[group('pre-commit')]
[group('pre-push')]
[group('check')]
[parallel]
check-spelling: (verify-tool "codespell")
    @echo "{{CHECK}}Checking for code typos...{{NORMAL}}"
    @git ls-files -z | xargs -0 codespell --write-changes --check-filenames

# Run shellcheck on all shell scripts
[group('pre-commit')]
[group('pre-push')]
[group('check')]
[parallel]
check-scripts: (verify-tool "shellcheck")
    @echo "{{CHECK}}Checking shell scripts...{{NORMAL}}"
    @shellcheck **/*.sh

# Run checks for forbidden keywords
[group('pre-commit')]
[group('pre-push')]
[group('check')]
[parallel]
check-keywords: (verify-tool "rg" "ripgrep")
    #! /usr/bin/env bash
    set -e
    echo "{{CHECK}}Checking for forbidden words in staged files...{{NORMAL}}"

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
    @echo ""
    @echo "{{SUCCESS}}pre-push passed!{{NORMAL}}"
    @echo ""

# Run Clippy lints
[group('pre-push')]
lint-rust: (verify-tool "cargo")
    @echo "{{CHECK}}Cargo clippy...{{NORMAL}}"
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
        echo "{{ERROR}}Missing required tool: {{tool + NORMAL}}"
        echo "{{HINT}}Use your systems package manager to install '$PKG'.{{NORMAL}}"
        exit 1
    fi

# SECURITY: We COPY the hook template instead of symlinking it. This ensures that
# checking out an untrusted patch won't overwrite your local git hooks. The copied
# script also checks if sensitive files (like build.rs or justfile) were modified
# in the patch and prompts for confirmation, preventing arbitrary code execution.
#
# Install git hooks
[group('hooks')]
[confirm("Overwrite existing hooks '" + hooks + "'?")]
install-hooks:
    #!/usr/bin/env bash
    set -e
    for hook in {{hooks}}; do
        if [ -f ".git/hooks/$hook" ]; then
          rm ".git/hooks/$hook"
        fi
        cp {{hook-script}} ".git/hooks/$hook"
        chmod +x ".git/hooks/$hook"
    done
    echo ""
    echo "{{SUCCESS}}Hooks installed: {{hooks + NORMAL}}"

# Check for missing or changed hooks
[group('hooks')]
check-hooks:
    #!/usr/bin/env bash
    set -e
    TEMPLATE="{{hook-script}}"
    OUTDATED=()
    MISSING=0
    TOTAL=0

    for hook in {{hooks}}; do
        TOTAL=$((TOTAL + 1))
        if [ ! -f ".git/hooks/$hook" ]; then
            MISSING=$((MISSING + 1))
            OUTDATED+=("$hook")
        elif ! cmp -s "$TEMPLATE" ".git/hooks/$hook"; then
            OUTDATED+=("$hook")
        fi
    done

    if [ "$MISSING" -eq "$TOTAL" ] && [ "$TOTAL" -gt 0 ]; then
        echo ""
        echo "{{HINT}}No git hooks are installed. Run 'just install-hooks' to install them.{{NORMAL}}"
        echo ""
    elif [ ${#OUTDATED[@]} -gt 0 ]; then
        echo ""
        echo "{{WARN}}WARNING: The following git hooks are missing or out of date:{{NORMAL}}"
        echo ""
        for hook in "${OUTDATED[@]}"; do
            echo -e "\t$hook"
        done
        echo ""
        echo "{{HINT}}Check them with 'diff scripts/git-hook-template.sh .git/hooks/<hook name>' then run 'just install-hooks'{{NORMAL}}"
        echo ""
    fi

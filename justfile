hooks := "pre-commit pre-push post-checkout"
hook-script := "scripts/git-hook-template.sh"

WARN := "⚠️ " + YELLOW + BOLD
SUCCESS := "✅ " + GREEN + BOLD
ERROR := "❌ " + RED + BOLD
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

# just runs with `/bin/sh` which has no doublestar glob 
# expansion, furthermore, `time ls **/*.sh` takes ~5s 
# locally. The `find` solution below is fastest ~900ms.
#
# Run shellcheck on all shell scripts
[group('pre-commit')]
[group('pre-push')]
[group('check')]
[parallel]
check-scripts: (verify-tool "shellcheck")
    @echo "{{CHECK}}Checking shell scripts...{{NORMAL}}"
    @find . -type f -name "*.sh" -exec shellcheck {} +

# Run checks for forbidden keywords
[group('pre-commit')]
[group('pre-push')]
[group('check')]
[parallel]
check-keywords: (verify-tool "rg" "ripgrep")
    @CHECK="{{CHECK}}" NORMAL="{{NORMAL}}" scripts/just/check-keywords.sh

# Format Nix files
[group('pre-commit')]
[group('pre-push')]
[group('format')]
[parallel]
format-nix:
    @scripts/just/format-nix.sh

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
    @ERROR="{{ERROR}}" NORMAL="{{NORMAL}}" HINT="{{HINT}}" scripts/just/verify-tool.sh "{{tool}}" "{{package_name}}"

# SECURITY: We COPY the hook template instead of symlinking it. This ensures that
# checking out an untrusted patch won't overwrite your local git hooks. The copied
# script also checks if sensitive files (like build.rs or justfile) were modified
# in the patch and prompts for confirmation, preventing arbitrary code execution.
#
# Install git hooks
[group('hooks')]
install-hooks:
    @SUCCESS="{{SUCCESS}}" NORMAL="{{NORMAL}}" scripts/just/install-hooks.sh "{{hook-script}}" "{{hooks}}"

# Check for missing or changed hooks
[group('hooks')]
check-hooks:
    @HINT="{{HINT}}" NORMAL="{{NORMAL}}" WARN="{{WARN}}" scripts/just/check-hooks.sh "{{hook-script}}" "{{hooks}}"

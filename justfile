default:
    @just --list

# Run pre-push checks
pre-push: lint-rust
    @echo "✅ pre-push passed"

# Run Clippy lints
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

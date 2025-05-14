# Radicle Agent Guidelines

## Build & Test Commands
- Run tests: `cargo test --workspace`
- Run a single test: `cargo test --package <package> <test_name>` or use `cargo nextest run <test_name>`
- Run linting: `cargo clippy --workspace --tests`
- Format code: `cargo fmt`
- Generate docs: `cargo doc --workspace --all-features`
- Run in debug mode: `cargo run -p <package>` (e.g. `cargo run -p radicle-cli --bin rad -- auth`)

## Code Style Guidelines
- **Imports**: Group by std, external deps, then crate imports. Public modules before imports.
- **Naming**: Short names for small scopes, descriptive for larger scopes & globals.
- **Error handling**: Only use `unwrap()` when: (1) panic impossible, (2) bug detected, (3) in test code. Document with `// SAFETY:`.
- **Logging**: Include target and context (e.g. `debug!(target: "service", "Message with {context}")`)
- **Documentation**: Document public types and functions with full English sentences.
- **Dependencies**: Minimize external dependencies, check with maintainers before adding.
- **Commits**: Write in imperative mood, capitalized, no period (e.g. "Add feature").

For more details, see CONTRIBUTING.md and HACKING.md.
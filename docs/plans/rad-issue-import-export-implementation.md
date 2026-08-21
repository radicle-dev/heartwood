# Implementation Plan: `rad issue import` and `rad issue export`

## Objective

Implement two new `rad issue` subcommands that synchronize issues between:

- internal Git-backed issue storage (`.git` object store), and
- plain Markdown files in repository tree (default `issues/` at repo root).

## Deliverables

- CLI support for `rad issue export` and `rad issue import`.
- Configurable issue directory with default `issues`.
- Canonical Markdown codec for issue round-tripping.
- Deterministic import/export behavior with useful summaries and errors.
- Test coverage at unit, integration, and command levels.

## Work packages

### 1) Command and option wiring

- Add `import` and `export` subcommands under existing `rad issue` command parser.
- Add options: `--path`, `--dry-run` on both commands, and `--force` on `import`.
- Hook commands into existing repository discovery and identity/context loading flow.

Exit criteria:

- Commands appear in help output.
- Options parse correctly and map to internal option structs.

### 2) Configuration support

- Add profile config key `cli.issues.directory` (default `issues`).
- Implement path resolution precedence: CLI flag > config > default.
- Anchor resolved path to repository root.
- Reject absolute paths in v1.

Exit criteria:

- Commands use resolved path consistently.
- Relative and absolute path inputs are validated.

### 3) Markdown codec

- Create a dedicated issue Markdown serializer/deserializer module.
- Define canonical front matter key set and output ordering.
- Validate required fields and normalize values (`state`, timestamps).
- Ensure parser tolerates unknown keys (forward compatibility).

Exit criteria:

- Codec supports lossless round-trip for fields in scope.
- Canonical output is stable across repeated serialization.

### 4) Export pipeline

- Load issues from internal store in deterministic order.
- Render each issue through codec to Markdown.
- Resolve the owner segment per issue (local alias for own issues, node-known alias otherwise, raw `did:key` fallback) and slugify it.
- Perform atomic writes to `<issues-dir>/<owner>/<YYYY-MM-DD>-<slug-of-title>.md`, creating owner subdirectories as needed.
- Support `--dry-run` reporting without filesystem writes.
- Implement conflict policy where divergent existing Markdown files are never overwritten.

Exit criteria:

- Export produces stable files and summary counters.
- Issues group into per-owner subdirectories under the configured folder.
- Re-running export without internal changes yields zero file diffs.
- Divergent existing Markdown files are reported as conflicts and left untouched.

### 5) Import pipeline

- Recursively enumerate Markdown files under the target folder, including owner subdirectories, in deterministic order.
- Parse and validate files; detect duplicate IDs across files.
- Create/update internal issues by id.
- For new markdown ids that don't exist internally, create new issues and persist external->internal id mapping in `.radicle-issue-import-map.json`.
- Treat divergence on existing internal issues as conflict by default.
- Allow `--force` to overwrite conflicting internal issues from Markdown input.
- Continue after per-file failures; aggregate errors for final exit status.
- Support `--dry-run` reporting without internal writes.

Exit criteria:

- Import updates internal store as expected.
- Invalid files are reported clearly with path-level diagnostics.
- Conflicts are reported with clear guidance to rerun with `--force`.

### 6) Output and UX

- Add concise operation summaries: imported/exported/unchanged/conflicted/failed.
- Keep message format script-friendly (single-line counters where possible).
- Document examples in command help text.

Exit criteria:

- Users can quickly understand what changed and what failed.

## Suggested internal architecture

- `issue::sync::path` module: path/config resolution.
- `issue::sync::codec` module: markdown <-> issue model mapping.
- `issue::sync::export` module: internal store -> files.
- `issue::sync::import` module: files -> internal store.

Keep command handlers thin and delegate core logic to testable library functions.

## Sequencing

1. Command wiring + config key.
2. Codec module and tests.
3. Export flow + tests.
4. Import flow + tests.
5. UX polish, docs, and end-to-end tests.

## Risks and mitigations

- Format drift causes noisy diffs.
  - Mitigation: strict canonical serializer and snapshot tests.
- Owner alias resolution differs between machines, moving files across owner directories.
  - Mitigation: import ignores directory names and trusts front matter `id`; export treats a moved file with identical content as unchanged only at its canonical path (documented behavior).
- Partial import success can confuse users.
  - Mitigation: clear per-file errors + final non-zero exit on any failure.
- Manual file edits may break parse rules.
  - Mitigation: precise validation messages with required schema hints.
- Repository path confusion from nested cwd.
  - Mitigation: always resolve from detected repository root.

## Definition of done

- Both subcommands are available and documented.
- Default and configured issue directories work.
- Export/import idempotency checks pass.
- Full TDD plan scenarios pass in CI.
- No regressions to existing `rad issue` functionality.

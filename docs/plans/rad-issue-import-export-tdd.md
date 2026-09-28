# TDD Plan: `rad issue import` / `rad issue export`

## Strategy

Use red-green-refactor in thin vertical slices, starting from pure codec logic and moving outward to command-level behavior.

Order of test layers:

1. Unit tests for Markdown codec.
2. Unit tests for path/config resolution.
3. Integration tests for export/import pipelines.
4. CLI tests for command flags, output, and exit codes.

## Test environment

- Use temp Git repositories with seeded internal issue objects.
- Use deterministic fixture timestamps and issue IDs.
- Freeze locale/timezone assumptions where needed.

## Red-Green-Refactor slices

### Slice 1: path resolution

Red tests:

- resolves default path to `<repo-root>/issues`.
- uses `cli.issues.directory` config path when set.
- `--path` overrides config.
- relative override path anchors to repo root.
- absolute path is rejected.

Green target:

- `resolve_issue_dir(repo_root, config, cli_override)` returns canonical path.

Refactor:

- centralize all path logic in one module and reuse in both commands.

### Slice 2: Markdown serializer

Red tests:

- serializes issue to canonical front matter key order.
- output is stable across repeated serialization.
- generated filename is `<YYYY-MM-DD>-<slug-of-title>.md`.
- owner segment slugifies aliases and falls back to `unknown-owner` for non-alphanumeric input.

Green target:

- `serialize_issue(issue)` emits deterministic Markdown.

Refactor:

- extract reusable helpers for timestamp/state formatting.

### Slice 3: Markdown parser

Red tests:

- parses valid front matter + body into issue model.
- rejects missing required keys (`id`, `title`, `state`).
- rejects invalid `state` value.
- ignores unknown keys without failure.

Green target:

- `parse_issue_markdown(input)` returns validated domain model.

Refactor:

- unify validation error types for user-facing diagnostics.

### Slice 4: export pipeline

Red tests:

- exports all internal issues into target folder, grouped into per-owner subdirectories (`<issues-dir>/<owner>/<file>.md`).
- creates missing owner subdirectories on demand.
- writes files in deterministic order and content.
- repeated export with unchanged data produces no diffs.
- `--dry-run` reports changes and performs no writes.
- existing divergent Markdown file is reported as conflict and is not overwritten.

Green target:

- `run_export(ctx, opts)` yields expected filesystem state and summary counts.

Refactor:

- isolate atomic writer component for easier error injection tests.

### Slice 5: import pipeline

Red tests:

- imports valid files and creates missing internal issues.
- recursively discovers files nested in owner subdirectories.
- creating a missing issue records id mapping in `.radicle-issue-import-map.json`.
- import uses persisted id mapping to avoid duplicate re-creation on repeated runs.
- conflicting existing internal issue is reported and skipped by default.
- `--force` updates conflicting existing internal issues by ID.
- duplicate IDs across files produce error.
- per-file parse error does not stop processing others.
- command exits non-zero when any file fails.
- command exits non-zero when one or more conflicts occur.
- `--dry-run` reports changes and performs no internal writes.

Green target:

- `run_import(ctx, opts)` mutates internal store correctly and reports aggregate result.

Refactor:

- extract reusable operation summary accumulator.

### Slice 6: round-trip invariants

Red tests:

- `export -> import -> export` yields byte-identical Markdown.
- `import -> export` on canonical input is idempotent.

Green target:

- codec and pipeline combination is deterministic.

Refactor:

- remove duplicated fixture setup and use shared builders.

### Slice 7: CLI behavior

Red tests:

- `rad issue export --help` and `rad issue import --help` contain expected options (including `--force` only on import).
- success path returns exit code `0` with summary output.
- partial failure returns non-zero with diagnostics.

Green target:

- CLI layer maps errors and summaries to stable output.

Refactor:

- ensure thin command handlers with reusable service calls.

## Regression matrix

Before merge, run focused matrix:

- clean repo and dirty repo working tree,
- default folder and custom folder,
- empty issue set and multi-issue set,
- valid-only files and mixed valid/invalid files.

## Completion criteria

- All red tests introduced in each slice are green.
- Round-trip invariants hold in CI.
- No flaky tests under repeated runs.
- Existing `rad issue` tests remain green.

# Specification: `rad issue import` and `rad issue export`

Status: Draft

## Problem

Issues currently live in Radicle's internal Git object store (`.git`). That keeps issue data available to `rad`, but it does not make issues visible as regular files in the repository tree. This makes issue content harder to:

- share using normal Git workflows,
- review in pull requests,
- edit with standard text tooling,
- mirror across systems that only understand repository files.

## Goals

- Add `rad issue export` to materialize internal issues as Markdown files in the repository working tree.
- Add `rad issue import` to read Markdown issue files from the repository and write them back into the internal issue store.
- Use a configurable issue folder at repository root; default to `issues/`.
- Provide deterministic and idempotent behavior so repeated import/export produces stable results.
- Keep issue files human-readable and machine-parseable.

## Non-goals

- Auto-committing exported files.
- Synchronizing attachments or binary payloads.
- Deleting internal issues when a Markdown file is removed (initial version).
- Resolving complex multi-writer conflicts automatically.

## User stories

- As a repository maintainer, I can run `rad issue export` and commit issue files so collaborators can read them without `rad`.
- As a contributor, I can edit exported issue Markdown in Git and run `rad issue import` to sync those edits back into the internal issue store.
- As an automation user, I can point import/export at a custom issue folder path.

## CLI surface

### `rad issue export`

Exports all issues from internal storage to Markdown files under the configured folder.

Usage:

```text
rad issue export [--path <DIR>] [--dry-run]
```

- `--path <DIR>`: override configured folder for this invocation.
- `--dry-run`: show planned file writes/updates without changing files.

Default behavior:

- Resolves the target path relative to repository root.
- Creates the directory when missing.
- Writes one Markdown file per issue.
- Does not stage or commit changes.
- Never overwrites divergent Markdown files.

### `rad issue import`

Imports Markdown issues from repository files into internal issue storage.

Usage:

```text
rad issue import [--path <DIR>] [--dry-run] [--force]
```

- `--path <DIR>`: override configured folder for this invocation.
- `--dry-run`: show planned internal updates without writing to object store.
- `--force`: overwrite existing internal issue state with Markdown file state when they differ.

Default behavior:

- Reads `*.md` files in the target folder (non-recursive in v1).
- Parses each file using the canonical issue Markdown format.
- Creates missing internal issues and updates existing ones only when `--force` is provided.

## Configuration

- Add a new profile config key at `cli.issues.directory`.
- JSON shape:

```json
{
  "cli": {
    "issues": {
      "directory": "issues"
    }
  }
}
```

- Default value: `issues`.
- `--path` always overrides config for the current command.

Why this key:

- Existing profile config groups user-interface behavior under `cli` (eg. `cli.hints`).
- `issues` as a nested object leaves room for future issue-file settings without adding more top-level keys.
- `directory` is explicit and aligns with current config naming that favors descriptive fields over abbreviations.

Resolution rules:

1. `--path` CLI flag, if provided.
2. Configured issue directory.
3. Default `issues`.

The final path is anchored at repository root.

Path constraints:

- Relative paths are resolved from repository root.
- Absolute paths are rejected in v1 to keep import/export repository-local.

## File format

Each issue is represented by one Markdown file, grouped in a per-owner subdirectory:

```text
<issues-dir>/<owner>/<YYYY-MM-DD>-<slug-of-title>.md
```

The `<owner>` segment is the slugified name of the issue author:

- If the author is the local profile identity, the configured node alias is used.
- Otherwise, the alias known to the local node for that key is used when available.
- If no alias is known, the raw `did:key` string is used.

Owner names are slugified with the same rules as title slugs (lowercase ASCII alphanumerics, other runs collapsed to `-`, leading/trailing hyphens trimmed). A fully non-alphanumeric owner falls back to `unknown-owner`.

Import-created ID mappings are stored in:

```text
<issues-dir>/.radicle-issue-import-map.json
```

This file maps external markdown `id` values to internal Radicle issue IDs when a markdown file creates a new issue.

File content uses YAML front matter plus Markdown body:

```markdown
---
id: <issue-id>
title: <title>
state: open|closed
author: <did-or-handle>
assignees: []
labels: []
created: <RFC3339 timestamp>
updated: <RFC3339 timestamp>
---

<issue description in markdown>
```

When the issue has discussion comments, they are appended to the body after the description, oldest first, using machine-parseable delimiters:

```markdown
<issue description in markdown>

## Comments

<!-- radicle:comment -->
<comment body>
<!-- /radicle:comment -->
```

Notes:

- `id` is required and is the canonical mapping key for round-trips.
- Filename is derived from `created` date and title slug for chronological ordering.
- The owning subdirectory is derived from the issue author, not from file content position; import ignores directory names and trusts front matter.
- Unknown front matter keys are ignored on import in v1 (forward-compatible).
- Output serialization order of front matter keys is fixed for stable diffs.
- Comments are exported in ascending comment-timestamp order (stable with respect to the internal timeline for equal timestamps). The root comment (the description) is not repeated in the comments section.
- Comment blocks carry only the body. Import appends file comments to the internal thread as top-level comments, preserving body and order; per-comment authorship and timestamps are regenerated by the importing node (the COB store stamps new operations with the local identity and clock), so they are intentionally not part of the file format.

## Export semantics

- Enumerate all internal issues in deterministic order (ascending by issue id).
- Render each issue to canonical Markdown.
- Write files atomically (temp file + rename) to `<issues-dir>/<owner>/<file>.md`, creating owner directories as needed.
- If target file does not exist: create it.
- If target file exists and is byte-identical to canonical export output: mark unchanged.
- If target file exists and differs from canonical export output: mark conflict and skip (Markdown file takes precedence).
- If target file exists but is invalid Markdown/front matter or has mismatched `id`, mark conflict and skip.
- Print summary: exported, unchanged, conflicted, failed.

## Import semantics

- Recursively collect `*.md` files under the target folder, including owner subdirectories, in deterministic order (lexicographic full path).
- Parse front matter and body.
- Validate required fields (`id`, `title`, `state`).
- For each parsed issue:
  - if issue id does not exist internally: create a new issue object and record external-id -> internal-id mapping,
  - if issue id exists and canonical internal representation is identical: mark unchanged,
  - if issue id exists and differs: mark conflict unless `--force` is set.
- With `--force`, conflicting existing internal issues are overwritten by file content. Missing file comments are appended to the internal thread; existing internal comments are never deleted.
- Preserve unchanged internal values when a field is missing and optional.
- Print summary: imported, unchanged, conflicted, failed.

## Identity and replication

- The front-matter `id` of an exported file is the internal Radicle issue id (COB object id).
- Issue identity across machines comes from Radicle replication (`rad clone`, `rad sync`), which carries COB data alongside repository files. Markdown files alone do not confer identity.
- If an import file references a Radicle issue id that is absent locally (eg. a plain Git clone that lacks COB refs), import reports a conflict instead of silently creating a divergent object. Replicate the issue data first, or pass `--force` to deliberately create a distinct local issue; the id map records the substitution.
- Files with non-Radicle ids (eg. imported from another tracker) always create fresh local issues and are tracked via `.radicle-issue-import-map.json`. Commit this file so mappings propagate to collaborators.
- Consequently, `rad issue show <ID>` refers to the same collaboratively replicated issue on two machines only when both obtained the COB data through replication. Importing the same Markdown file on two machines without replication yields distinct objects sharing a common external id.

## Conflict policy

- Export is conservative: repository Markdown wins on divergence.
- Import is conservative by default: existing internal issues are not overwritten on divergence.
- Import with `--force` is authoritative: file state overwrites internal issue state.
- A conflict in either command does not abort processing of other issues/files.
- Comment divergence (missing or differing comment bodies) counts as divergence for both commands.

## Validation and errors

- Fail fast when repository root cannot be resolved.
- Report per-file parse/validation errors with file path and line context when available.
- Continue processing other files after a per-file error; return non-zero exit code if any error occurred.
- Reject duplicate `id` values across input files during import.
- Reject files not ending in `.md`.
- Ignore `.radicle-issue-import-map.json` during markdown file scanning.
- Import does not require files to live in an owner subdirectory; any nested location is accepted.
- Return non-zero exit code if one or more conflicts occurred.

## Idempotency guarantees

- Running `rad issue export` twice without internal changes yields no file diffs.
- Running `rad issue import` twice without file changes yields no internal changes.
- `export -> import -> export` preserves canonical file content, including comment bodies and ordering.

## Compatibility

- Existing `rad issue` subcommands remain unchanged.
- New subcommands are additive and backward-compatible.

## Acceptance criteria

- `rad issue export` creates `<issues-dir>/<owner>/*.md` by default from internal issues, one subdirectory per distinct owner.
- `rad issue import` recursively consumes those files (including owner subdirectories) and reconstructs corresponding internal issues.
- Discussion comments are exported oldest-first and imported back onto the internal thread without duplication on repeated imports.
- Custom directory works via config and `--path` override.
- Config key `cli.issues.directory` is honored and documented.
- Export never overwrites divergent Markdown files.
- Import overwrites divergent internal issues only with `--force`.
- Round-trip behavior is deterministic and idempotent.
- Command outputs provide actionable summaries and errors.

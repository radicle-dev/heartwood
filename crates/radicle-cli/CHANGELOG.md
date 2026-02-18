# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

### Changed

### Removed

### Security

## 0.18.0

### Changed

- `radicle_cli::terminal::fail` now takes 1 parameter, the `anyhow::Error`
  instead of 2.
- `radicle_cli::commands::diff::run` now delegates to the `git diff` process,
  and only accepts the `Vec<OsString>` args.

### Removed

The `radicle-cli` crate is refactored to use the `clap` crate, and as a result
many things were removed from the public API.

- The `radicle_cli::terminal::args` module was removed, including the `Args`
  trait, the `Help` struct, the `Error` enum, and all parsing functions
  (`parse_value`, `finish`, `format`, `refstring`, `did`, `nid`, `rid`,
  `pubkey`, `addr`, `socket_addr`, `number`, `seconds`, `milliseconds`,
  `string`, `rev`, `oid`, `alias`, `issue`, `patch`, `cob`).
- The `radicle_cli::terminal::Command` trait was removed.
- `radicle_cli::terminal::run_command`, `radicle_cli::terminal::run_command_args`,
  and `radicle_cli::terminal::run_command_fn` were removed.
- The `radicle_cli::commands::help` module was removed, including
  `help::Options` and `help::run`.
- `radicle_cli::git::parse_remote` was removed.
- The `HELP` constant was removed from the following command modules: `auth`,
  `block`, `checkout`, `clone`, `cob`, `config`, `debug`, `diff`, `follow`,
  `fork`, `help`, `id`, `inbox`, `init`, `inspect`, `ls`, `node`, `patch`,
  `publish`, `remote`, `seed`, `self`, `sync`, `unblock`, `watch`.
- The `Options` struct was removed from the following command modules: `auth`,
  `block`, `checkout`, `clone`, `cob`, `config`, `debug`, `diff`, `follow`,
  `fork`, `help`, `id`, `inbox`, `init`, `inspect`, `ls`, `node`, `patch`,
  `publish`, `remote`, `seed`, `rad_self`, `sync`, `unblock`, `watch`.
- `radicle_cli::commands::patch::AssignOptions`,
  `radicle_cli::commands::patch::LabelOptions`, and
  `radicle_cli::commands::patch::CommentOperation` were removed.
- The following enums were removed from their respective command modules:
  `follow::Operation`, `follow::OperationName`, `id::Operation`,
  `id::OperationName`, `inspect::Target`, `node::Addr`, `node::Operation`,
  `node::OperationName`, `patch::Operation`, `patch::OperationName`,
  `remote::Operation`, `remote::OperationName`, `remote::ListOption`,
  `seed::Operation`, `sync::Operation`, `sync::SyncDirection`,
  `sync::SyncMode`, `sync::SortBy`.

### Security

*No security updates.*

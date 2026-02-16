# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `radicle::node::Handle` added a `block` method to allow setting the follow
  policy to `Policy::Block`.
- `radicle::node::Handle::announce_refs_for` now allows specifying for which
  namespaces changes should be announced. A corresponding enum variant
  `radicle::node::Command::AnnounceRefsFor` is added.
- `radicle::node::Handle::seeds_for` now allows specifying for which
  namespaces sync status should be reported. A corresponding enum variant
  `radicle::node::Command::SeedsFor` is added.

### Changed

- `radicle::storage::git::Storage::repositories_by_id` returns
  `impl Iterator<Item = Result<RepositoryInfo, RepositoryError>>` instead of
  `Result<Vec<RepositoryInfo>, RepositoryError>`. Allowing callers to handle
  failures on a per-repository basis rather than having the entire operation
  fail if a single repository lookup fails.
- Re-exports from `git2` at `radicle::git::raw` were limited, using
  the heartwood workspace as a filter. Dependents that require members that
  are not exported anymore will have to depend on `git2` directly.
- Some re-exports from `git-ref-format-core` were moved from `radicle::git`
  to `radicle::fmt`.
- The crate now re-exports `radicle::git::Oid` from a new `radicle-oid` crate,
  in an effort to decrease dependence on `git2` via `radicle-git-ext`. This
  new object identifier type does not implement `Deref` anymore. Use `Into`
  to convert to a `git2::Oid` as necessary.
- Re-exports of `radicle-git-ext` were removed, as this dependency is removed.
  Instead of `radicle_git_ext::Error`, use `git2::Error` (re-exported as
  `radicle::git::raw::Error`) together with the new extension trait
  `radicle::git::raw::ErrorExt`.

### Deprecated

- `radicle::node::Handle::announce_refs` is deprecated in favor of
  `radicle::node::Handle::announce_refs_for`.
- `radicle::node::Handle::seeds` is deprecated in favor of
  `radicle::node::Handle::seeds_for`.

### Removed

- The data returned by `Seeds` contains `state`, which in turn contained the
  field `fetching` for ongoing fetches of that node, if in the `Connected`
  state. `Connected` no longer contains that field.

### Security

## 0.20.0

### Added

- Introduce a node event for canonical reference updates, `Event::CanonicalRefUpdated`.
  Whenever the node fetches new updates, it checks if canonical references can
  be updated. The node has learned how to return these results and emit them as
  node events. This is a breaking change since it adds a new variant the `Event`
  type.
- Add `#[non_exhaustive]` to `Event` to prevent any further breaking changes
  when adding new variants.

### Changed

- `radicle::profile::Home::socket` defaults to the path `\\.\pipe\radicle-node`
  on Windows. The behavior on Unix-like systems has *not* changed.

### Removed

- `radicle::node::DEFAULT_SOCKET_NAME`, use `radicle::profile::Home::socket`
  instead.

### Security

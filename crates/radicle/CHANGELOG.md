# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

### Changed

### Removed

### Security

## 0.21.0

### Added

- `radicle::node::command::Command` added variants `AnnounceRefsFor`,
  `SeedsFor`, and `Block`.
- `radicle::cob::identity::ApplyError` now contains a new variant
  `NonDelegateUnauthorized`.
- `radicle::node::Handle` added a `block` method to allow setting the follow
  policy to `Policy::Block`.
- `radicle::node::Handle::announce_refs_for` now allows specifying for which
  namespaces changes should be announced. A corresponding enum variant
  `radicle::node::Command::AnnounceRefsFor` is added.
- `radicle::node::Handle::seeds_for` now allows specifying for which
  namespaces sync status should be reported. A corresponding enum variant
  `radicle::node::Command::SeedsFor` is added.

### Changed

- The discriminant values for `radicle::node::command::Command` have changed.
- `radicle::rad::CheckoutError` is now marked as `non_exhaustive`.
- `radicle::storage::git::Storage::repositories_by_id` returns
  `impl Iterator<Item = Result<RepositoryInfo, RepositoryError>>` instead of
  `Result<Vec<RepositoryInfo>, RepositoryError>`. The method now also requires
  one generic type parameter. Allowing callers to handle failures on a
  per-repository basis rather than having the entire operation fail if a single
  repository lookup fails.
- `radicle::node::Node::announce` now takes an additional parameter to specify
  for which namespaces changes should be announced.
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

- `radicle::storage::RepositoryError::GitExt` was removed as a variant, where
  `RepositoryError::Git` now subsumes all Git errors.
- `radicle::identity::doc::DocError::GitExt` was removed as a variant, where
  `DocError::Git` now subsumes all Git errors.
- `radicle::storage::refs::Error::GitExt` was removed as a variant, where
  `Error::Git` now subsumes all Git errors.
- `radicle::cob::identity::ApplyError::GitExt` was removed as a variant, where
  `ApplyError::Git` now subsumes all Git errors.
- `radicle::storage::Error::GitExt` was removed as a variant, where
  `Error::Git` now subsumes all Git errors.
- `radicle::storage::git::cob::ObjectsError::GitExt` was removed as a variant,
  where `ObjectsError::Git` now subsumes all Git errors.
- `radicle::git::canonical::rules::CanonicalError::References` was removed as a
  variant, as it no longer occurs as an error.
- The `radicle::node::State::Connected` variant no longer has a `fetching`
  field. Fetching information is now tracked in the service.
- The data returned by `Seeds` contains `state`, which in turn contained the
  field `fetching` for ongoing fetches of that node, if in the `Connected`
  state. `Connected` no longer contains that field.
- `radicle::identity::doc::RepoId` was removed, along with its re-exports at
  `radicle::identity::RepoId` and `radicle::prelude::RepoId`. The type is now
  provided by the `radicle-core` crate.
- `radicle::identity::doc::IdError` was removed, along with its re-export at
  `radicle::identity::IdError`.
- `radicle::identity::doc::id::RAD_PREFIX` constant was removed.
- `radicle::identity::doc::VersionError::UnkownVersion` variant was renamed to
  `UnknownVersion`, correcting the typo.
  The typo has been corrected to `UnknownVersion`.
- `radicle::storage::git::RefError` was removed.
- `radicle::storage::git::UserInfo` was removed.
- `radicle::storage::git::NAMESPACES_GLOB`, `radicle::storage::git::CANONICAL_IDENTITY`,
  and `radicle::storage::git::SIGREFS_GLOB` static variables were removed.
- `radicle::storage::git::trailers::SIGNATURE_TRAILER` constant was removed.
- The `radicle::serde_ext::localtime` module and its submodules (`time`,
  `option::time`, `duration`) were removed, including all associated
  serialize/deserialize functions. The `radicle-localtime` crate is introduced
  and provides these helpers.
- The `radicle::schemars_ext::crypto` module was removed, including the
  `PublicKey` schema type. The schema is now provided by `radicle-crypto`.
- The test storage modules under `radicle::test::storage::git` and their
  submodules (`transport`, `cob`, `trailers`, `paths`, `temp`) were removed
  from the public API, along with all associated types, traits, and functions.

### Security

*No security updates.*

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

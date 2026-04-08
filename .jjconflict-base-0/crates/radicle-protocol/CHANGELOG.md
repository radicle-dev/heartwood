# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

### Changed

### Removed

### Security

## 0.7.0

### Added

- `radicle_protocol::fetcher::state::QueuedFetch` has a new required `config`
  field. Existing struct literals must be updated to include this field.
- `radicle_protocol::fetcher::state::command::Fetch` has a new required `config`
  field. Existing struct literals must be updated to include this field.
- `radicle_protocol::service::io::Io::Fetch` variant has a new `config` field.
- `radicle_protocol::fetcher::state::event::Fetch::Started` variant has a new
  `config` field.
- `radicle_protocol::fetcher::state::event::Fetch::QueueAtCapacity` variant has
  a new `config` field.
- `radicle_protocol::worker::FetchRequest::Initiator` variant has a new `config`
  field.
- `radicle_protocol::service::command::Command::Fetch` tuple variant has a new
  field at position 4.

### Changed

- `radicle_protocol::service::command::Command::fetch` now takes 4 parameters
  instead of 3.

### Removed

- `radicle_protocol::fetcher::state::QueuedFetch` field `timeout` was removed.
- `radicle_protocol::fetcher::state::command::Fetch` field `timeout` was
  removed.
- `radicle_protocol::fetcher::state::event::Fetch::Started` field `timeout` was
  removed.
- `radicle_protocol::fetcher::state::event::Fetch::QueueAtCapacity` field
  `timeout` was removed.
- `radicle_protocol::service::io::Io::Fetch` field `timeout` was removed.

## 0.6.0

### Changed

- Dependency update of `radicle` to `0.22`.

## 0.5.0

### Added

- `radicle_protocol::service::DisconnectReason` added a `Policy` variant.
- `radicle_protocol::service::ConnectError` added `UnsupportedAddress` and
  `Blocked` variants.
- `radicle_protocol::service::command::Command` added a `Block` variant.
- `radicle_protocol::service::command::Command::AnnounceRefs` now carries an
  additional field for specifying namespaces.
- `radicle_protocol::service::command::Command::Seeds` now carries an
  additional field for specifying namespaces.

### Changed

- `radicle_protocol::service::session::Session::outbound` now takes 4
  parameters instead of 5. Fetching information is no longer tracked in the
  session.
- `radicle_protocol::service::session::Session::inbound` now takes 5
  parameters instead of 6. Fetching information is no longer tracked in the
  session.

### Removed

- `radicle_protocol::service::CommandError` was removed.
- `radicle_protocol::service::Error::GitExt` was removed as a variant, where
  `Error::Git` now subsumes all Git errors.
- The `queue` field was removed from the `Session` struct. Fetching information
  is now tracked in the service rather than per-session.
- The following methods were removed from `Session`: `is_at_capacity`,
  `is_fetching`, `queue_fetch`, `dequeue_fetch`, `fetching`, and `fetched`.

### Security

*No security updates.*

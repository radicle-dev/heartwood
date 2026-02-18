# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

### Changed

### Removed

### Security

## 0.17.0

### Added

- The test `Handle` struct now has a `blocked` field.

### Removed

- The `radicle_node::wire` module was removed, including the `Wire` struct,
  the `Control` enum, the `dial` and `accept` functions, and the
  `NOISE_XK`, `DEFAULT_CONNECTION_TIMEOUT`, `DEFAULT_DIAL_TIMEOUT`, and
  `MAX_INBOX_SIZE` constants.
- The `radicle_node::control` module was removed, including the `Error` enum
  and the `listen` function.
- The `radicle_node::worker` module was removed, including the `Config`,
  `Pool`, `Task`, `TaskResult`, `FetchConfig`, `Channels`, and
  `ChannelsConfig` structs, the `ChannelEvent` enum, and the
  `worker::fetch::Handle` enum.
- The `radicle_node::worker::garbage` module was removed, including the
  `Expiry` enum, the `collect` function, and the `EXPIRY_DEFAULT` constant.

### Security

*No security updates.*

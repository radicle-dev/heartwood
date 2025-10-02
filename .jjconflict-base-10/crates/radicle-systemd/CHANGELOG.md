# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

### Changed

### Removed

### Security

## 0.13.0

### Changed

- `radicle_systemd::listen::fd` is now marked `unsafe`. On success
  (i.e. when it returns `Ok(Some(_))`), it removes the `LISTEN_PID`,
  `LISTEN_FDS`, and `LISTEN_FDNAMES` environment variables via
  `std::env::remove_var`, and inherits that function's safety
  contract: callers must ensure no other thread is concurrently
  reading or writing environment variables at the point of the call.
  In practice, call this early in `main` — before spawning threads
  and before any code (Rust or FFI) that may read the environment.

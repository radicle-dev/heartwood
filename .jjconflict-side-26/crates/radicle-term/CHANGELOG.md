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

- `println` is renamed to `println_prefixed` to better represent the functions behavior.
- The new `println` achieves the same behavior as `println!`.
- The new `print_inline` is renamed to `print`, and acts the same as `print!`.
- All print function variants now swallow `BrokenPipe` errors.

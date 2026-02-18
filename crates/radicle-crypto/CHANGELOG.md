# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

### Changed

### Removed

### Security

## 0.15.0

### Changed

- The following enums are now marked as `non_exhaustive`:
  `radicle_crypto::SignatureError`, `radicle_crypto::PublicKeyError`,
  `radicle_crypto::ssh::PublicKeyError`, `radicle_crypto::ssh::SignatureError`,
  `radicle_crypto::ssh::SecretKeyError`,
  `radicle_crypto::ssh::ExtendedSignatureError`,
  `radicle_crypto::ssh::keystore::Error`, and
  `radicle_crypto::ssh::keystore::MemorySignerError`.

### Removed

- The `radicle-git-ext` Cargo feature was removed.

### Security

*No security updates.*

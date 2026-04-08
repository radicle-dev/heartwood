# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

### Changed

### Removed

### Security

## 0.16.0

### Changed

- `radicle_crypto::Signer` now has `Signer` (from the `signature` crate) as a
  supertrait. Downstream implementations of `radicle_crypto::Signer` must also
  implement this supertrait.
- The `sign` and `try_sign` methods were removed from `radicle_crypto::Signer`
  in favor of those provided by the `signature::Signer` supertrait.

### Removed

- `radicle_crypto::Verified` and `radicle_crypto::Unverified` marker structs
  were removed. Code using these types to parameterize verification state should
  be updated accordingly.

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

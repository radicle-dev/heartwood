// Copyright © 2022 The Radicle Link Contributors

#[cfg(feature = "git2")]
pub mod git;

#[cfg(feature = "stable-commit-ids")]
pub mod stable;

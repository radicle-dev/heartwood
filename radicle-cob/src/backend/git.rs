// Copyright © 2022 The Radicle Link Contributors

pub mod change;

/// Environment variable to set to overwrite the commit date for both the author and the committer.
///
/// The format must be a unix timestamp.
pub const RAD_COMMIT_TIME: &str = "RAD_COMMIT_TIME";

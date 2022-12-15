// Copyright © 2022 The Radicle Link Contributors
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

pub mod change;

/// Environment variable to set to overwrite the commit date for both the author and the committer.
///
/// The format must be a unix timestamp.
pub const RAD_COMMIT_TIME: &str = "RAD_COMMIT_TIME";

// Copyright © 2021 The Radicle Link Contributors
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

use git_ext::Oid;

pub mod store;
pub use store::{Create, Storage};

use crate::signatures::Signature;

/// A single change in the change graph. The layout of changes in the repository
/// is specified in the RFC (docs/rfc/0662-collaborative-objects.adoc)
/// under "Change Commits".
pub type Change = store::Change<Oid, Oid, Signature>;

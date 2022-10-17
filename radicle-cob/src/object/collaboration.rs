// Copyright © 2022 The Radicle Link Contributors
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

use std::collections::BTreeSet;

use git_ext::Oid;

use crate::{change, identity::Identity, Contents, History, ObjectId, TypeName};

pub mod error;

mod create;
pub use create::{create, Create};

mod get;
pub use get::get;

pub mod info;

mod list;
pub use list::list;

mod update;
pub use update::{update, Update};

/// A collaborative object
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollaborativeObject {
    /// The typename of this object
    pub(crate) typename: TypeName,
    /// The CRDT history we know about for this object
    pub(crate) history: History,
    /// The id of the object
    pub(crate) id: ObjectId,
}

impl CollaborativeObject {
    pub fn history(&self) -> &History {
        &self.history
    }

    pub fn id(&self) -> &ObjectId {
        &self.id
    }

    pub fn typename(&self) -> &TypeName {
        &self.typename
    }

    fn tips(&self) -> BTreeSet<Oid> {
        self.history.tips().into_iter().map(Oid::from).collect()
    }
}

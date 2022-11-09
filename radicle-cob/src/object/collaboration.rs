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

/// The full object identifier for a [`CollaborativeObject`].
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObjectIdentifier {
    /// The [`TypeName`] for the given [`CollaborativeObject`].
    pub name: TypeName,
    /// The [`ObjectId`] for the given [`CollaborativeObject`].
    pub object: ObjectId,
}

impl ObjectIdentifier {
    /// Takes a `refname` and performs a best attempt to extract out the
    /// [`TypeName`] and [`ObjectId`] from it.
    ///
    /// This assumes that the `refname` is in a [`Qualified`] format. If
    /// it has any `refs/namespaces`, they will be stripped to access the
    /// underlying [`Qualified`] format.
    ///
    /// In the [`Qualified`] format it assumes that the reference name is
    /// of the form:
    ///
    ///   `refs/<category>/<typename>/<object_id>[/<rest>*]`
    ///
    /// Note that their may be more components to the path after the
    /// [`ObjectId`] but they are ignored.
    ///
    /// Also note that this will return `None` if:
    ///
    ///   * The `refname` is not [`Qualified`]
    ///   * The parsing of the [`ObjectId`] fails
    ///   * The parsing of the [`TypeName`] fails
    pub fn from_refstr<R>(refname: &R) -> Option<Self>
    where
        R: AsRef<git_ref_format::RefStr>,
    {
        use git_ref_format::Qualified;
        let refname = refname.as_ref();
        let refs_cobs = match refname.to_namespaced() {
            None => Qualified::from_refstr(refname)?,
            Some(ns) => ns.strip_namespace_recursive(),
        };

        let (_refs, _cobs, typename, mut object_id) = refs_cobs.non_empty_components();
        let object = object_id
            .next()
            .and_then(|oid| oid.parse::<ObjectId>().ok())?;
        let name = typename.parse::<TypeName>().ok()?;
        Some(Self { name, object })
    }
}

/// A collaborative object
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollaborativeObject {
    /// The identifier for this object.
    pub(crate) identifier: ObjectIdentifier,
    /// The CRDT history we know about for this object.
    pub(crate) history: History,
}

impl CollaborativeObject {
    pub fn history(&self) -> &History {
        &self.history
    }

    pub fn identifier(&self) -> &ObjectIdentifier {
        &self.identifier
    }

    pub fn id(&self) -> &ObjectId {
        &self.identifier.object
    }

    pub fn typename(&self) -> &TypeName {
        &self.identifier.name
    }

    fn tips(&self) -> BTreeSet<Oid> {
        self.history.tips().into_iter().map(Oid::from).collect()
    }
}

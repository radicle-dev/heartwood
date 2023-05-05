// Copyright © 2022 The Radicle Link Contributors

use std::collections::BTreeSet;

use git_ext::Oid;

use crate::change::store::Manifest;
use crate::{change, History, ObjectId, TypeName};

pub mod error;

mod create;
pub use create::{create, Create};

mod get;
pub use get::get;

pub mod info;

mod list;
pub use list::list;

mod remove;
pub use remove::remove;

mod update;
pub use update::{update, Update, Updated};

/// A collaborative object
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollaborativeObject {
    /// The manifest of this object
    pub(crate) manifest: Manifest,
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
        &self.manifest.typename
    }

    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    fn tips(&self) -> BTreeSet<Oid> {
        self.history.tips().into_iter().map(Oid::from).collect()
    }
}

/// Takes a `refname` and performs a best attempt to extract out the
/// [`TypeName`] and [`ObjectId`] from it.
///
/// This assumes that the `refname` is in a
/// [`git_ref_format::Qualified`] format. If it has any
/// `refs/namespaces`, they will be stripped to access the underlying
/// [`git_ref_format::Qualified`] format.
///
/// In the [`git_ref_format::Qualified`] format it assumes that the
/// reference name is of the form:
///
///   `refs/<category>/<typename>/<object_id>[/<rest>*]`
///
/// Note that their may be more components to the path after the
/// [`ObjectId`] but they are ignored.
///
/// Also note that this will return `None` if:
///
///   * The `refname` is not [`git_ref_format::Qualified`]
///   * The parsing of the [`ObjectId`] fails
///   * The parsing of the [`TypeName`] fails
pub fn parse_refstr<R>(name: &R) -> Option<(TypeName, ObjectId)>
where
    R: AsRef<git_ext::ref_format::RefStr>,
{
    use git_ext::ref_format::Qualified;
    let name = name.as_ref();
    let refs_cobs = match name.to_namespaced() {
        None => Qualified::from_refstr(name)?,
        Some(ns) => ns.strip_namespace_recursive(),
    };

    let (_refs, _cobs, typename, mut object_id) = refs_cobs.non_empty_components();
    let object = object_id
        .next()
        .and_then(|oid| oid.parse::<ObjectId>().ok())?;
    let name = typename.parse::<TypeName>().ok()?;
    Some((name, object))
}

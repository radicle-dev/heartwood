// Copyright © 2022 The Radicle Link Contributors
use std::convert::Infallible;
use std::fmt::Debug;

use git_ext::Oid;
use nonempty::NonEmpty;

use crate::change::store::{Manifest, Version};
use crate::{change, Entry, History, ObjectId, TypeName};

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
pub struct CollaborativeObject<T> {
    /// The manifest of this object
    pub manifest: Manifest,
    /// The materialized object resulting from traversing the history.
    pub object: T,
    /// The history DAG.
    pub history: History,
    /// The id of the object
    pub id: ObjectId,
}

impl<T> CollaborativeObject<T> {
    pub fn object(&self) -> &T {
        &self.object
    }

    pub fn history(&self) -> &History {
        &self.history
    }

    pub fn id(&self) -> &ObjectId {
        &self.id
    }

    pub fn typename(&self) -> &TypeName {
        &self.manifest.type_name
    }

    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }
}

/// An object that can be built by evaluating a history.
pub trait Evaluate<R>: Sized + Debug + 'static {
    type Error: std::error::Error + Send + Sync + 'static;

    /// Initialize the object with the first (root) history entry.
    fn init(entry: &Entry, store: &R) -> Result<Self, Self::Error>;

    /// Apply a history entry to the evaluated state.
    fn apply(&mut self, entry: &Entry, store: &R) -> Result<(), Self::Error>;
}

impl<R> Evaluate<R> for NonEmpty<Entry> {
    type Error = Infallible;

    fn init(entry: &Entry, _store: &R) -> Result<Self, Self::Error> {
        Ok(Self::new(entry.clone()))
    }

    fn apply(&mut self, entry: &Entry, _store: &R) -> Result<(), Self::Error> {
        self.push(entry.clone());

        Ok(())
    }
}

/// Takes a `refname` and performs a best attempt to extract out the
/// [`TypeName`] and [`ObjectId`] from it.
///
/// This assumes that the `refname` is in a
/// [`git_ext::ref_format::Qualified`] format. If it has any
/// `refs/namespaces`, they will be stripped to access the underlying
/// [`git_ext::ref_format::Qualified`] format.
///
/// In the [`git_ext::ref_format::Qualified`] format it assumes that the
/// reference name is of the form:
///
///   `refs/<category>/<typename>/<object_id>[/<rest>*]`
///
/// Note that their may be more components to the path after the
/// [`ObjectId`] but they are ignored.
///
/// Also note that this will return `None` if:
///
///   * The `refname` is not [`git_ext::ref_format::Qualified`]
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

#![warn(clippy::unwrap_used)]
pub mod cache;
pub mod common;
pub mod identity;
pub mod issue;
pub mod job;
pub mod op;
pub mod patch;
pub mod store;
pub mod thread;

#[cfg(test)]
pub mod test;

pub use cache::{migrate, MigrateCallback};
pub use common::*;
pub use op::{ActorId, Op};
pub use radicle_cob::{
    change, history::EntryId, object, object::collaboration::error, type_name::TypeNameParse,
    CollaborativeObject, Contents, Create, Embed, Entry, Evaluate, History, Manifest, ObjectId,
    Store, TypeName, Update, Updated, Version,
};
pub use radicle_cob::{create, get, git, list, remove, update};

/// The exact identifier for a particular COB.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
pub struct TypedId {
    /// The identifier of the COB in the store.
    pub id: ObjectId,
    /// The type identifier of the COB in the store.
    pub type_name: TypeName,
}

impl std::fmt::Display for TypedId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.type_name, self.id)
    }
}

/// Errors that occur when parsing a Git refname into a [`TypedId`].
#[derive(Debug, thiserror::Error)]
pub enum ParseIdentifierError {
    #[error(transparent)]
    TypeName(#[from] TypeNameParse),
    #[error(transparent)]
    ObjectId(#[from] object::ParseObjectId),
}

impl TypedId {
    /// Returns `true` is the [`TypedId::type_name`] is for an
    /// [`issue::Issue`].
    pub fn is_issue(&self) -> bool {
        self.type_name == *issue::TYPENAME
    }

    /// Returns `true` is the [`TypedId::type_name`] is for an
    /// [`patch::Patch`].
    pub fn is_patch(&self) -> bool {
        self.type_name == *patch::TYPENAME
    }

    /// Returns `true` is the [`TypedId::type_name`] is for an
    /// [`identity::Identity`].
    pub fn is_identity(&self) -> bool {
        self.type_name == *identity::TYPENAME
    }

    /// Parse a [`crate::git::Namespaced`] refname into a [`TypedId`].
    ///
    /// All namespaces are stripped before parsing the suffix for the
    /// [`TypedId`] (see [`TypedId::from_qualified`]).
    pub fn from_namespaced(
        n: &crate::git::Namespaced,
    ) -> Result<Option<Self>, ParseIdentifierError> {
        Self::from_qualified(&n.strip_namespace_recursive())
    }

    /// Parse a [`crate::git::Qualified`] refname into a [`TypedId`].
    ///
    /// The refname is expected to be of the form:
    ///     `refs/cobs/<type name>/<object id>`
    ///
    /// If the refname is not of that form then `None` will be returned.
    ///
    /// # Errors
    ///
    /// This will fail if the refname is of the correct form, but the
    /// type name or object id fail to parse.
    pub fn from_qualified(q: &crate::git::Qualified) -> Result<Option<Self>, ParseIdentifierError> {
        match q.non_empty_iter() {
            ("refs", "cobs", type_name, mut id) => {
                let Some(id) = id.next() else {
                    return Ok(None);
                };
                Ok(Some(Self {
                    id: id.parse()?,
                    type_name: type_name.parse()?,
                }))
            }
            _ => Ok(None),
        }
    }
}

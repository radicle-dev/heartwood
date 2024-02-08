#![warn(clippy::unwrap_used)]
pub mod cache;
pub mod common;
pub mod identity;
pub mod issue;
pub mod op;
pub mod patch;
pub mod store;
pub mod thread;

#[cfg(test)]
pub mod test;

pub use common::*;
pub use op::{ActorId, Op};
pub use radicle_cob::{
    change, history::EntryId, object, object::collaboration::error, type_name::TypeNameParse,
    CollaborativeObject, Contents, Create, Embed, Entry, Evaluate, History, Manifest, ObjectId,
    Store, TypeName, Update, Updated, Version,
};
pub use radicle_cob::{create, get, git, list, remove, update};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TypedId {
    pub id: ObjectId,
    pub type_name: TypeName,
}

#[derive(Debug, thiserror::Error)]
pub enum ParseIdentifierError {
    #[error(transparent)]
    TypeName(#[from] TypeNameParse),
    #[error(transparent)]
    ObjectId(#[from] object::ParseObjectId),
}

impl TypedId {
    pub fn is_issue(&self) -> bool {
        self.type_name == *issue::TYPENAME
    }

    pub fn is_patch(&self) -> bool {
        self.type_name == *patch::TYPENAME
    }

    pub fn from_namespaced(
        n: &crate::git::Namespaced,
    ) -> Result<Option<Self>, ParseIdentifierError> {
        Self::from_qualified(&n.strip_namespace())
    }

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

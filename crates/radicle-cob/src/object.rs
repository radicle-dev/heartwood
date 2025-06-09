// Copyright © 2022 The Radicle Link Contributors

use std::{convert::TryFrom as _, fmt, ops::Deref, str::FromStr};

use git_ext::ref_format::{Component, RefString};
use git_ext::Oid;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub mod collaboration;
pub use collaboration::{
    create, get, info, list, parse_refstr, remove, update, CollaborativeObject, Create, Evaluate,
    Update, Updated,
};

pub mod storage;
pub use storage::{Commit, Objects, Reference, Storage};

#[derive(Debug, Error)]
pub enum ParseObjectId {
    #[error(transparent)]
    Git(#[from] git2::Error),
}

/// The id of an object
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(transparent)]
pub struct ObjectId(Oid);

impl FromStr for ObjectId {
    type Err = ParseObjectId;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let oid = Oid::from_str(s)?;
        Ok(ObjectId(oid))
    }
}

impl From<Oid> for ObjectId {
    fn from(oid: Oid) -> Self {
        ObjectId(oid)
    }
}

impl From<&Oid> for ObjectId {
    fn from(oid: &Oid) -> Self {
        (*oid).into()
    }
}

impl From<git2::Oid> for ObjectId {
    fn from(oid: git2::Oid) -> Self {
        Oid::from(oid).into()
    }
}

impl From<&git2::Oid> for ObjectId {
    fn from(oid: &git2::Oid) -> Self {
        ObjectId(Oid::from(*oid))
    }
}

impl Deref for ObjectId {
    type Target = Oid;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl fmt::Display for ObjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&ObjectId> for Component<'_> {
    fn from(id: &ObjectId) -> Self {
        let refstr = RefString::from(*id);

        Component::from_refstr(refstr)
            .expect("collaborative object id's are valid refname components")
    }
}

impl From<ObjectId> for RefString {
    fn from(id: ObjectId) -> Self {
        RefString::try_from(id.0.to_string())
            .expect("collaborative object id's are valid ref strings")
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_serde() {
        let id = ObjectId::from_str("3ad84420bd882f983c2f9b605e7a68f5bdf95f5c").unwrap();

        assert_eq!(
            serde_json::to_string(&id).unwrap(),
            serde_json::to_string(&id.0).unwrap()
        );
    }
}

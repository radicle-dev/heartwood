// Copyright © 2022 The Radicle Link Contributors

use std::str::FromStr;

use fmt::{Component, RefString};
use oid::Oid;

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub mod collaboration;
pub use collaboration::{
    CollaborativeObject, Create, Evaluate, Update, Updated, create, get, info, list, parse_refstr,
    remove, update,
};

pub mod storage;
pub use storage::{Commit, Objects, Reference, Storage};

#[derive(Debug, Error)]
#[error(transparent)]
pub struct ParseObjectId(#[from] oid::str::ParseOidError);

/// The id of an object
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(transparent)]
pub struct ObjectId(Oid);

impl FromStr for ObjectId {
    type Err = ParseObjectId;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Oid::from_str(s)?))
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

impl From<ObjectId> for Oid {
    fn from(id: ObjectId) -> Self {
        id.0
    }
}

#[cfg(feature = "git2")]
impl From<git2::Oid> for ObjectId {
    fn from(oid: git2::Oid) -> Self {
        Self(Oid::from(oid))
    }
}

#[cfg(feature = "git2")]
impl From<&git2::Oid> for ObjectId {
    fn from(oid: &git2::Oid) -> Self {
        Self(Oid::from(*oid))
    }
}

impl std::ops::Deref for ObjectId {
    type Target = Oid;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::fmt::Display for ObjectId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&ObjectId> for Component<'_> {
    fn from(id: &ObjectId) -> Self {
        Self::from(&id.0)
    }
}

impl From<&ObjectId> for RefString {
    fn from(id: &ObjectId) -> Self {
        Self::from(&id.0)
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

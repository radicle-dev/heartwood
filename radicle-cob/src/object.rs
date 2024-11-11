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
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
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

impl Serialize for ObjectId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        git_ext::Oid::serialize(&self.0, serializer)
    }
}

impl<'de> Deserialize<'de> for ObjectId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        /// We use an internal enum here for backwards-compatibility. Initially,
        /// `ObjectId` was serialized into its byte representation, which would
        /// be represented as an array in JSON. To support this we use the enum
        /// for deserializing into either a `Vec` or `str`.
        ///
        /// Note that `Serialize` now serializes into `str` via the `git_ext::Oid`.
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Internal<'a> {
            Buffer(Vec<u8>),
            Str(&'a str),
        }

        impl<'a> TryFrom<Internal<'a>> for ObjectId {
            type Error = ParseObjectId;

            fn try_from(value: Internal) -> Result<Self, Self::Error> {
                match value {
                    Internal::Buffer(bs) => git2::Oid::from_bytes(&bs)
                        .map_err(ParseObjectId::from)
                        .map(ObjectId::from),
                    Internal::Str(s) => ObjectId::from_str(s).map(ObjectId::from),
                }
            }
        }

        let internal = Internal::deserialize(deserializer).map_err(|e| {
            serde::de::Error::custom(format!("expected sequence of bytes or a string: {e}"))
        })?;
        ObjectId::try_from(internal).map_err(|e| {
            serde::de::Error::custom(format!("failed to deserialize object identifier: {e}"))
        })
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

#[allow(clippy::unwrap_used)]
#[cfg(test)]
mod tests {
    use qcheck::quickcheck;
    use serde_json as json;

    use super::*;

    #[quickcheck]
    fn test_str_repr(bytes: [u8; 20]) -> bool {
        let oid = git2::Oid::from_bytes(&bytes).unwrap();
        let object = ObjectId::from(oid);

        oid.to_string() == object.to_string()
    }

    #[quickcheck]
    fn test_ser_commutes(bytes: [u8; 20]) -> bool {
        let oid = git_ext::Oid::from(git2::Oid::from_bytes(&bytes).unwrap());
        let object = ObjectId::from(oid);
        json::to_string(&oid).unwrap() == json::to_string(&object).unwrap()
    }

    #[quickcheck]
    fn test_de_commutes(bytes: [u8; 20]) -> bool {
        let oid = git_ext::Oid::from(git2::Oid::from_bytes(&bytes).unwrap());
        let object = ObjectId::from(oid);
        json::from_str::<ObjectId>(&json::to_string(&oid).unwrap()).unwrap()
            == json::from_str(&json::to_string(&object).unwrap()).unwrap()
    }

    #[quickcheck]
    fn test_refstring(bytes: [u8; 20]) -> bool {
        let oid = git_ext::Oid::from(git2::Oid::from_bytes(&bytes).unwrap());
        let object = ObjectId::from(oid);
        Component::from(&object).to_string() == object.to_string()
            && RefString::from(object).to_string() == object.to_string()
    }

    #[quickcheck]
    fn test_ensure_backwards_compatible_bytes_parser(bytes: [u8; 20]) -> bool {
        json::from_str::<ObjectId>(&json::to_string(&bytes).unwrap()).is_ok()
    }
}

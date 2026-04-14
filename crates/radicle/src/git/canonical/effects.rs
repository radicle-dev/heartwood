use std::collections::{BTreeMap, BTreeSet};

use crate::git;
use crate::git::Oid;
use crate::git::fmt::Qualified;
use crate::git::repository::object;
use crate::git::repository::reference;
use crate::git::repository::user;
use crate::prelude::Did;

use super::{FoundObjects, Object};

/// Finds objects for the canonical computation by resolving namespaced
/// references and determining their object types.
pub struct FindObjects<'a, 'b, R> {
    repository: &'a R,
    refname: &'b Qualified<'b>,
    dids: &'b [Did],
}

impl<'a, 'b, R> FindObjects<'a, 'b, R>
where
    R: reference::Reader + object::Reader,
{
    /// Construct a new [`FindObjects`] query.
    pub fn new(repository: &'a R, refname: &'b Qualified<'b>, dids: &'b [Did]) -> Self {
        Self {
            repository,
            refname,
            dids,
        }
    }

    /// Resolve all references and produce the [`FoundObjects`].
    pub fn resolve(self) -> Result<FoundObjects, FindObjectsError> {
        let mut objects = BTreeMap::new();
        let mut missing_refs = BTreeSet::new();
        let mut missing_objects = BTreeMap::new();

        for did in self.dids {
            let name = self.refname.with_namespace(did.as_key().into());

            let oid = match self.repository.ref_target(&name) {
                Ok(Some(oid)) => oid,
                Ok(None) => {
                    missing_refs.insert(name.to_owned());
                    continue;
                }
                Err(e) => {
                    return Err(FindObjectsError::find_reference(name.to_owned(), e));
                }
            };

            let kind = match self.repository.object_kind(oid) {
                Ok(Some(kind)) => kind,
                Ok(None) => {
                    missing_objects.insert(*did, oid);
                    continue;
                }
                Err(e) => return Err(FindObjectsError::find_object(oid, e)),
            };

            let object = Object::from_kind(oid, kind).ok_or_else(|| {
                FindObjectsError::invalid_object_type(*did, oid, Some(kind.to_string()))
            })?;

            objects.insert(*did, object);
        }

        Ok(FoundObjects {
            objects,
            missing_refs,
            missing_objects,
        })
    }
}

/// Error produced by [`FindObjects::resolve`].
#[derive(Debug, thiserror::Error)]
pub enum FindObjectsError {
    #[error(transparent)]
    InvalidObjectType(#[from] InvalidObjectType),
    #[error(transparent)]
    MissingObject(#[from] MissingObject),
    #[error("failed to find object {oid}: {source}")]
    FindObject {
        oid: Oid,
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
    #[error("failed to find reference {refname}: {source}")]
    FindReference {
        refname: git::fmt::Namespaced<'static>,
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
}

impl FindObjectsError {
    pub fn find_object<E>(oid: Oid, err: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        Self::FindObject {
            oid,
            source: Box::new(err),
        }
    }

    pub fn find_reference<E>(refname: git::fmt::Namespaced<'static>, err: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        Self::FindReference {
            refname,
            source: Box::new(err),
        }
    }

    pub fn missing_object<E>(did: Did, oid: Oid, err: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        MissingObject {
            did,
            commit: oid,
            source: Box::new(err),
        }
        .into()
    }

    pub fn invalid_object_type(did: Did, oid: Oid, kind: Option<String>) -> Self {
        InvalidObjectType { did, oid, kind }.into()
    }
}

#[derive(Debug, thiserror::Error)]
#[error("the object {oid} for {did} is of unexpected type {kind:?}")]
pub struct InvalidObjectType {
    did: Did,
    oid: Oid,
    kind: Option<String>,
}

#[derive(Debug, thiserror::Error)]
#[error("the commit {commit} for {did} is missing")]
pub struct MissingObject {
    did: Did,
    commit: Oid,
    source: Box<dyn std::error::Error + Send + Sync + 'static>,
}

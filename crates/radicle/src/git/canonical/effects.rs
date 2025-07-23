use std::collections::{BTreeMap, BTreeSet};

use crate::git;
use crate::git::{Oid, Qualified};
use crate::prelude::Did;

use super::{FoundObjects, GraphAheadBehind, MergeBase, Object};

/// Find objects for the canonical computation.
///
/// Typically implemented by a Git repository.
pub trait FindObjects {
    /// Find the objects for the given [`Qualified`] reference name, for each
    /// [`Did`]'s namespace.
    ///
    /// The resulting [`FoundObjects`] includes all objects that were found, the
    /// references that were missing, and the objects that were missing (if the
    /// reference was found).
    fn find_objects<'a, 'b, I>(
        &self,
        refname: &Qualified<'a>,
        dids: I,
    ) -> Result<FoundObjects, FindObjectsError>
    where
        I: Iterator<Item = &'b Did>;
}

/// Error produced by the [`FindObjects::find_objects`] method.
#[derive(Debug, thiserror::Error)]
pub enum FindObjectsError {
    #[error(transparent)]
    InvalidObjectType(#[from] InvalidObjectType),
    #[error(transparent)]
    MissingObject(#[from] MissingObject),
    #[error("failed to find object {oid} due to: {source}")]
    FindObject {
        oid: Oid,
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
    #[error("failed to find reference {refname} due to: {source}")]
    FindReference {
        refname: git::Namespaced<'static>,
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
    #[error("failed to find objects")]
    Other {
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

    pub fn find_reference<E>(refname: git::Namespaced<'static>, err: E) -> Self
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

    pub fn other<E>(err: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        Self::Other {
            source: Box::new(err),
        }
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

/// Find the merge base of two commits.
///
/// Typically implemented by a Git repository.
pub trait FindMergeBase {
    /// Produce the [`MergeBase`] of commits `a` and `b`.
    fn merge_base(&self, a: Oid, b: Oid) -> Result<MergeBase, MergeBaseError>;
}

#[derive(Debug, thiserror::Error)]
#[error("failed to find merge base for {a} and {b} due to: {source}")]
pub struct MergeBaseError {
    a: Oid,
    b: Oid,
    source: Box<dyn std::error::Error + Send + Sync + 'static>,
}

impl MergeBaseError {
    pub fn new<E>(a: Oid, b: Oid, source: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        Self {
            a,
            b,
            source: Box::new(source),
        }
    }
}

/// Calculate the ancestry of two commits.
///
/// Typically implemented by a Git repository.
pub trait Ancestry {
    /// Produce the [`GraphAheadBehind`] of `commit` and `upstream`.
    ///
    /// The result should provide how many commits are ahead and behind when
    /// comparing the `commit` and `upstream`.
    fn graph_ahead_behind(
        &self,
        commit: Oid,
        upstream: Oid,
    ) -> Result<GraphAheadBehind, GraphDescendant>;
}

#[derive(Debug, thiserror::Error)]
#[error("failed to check if {commit} is an ancestor of {upstream} due to: {source}")]
pub struct GraphDescendant {
    commit: Oid,
    upstream: Oid,
    source: Box<dyn std::error::Error + Send + Sync + 'static>,
}

// ===========================================
// `git2` implementations of the above effects
// ===========================================

impl FindMergeBase for git2::Repository {
    fn merge_base(&self, a: Oid, b: Oid) -> Result<MergeBase, MergeBaseError> {
        self.merge_base(*a, *b)
            .map_err(|err| MergeBaseError {
                a,
                b,
                source: Box::new(err),
            })
            .map(|base| MergeBase {
                a,
                b,
                base: base.into(),
            })
    }
}

impl Ancestry for git2::Repository {
    fn graph_ahead_behind(
        &self,
        commit: Oid,
        upstream: Oid,
    ) -> Result<GraphAheadBehind, GraphDescendant> {
        self.graph_ahead_behind(*commit, *upstream)
            .map_err(|err| GraphDescendant {
                commit,
                upstream,
                source: Box::new(err),
            })
            .map(|(ahead, behind)| GraphAheadBehind { ahead, behind })
    }
}

impl FindObjects for git2::Repository {
    fn find_objects<'a, 'b, I>(
        &self,
        refname: &Qualified,
        dids: I,
    ) -> Result<FoundObjects, FindObjectsError>
    where
        I: Iterator<Item = &'b Did>,
    {
        let mut objects = BTreeMap::new();
        let mut missing_refs = BTreeSet::new();
        let mut missing_objects = BTreeMap::new();
        for did in dids {
            let name = &refname.with_namespace(did.as_key().into());
            let reference = match self.find_reference(name.as_str()) {
                Ok(reference) => reference,
                Err(e) if git::ext::is_not_found_err(&e) => {
                    missing_refs.insert(name.to_owned());
                    continue;
                }
                Err(e) => {
                    return Err(FindObjectsError::find_reference(name.to_owned(), e));
                }
            };
            let Some(oid) = reference.target().map(Oid::from) else {
                log::warn!(target: "radicle", "Missing target for reference `{name}`");
                continue;
            };
            let object = match self.find_object(*oid, None) {
                Ok(object) => Object::new(&object).ok_or_else(|| {
                    FindObjectsError::invalid_object_type(
                        *did,
                        oid,
                        object.kind().map(|kind| kind.to_string()),
                    )
                }),
                Err(err) if git::ext::is_not_found_err(&err) => {
                    missing_objects.insert(*did, oid);
                    continue;
                }
                Err(err) => Err(FindObjectsError::find_object(oid, err)),
            };
            objects.insert(*did, object?);
        }
        Ok(FoundObjects {
            objects,
            missing_refs,
            missing_objects,
        })
    }
}

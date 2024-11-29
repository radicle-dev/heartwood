// Copyright © 2021 The Radicle Link Contributors

use std::{collections::BTreeMap, error::Error};

use git_ext::ref_format::RefString;
use git_ext::Oid;
use radicle_crypto::PublicKey;

use crate::change::EntryId;
use crate::{ObjectId, TypeName};

/// The [`Reference`]s that refer to the commits that make up a
/// [`crate::CollaborativeObject`].
#[derive(Clone, Debug, Default)]
pub struct Objects(Vec<Reference>);

impl Objects {
    pub fn new(reference: Reference) -> Self {
        Self(vec![reference])
    }

    pub fn push(&mut self, reference: Reference) {
        self.0.push(reference)
    }

    /// Return an iterator over the `local` and `remotes` of the given
    /// [`Objects`].
    pub fn iter(&self) -> impl Iterator<Item = &Reference> {
        self.0.iter()
    }
}

impl From<Vec<Reference>> for Objects {
    fn from(refs: Vec<Reference>) -> Self {
        Objects(refs)
    }
}

/// A [`Reference`] that must directly point to the [`Commit`] for a
/// [`crate::CollaborativeObject`].
#[derive(Clone, Debug)]
pub struct Reference {
    /// The `name` of the reference.
    pub name: RefString,
    /// The [`Commit`] that this reference points to.
    pub target: Commit,
}

/// A [`Commit`] that holds the data for a given [`crate::CollaborativeObject`].
#[derive(Clone, Debug)]
pub struct Commit {
    /// The content identifier of the commit.
    pub id: Oid,
}

pub trait Storage {
    type ObjectsError: Error + Send + Sync + 'static;
    type TypesError: Error + Send + Sync + 'static;
    type UpdateError: Error + Send + Sync + 'static;
    type RemoveError: Error + Send + Sync + 'static;

    /// Get all references which point to a head of the change graph for a
    /// particular object
    fn objects(
        &self,
        typename: &TypeName,
        object_id: &ObjectId,
    ) -> Result<Objects, Self::ObjectsError>;

    /// Get all references to objects of a given type within a particular
    /// identity
    fn types(&self, typename: &TypeName) -> Result<BTreeMap<ObjectId, Objects>, Self::TypesError>;

    /// Update a ref to a particular collaborative object
    fn update(
        &self,
        node_id: &PublicKey,
        typename: &TypeName,
        object_id: &ObjectId,
        entry: &EntryId,
    ) -> Result<(), Self::UpdateError>;

    /// Remove a ref to a particular collaborative object
    fn remove(
        &self,
        node_id: &PublicKey,
        typename: &TypeName,
        object_id: &ObjectId,
    ) -> Result<(), Self::RemoveError>;
}

pub mod convert {
    use std::str;

    use git_ext::ref_format::RefString;
    use thiserror::Error;

    use super::{Commit, Reference};

    #[derive(Debug, Error)]
    pub enum Error {
        #[error("the reference '{name}' does not point to a commit object")]
        NotCommit {
            name: RefString,
            #[source]
            err: git2::Error,
        },
        #[error(transparent)]
        Ref(#[from] git_ext::ref_format::Error),
        #[error(transparent)]
        Utf8(#[from] str::Utf8Error),
    }

    impl<'a> TryFrom<git2::Reference<'a>> for Reference {
        type Error = Error;

        fn try_from(value: git2::Reference<'a>) -> Result<Self, Self::Error> {
            let name = RefString::try_from(str::from_utf8(value.name_bytes())?)?;
            let target = Commit::from(value.peel_to_commit().map_err(|err| Error::NotCommit {
                name: name.clone(),
                err,
            })?);
            Ok(Self { name, target })
        }
    }

    impl<'a> From<git2::Commit<'a>> for Commit {
        fn from(commit: git2::Commit<'a>) -> Self {
            Commit {
                id: commit.id().into(),
            }
        }
    }
}

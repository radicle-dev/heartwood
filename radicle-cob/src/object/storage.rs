// Copyright © 2021 The Radicle Link Contributors
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

use std::{collections::HashMap, error::Error};

use git_ext::Oid;
use git_ref_format::RefString;

use crate::change::Change;
use crate::{ObjectId, TypeName};

/// The [`Reference`]s that refer to the commits that make up a
/// [`crate::CollaborativeObject`].
#[derive(Clone, Debug)]
pub struct Objects {
    /// If the local peer has a [`Reference`] for this particular
    /// object, then `local` should be set.
    pub local: Option<Reference>,
    /// The `remotes` are the entries for each remote peer's version
    /// of the particular object.
    pub remotes: Vec<Reference>,
}

impl Objects {
    /// Return an iterator over the `local` and `remotes` of the given
    /// [`Objects`].
    pub fn iter(&self) -> impl Iterator<Item = &Reference> {
        self.local.iter().chain(self.remotes.iter())
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
    /// The parents of the commit.
    pub parents: Vec<Commit>,
}

pub trait Storage {
    type ObjectsError: Error + Send + Sync + 'static;
    type TypesError: Error + Send + Sync + 'static;
    type UpdateError: Error + Send + Sync + 'static;

    type Identifier;

    /// Get all references which point to a head of the change graph for a
    /// particular object
    fn objects(
        &self,
        identifier: &Self::Identifier,
        typename: &TypeName,
        object_id: &ObjectId,
    ) -> Result<Objects, Self::ObjectsError>;

    /// Get all references to objects of a given type within a particular
    /// identity
    fn types(
        &self,
        identifier: &Self::Identifier,
        typename: &TypeName,
    ) -> Result<HashMap<ObjectId, Objects>, Self::TypesError>;

    /// Update a ref to a particular collaborative object
    fn update(
        &self,
        identifier: &Self::Identifier,
        typename: &TypeName,
        object_id: &ObjectId,
        change: &Change,
    ) -> Result<(), Self::UpdateError>;
}

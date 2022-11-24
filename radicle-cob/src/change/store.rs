// Copyright © 2022 The Radicle Link Contributors
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

use std::{error::Error, fmt};

use serde::{Deserialize, Serialize};

use crate::{
    history::{Contents, HistoryType},
    signatures, TypeName,
};

pub trait Storage {
    type CreateError: Error + Send + Sync + 'static;
    type LoadError: Error + Send + Sync + 'static;

    type ObjectId;
    type Author;
    type Resource;
    type Signatures;

    #[allow(clippy::type_complexity)]
    fn create<Signer>(
        &self,
        author: Option<Self::Author>,
        authority: Self::Resource,
        signer: &Signer,
        spec: Create<Self::ObjectId>,
    ) -> Result<
        Change<Self::Author, Self::Resource, Self::ObjectId, Self::Signatures>,
        Self::CreateError,
    >
    where
        Signer: crypto::Signer;

    #[allow(clippy::type_complexity)]
    fn load(
        &self,
        id: Self::ObjectId,
    ) -> Result<
        Change<Self::Author, Self::Resource, Self::ObjectId, Self::Signatures>,
        Self::LoadError,
    >;
}

pub struct Create<Id> {
    pub typename: TypeName,
    pub history_type: HistoryType,
    pub tips: Vec<Id>,
    pub message: String,
    pub contents: Contents,
}

#[derive(Clone, Debug)]
pub struct Change<Author, Resource, Id, Signatures> {
    /// The content address of the `Change` itself.
    pub id: Id,
    /// The content address of the tree of the `Change`.
    pub revision: Id,
    /// The cryptographic signatures and their public keys of the
    /// authors.
    pub signatures: Signatures,
    /// The author of this change. The `Author` is expected to be a
    /// content address to look up the identity of the author.
    pub author: Option<Author>,
    /// The parent resource that this change lives under. For example,
    /// this change could be for a patch of a project.
    pub resource: Resource,
    /// The manifest describing the type of object as well as the type
    /// of history for this `Change`.
    pub manifest: Manifest,
    /// The contents that describe `Change`.
    pub contents: Contents,
}

impl<Author, Resource, Id, S> fmt::Display for Change<Author, Resource, Id, S>
where
    Id: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Change {{ id: {} }}", self.id)
    }
}

impl<Author, Resource, Id, Signatures> Change<Author, Resource, Id, Signatures> {
    pub fn id(&self) -> &Id {
        &self.id
    }

    pub fn author(&self) -> &Option<Author> {
        &self.author
    }

    pub fn typename(&self) -> &TypeName {
        &self.manifest.typename
    }

    pub fn contents(&self) -> &Contents {
        &self.contents
    }

    pub fn resource(&self) -> &Resource {
        &self.resource
    }
}

impl<A, R, Id> Change<A, R, Id, signatures::Signatures>
where
    Id: AsRef<[u8]>,
{
    pub fn valid_signatures(&self) -> bool {
        self.signatures
            .iter()
            .all(|(key, sig)| key.verify(self.revision.as_ref(), sig).is_ok())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Manifest {
    /// The name given to the type of collaborative object.
    pub typename: TypeName,
    /// The type of history for the collaborative oject.
    pub history_type: HistoryType,
}

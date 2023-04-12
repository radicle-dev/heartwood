// Copyright © 2022 The Radicle Link Contributors
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

use std::{error::Error, fmt};

use nonempty::NonEmpty;
use serde::{Deserialize, Serialize};

use crate::{
    history::{Contents, Timestamp},
    signatures, TypeName,
};

/// Change storage.
pub trait Storage {
    type StoreError: Error + Send + Sync + 'static;
    type LoadError: Error + Send + Sync + 'static;

    type ObjectId;
    type Parent;
    type Signatures;

    /// Store a new change.
    #[allow(clippy::type_complexity)]
    fn store<G>(
        &self,
        resource: Self::Parent,
        parents: Vec<Self::Parent>,
        signer: &G,
        template: Template<Self::ObjectId>,
    ) -> Result<Change<Self::Parent, Self::ObjectId, Self::Signatures>, Self::StoreError>
    where
        G: crypto::Signer;

    /// Load a change.
    #[allow(clippy::type_complexity)]
    fn load(
        &self,
        id: Self::ObjectId,
    ) -> Result<Change<Self::Parent, Self::ObjectId, Self::Signatures>, Self::LoadError>;
}

/// Change template, used to create a new change.
pub struct Template<Id> {
    pub typename: TypeName,
    pub history_type: String,
    pub tips: Vec<Id>,
    pub message: String,
    pub contents: NonEmpty<Vec<u8>>,
}

#[derive(Clone, Debug)]
pub struct Change<Resource, Id, Signature> {
    /// The content address of the `Change` itself.
    pub id: Id,
    /// The content address of the tree of the `Change`.
    pub revision: Id,
    /// The cryptographic signature(s) and their public keys of the
    /// authors.
    pub signature: Signature,
    /// The parent resource that this change lives under. For example,
    /// this change could be for a patch of a project.
    pub resource: Resource,
    /// Other parents this change depends on.
    pub parents: Vec<Resource>,
    /// The manifest describing the type of object as well as the type
    /// of history for this `Change`.
    pub manifest: Manifest,
    /// The contents that describe `Change`.
    pub contents: Contents,
    /// Timestamp of change.
    pub timestamp: Timestamp,
}

impl<Resource, Id, S> fmt::Display for Change<Resource, Id, S>
where
    Id: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Change {{ id: {} }}", self.id)
    }
}

impl<Resource, Id, Signatures> Change<Resource, Id, Signatures> {
    pub fn id(&self) -> &Id {
        &self.id
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

impl<R, Id> Change<R, Id, signatures::Signatures>
where
    Id: AsRef<[u8]>,
{
    pub fn valid_signatures(&self) -> bool {
        self.signature
            .iter()
            .all(|(key, sig)| key.verify(self.revision.as_ref(), sig).is_ok())
    }
}

impl<R, Id> Change<R, Id, signatures::ExtendedSignature>
where
    Id: AsRef<[u8]>,
{
    pub fn valid_signatures(&self) -> bool {
        self.signature.verify(self.revision.as_ref())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// The name given to the type of collaborative object.
    pub typename: TypeName,
    /// The type of history for the collaborative oject.
    pub history_type: String,
}

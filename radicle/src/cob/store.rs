//! Generic COB storage.
#![allow(clippy::large_enum_variant)]
use std::marker::PhantomData;

use radicle_crdt::Lamport;
use serde::Serialize;

use crate::cob;
use crate::cob::common::Author;
use crate::cob::CollaborativeObject;
use crate::cob::{Create, History, ObjectId, TypeName, Update};
use crate::crypto::PublicKey;
use crate::git;
use crate::identity::project;
use crate::prelude::*;
use crate::storage::git as storage;

/// History type for standard radicle COBs.
pub const HISTORY_TYPE: &str = "radicle";

/// A type that can be materialized from an event history.
/// All collaborative objects implement this trait.
pub trait FromHistory: Sized {
    type Action;

    /// The object type name.
    fn type_name() -> &'static TypeName;
    /// Create an object from a history.
    fn from_history(history: &History) -> Result<(Self, Lamport), Error>;
}

/// Store error.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("create error: {0}")]
    Create(#[from] cob::error::Create),
    #[error("update error: {0}")]
    Update(#[from] cob::error::Update),
    #[error("retrieve error: {0}")]
    Retrieve(#[from] cob::error::Retrieve),
    #[error("remove error: {0}")]
    Remove(#[from] cob::error::Remove),
    #[error(transparent)]
    Identity(#[from] project::IdentityError),
    #[error(transparent)]
    Serialize(#[from] serde_json::Error),
    #[error("object `{1}` of type `{0}` was not found")]
    NotFound(TypeName, ObjectId),
}

/// Storage for collaborative objects of a specific type `T` in a single project.
pub struct Store<'a, T> {
    whoami: PublicKey,
    project: project::Identity<git::Oid>,
    raw: &'a storage::Repository,
    witness: PhantomData<T>,
}

impl<'a, T> AsRef<storage::Repository> for Store<'a, T> {
    fn as_ref(&self) -> &storage::Repository {
        self.raw
    }
}

impl<'a, T> Store<'a, T> {
    /// Open a new generic store.
    pub fn open(whoami: PublicKey, store: &'a storage::Repository) -> Result<Self, Error> {
        let project = project::Identity::load(&whoami, store)?;

        Ok(Self {
            project,
            whoami,
            raw: store,
            witness: PhantomData,
        })
    }

    /// Get this store's author.
    pub fn author(&self) -> Author {
        Author::new(self.whoami)
    }

    /// Get the public key associated with this store.
    pub fn public_key(&self) -> &PublicKey {
        &self.whoami
    }
}

impl<'a, T: FromHistory> Store<'a, T>
where
    T::Action: Serialize,
{
    /// Update an object.
    pub fn update<G: Signer>(
        &self,
        object_id: ObjectId,
        message: &'static str,
        action: T::Action,
        signer: &G,
    ) -> Result<CollaborativeObject, Error> {
        let changes = encoding::encode(&action)?;

        cob::update(
            self.raw,
            signer,
            &self.project,
            signer.public_key(),
            Update {
                object_id,
                history_type: HISTORY_TYPE.to_owned(),
                typename: T::type_name().clone(),
                message: message.to_owned(),
                changes,
            },
        )
        .map_err(Error::from)
    }

    /// Create an object.
    pub fn create<G: Signer>(
        &self,
        message: &'static str,
        action: T::Action,
        signer: &G,
    ) -> Result<(ObjectId, T, Lamport), Error> {
        let contents = encoding::encode(&action)?;
        let cob = cob::create(
            self.raw,
            signer,
            &self.project,
            signer.public_key(),
            Create {
                history_type: HISTORY_TYPE.to_owned(),
                typename: T::type_name().clone(),
                message: message.to_owned(),
                contents,
            },
        )?;
        let (object, clock) = T::from_history(cob.history())?;

        Ok((*cob.id(), object, clock))
    }

    /// Get an object.
    pub fn get(&self, id: &ObjectId) -> Result<Option<(T, Lamport)>, Error> {
        let cob = cob::get(self.raw, T::type_name(), id)?;

        if let Some(cob) = cob {
            if cob.manifest().history_type != HISTORY_TYPE {
                panic!();
            }
            let (obj, clock) = T::from_history(cob.history())?;
            Ok(Some((obj, clock)))
        } else {
            Ok(None)
        }
    }

    /// Return all objects.
    pub fn all(
        &self,
    ) -> Result<impl Iterator<Item = Result<(ObjectId, T, Lamport), Error>>, Error> {
        let raw = cob::list(self.raw, T::type_name())?;

        Ok(raw.into_iter().map(|o| {
            let (obj, clock) = T::from_history(o.history())?;
            Ok((*o.id(), obj, clock))
        }))
    }

    /// Return objects count.
    pub fn count(&self) -> Result<usize, Error> {
        let raw = cob::list(self.raw, T::type_name())?;

        Ok(raw.len())
    }

    /// Remove an object.
    pub fn remove(&self, id: &ObjectId) -> Result<(), Error> {
        cob::remove(self.raw, &self.whoami, T::type_name(), id).map_err(Error::from)
    }
}

mod encoding {
    use serde::Serialize;

    /// Serialize the change into a byte string.
    pub fn encode<T: Serialize>(obj: &T) -> Result<Vec<u8>, serde_json::Error> {
        let mut buf = Vec::new();
        let mut serializer =
            serde_json::Serializer::with_formatter(&mut buf, olpc_cjson::CanonicalFormatter::new());

        obj.serialize(&mut serializer)?;

        Ok(buf)
    }
}

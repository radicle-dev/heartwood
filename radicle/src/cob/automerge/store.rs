//! Generic COB storage.
#![allow(clippy::large_enum_variant)]
use std::marker::PhantomData;
use std::ops::ControlFlow;

use automerge::{Automerge, AutomergeError};

use crate::cob;
use crate::cob::automerge::doc::DocumentError;
use crate::cob::automerge::shared::FromHistory;
use crate::cob::automerge::transaction::TransactionError;
use crate::cob::automerge::{label, patch};
use crate::cob::common::Author;
use crate::cob::CollaborativeObject;
use crate::cob::{Contents, Create, HistoryType, ObjectId, TypeName, Update};
use crate::crypto::PublicKey;
use crate::git;
use crate::identity::project;
use crate::prelude::*;
use crate::storage::git as storage;

/// Store error.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("create error: {0}")]
    Create(#[from] cob::error::Create),
    #[error("update error: {0}")]
    Update(#[from] cob::error::Update),
    #[error("retrieve error: {0}")]
    Retrieve(#[from] cob::error::Retrieve),
    #[error(transparent)]
    Automerge(#[from] AutomergeError),
    #[error(transparent)]
    Transaction(#[from] TransactionError),
    #[error(transparent)]
    Identity(#[from] project::IdentityError),
    #[error(transparent)]
    Document(#[from] DocumentError),
    #[error("object `{1}`of type `{0}` was not found")]
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

impl<'a> Store<'a, ()> {
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

    /// Return a patch store from this generic store.
    pub fn patches(&self) -> patch::PatchStore<'_> {
        patch::PatchStore::new(Store {
            whoami: self.whoami,
            project: self.project.clone(),
            raw: self.raw,
            witness: PhantomData,
        })
    }

    /// Return a labels store from this generic store.
    pub fn labels(&self) -> label::LabelStore<'_> {
        label::LabelStore::new(Store {
            whoami: self.whoami,
            project: self.project.clone(),
            raw: self.raw,
            witness: PhantomData,
        })
    }
}

impl<'a, T> Store<'a, T> {
    /// Get this store's author.
    pub fn author(&self) -> Author {
        Author::new(self.whoami)
    }

    /// Get the public key associated with this store.
    pub fn public_key(&self) -> &PublicKey {
        &self.whoami
    }
}

impl<'a, T: FromHistory> Store<'a, T> {
    /// Update an object.
    pub fn update<G: Signer>(
        &self,
        object_id: ObjectId,
        message: &'static str,
        changes: Contents,
        signer: &G,
    ) -> Result<CollaborativeObject, cob::error::Update> {
        cob::update(
            self.raw,
            signer,
            &self.project,
            Update {
                author: Some(cob::Author::from(*signer.public_key())),
                object_id,
                history_type: HistoryType::Automerge,
                typename: T::type_name().clone(),
                message: message.to_owned(),
                changes,
            },
        )
    }

    /// Create an object.
    pub fn create<G: Signer>(
        &self,
        message: &'static str,
        contents: Contents,
        signer: &G,
    ) -> Result<CollaborativeObject, cob::error::Create> {
        cob::create(
            self.raw,
            signer,
            &self.project,
            Create {
                author: Some(cob::Author::from(*signer.public_key())),
                history_type: HistoryType::Automerge,
                typename: T::type_name().clone(),
                message: message.to_owned(),
                contents,
            },
        )
    }

    /// Get an object.
    pub fn get(&self, id: &ObjectId) -> Result<Option<T>, Error> {
        let cob = cob::get(self.raw, T::type_name(), id)?;

        if let Some(cob) = cob {
            let history = cob.history();
            let obj = T::from_history(history)?;

            Ok(Some(obj))
        } else {
            Ok(None)
        }
    }

    /// Get an object as a raw automerge document.
    pub fn get_raw(&self, id: &ObjectId) -> Result<Automerge, Error> {
        let Some(cob) = cob::get(self.raw, T::type_name(), id)? else {
            return Err(Error::NotFound(T::type_name().clone(), *id));
        };

        let doc = cob.history().traverse(Vec::new(), |mut doc, entry| {
            doc.extend(entry.contents());
            ControlFlow::Continue(doc)
        });

        let doc = Automerge::load(&doc)?;

        Ok(doc)
    }

    /// List objects.
    pub fn list(&self) -> Result<Vec<(ObjectId, T)>, Error> {
        let raw = cob::list(self.raw, T::type_name())?;

        raw.into_iter()
            .map(|o| {
                let obj = T::from_history(o.history())?;
                Ok::<_, Error>((*o.id(), obj))
            })
            .collect()
    }
}

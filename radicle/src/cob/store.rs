//! Generic COB storage.
#![allow(clippy::large_enum_variant)]
#![allow(clippy::type_complexity)]
use std::marker::PhantomData;
use std::ops::ControlFlow;

use nonempty::NonEmpty;
use radicle_crdt::Lamport;
use serde::{Deserialize, Serialize};

use crate::cob;
use crate::cob::common::Author;
use crate::cob::op::{Op, OpId, Ops};
use crate::cob::CollaborativeObject;
use crate::cob::{ActorId, Create, History, ObjectId, TypeName, Update};
use crate::crypto::PublicKey;
use crate::git;
use crate::identity;
use crate::identity::Identity;
use crate::prelude::*;
use crate::storage::git as storage;

/// History type for standard radicle COBs.
pub const HISTORY_TYPE: &str = "radicle";

/// A type that can be materialized from an event history.
/// All collaborative objects implement this trait.
pub trait FromHistory: Sized + Default {
    /// The underlying action composing each operation.
    type Action: for<'de> Deserialize<'de>;
    /// Error returned by `apply` function.
    type Error: std::error::Error;

    /// The object type name.
    fn type_name() -> &'static TypeName;

    /// Apply a list of operations to the state.
    fn apply(&mut self, ops: impl IntoIterator<Item = Op<Self::Action>>)
        -> Result<(), Self::Error>;

    /// Create an object from a history.
    fn from_history(history: &History) -> Result<(Self, Lamport), Error> {
        let obj = history.traverse(Self::default(), |mut acc, entry| {
            if let Ok(Ops(ops)) = Ops::try_from(entry) {
                if let Err(err) = acc.apply(ops) {
                    log::warn!("Error applying op to `{}` state: {err}", Self::type_name());
                    return ControlFlow::Break(acc);
                }
            } else {
                return ControlFlow::Break(acc);
            }
            ControlFlow::Continue(acc)
        });

        Ok((obj, history.clock().into()))
    }

    /// Create an object from individual operations.
    /// Returns an error if any of the operations fails to apply.
    fn from_ops(ops: impl IntoIterator<Item = Op<Self::Action>>) -> Result<Self, Self::Error> {
        let mut state = Self::default();
        state.apply(ops)?;

        Ok(state)
    }
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
    Identity(#[from] identity::IdentityError),
    #[error(transparent)]
    Serialize(#[from] serde_json::Error),
    #[error("unexpected history type '{0}'")]
    HistoryType(String),
    #[error("object `{1}` of type `{0}` was not found")]
    NotFound(TypeName, ObjectId),
}

/// Storage for collaborative objects of a specific type `T` in a single repository.
pub struct Store<'a, T> {
    whoami: PublicKey,
    identity: Identity<git::Oid>,
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
        let identity = Identity::load(&whoami, store)?;

        Ok(Self {
            identity,
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
        message: &str,
        actions: impl Into<NonEmpty<T::Action>>,
        signer: &G,
    ) -> Result<CollaborativeObject, Error> {
        let changes = actions.into().try_map(|e| encoding::encode(&e))?;

        cob::update(
            self.raw,
            signer,
            &self.identity,
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
        message: &str,
        actions: impl Into<NonEmpty<T::Action>>,
        signer: &G,
    ) -> Result<(ObjectId, T, Lamport), Error> {
        let contents = actions.into().try_map(|e| encoding::encode(&e))?;
        let cob = cob::create(
            self.raw,
            signer,
            &self.identity,
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
                return Err(Error::HistoryType(cob.manifest().history_type.clone()));
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

/// Allows operations to be batched atomically.
#[derive(Debug)]
pub struct Transaction<T: FromHistory> {
    actor: ActorId,
    start: Lamport,
    clock: Lamport,
    actions: Vec<T::Action>,
}

impl<T: FromHistory> Transaction<T> {
    /// Create a new transaction.
    pub fn new(actor: ActorId, clock: Lamport) -> Self {
        let start = clock;

        Self {
            actor,
            start,
            clock,
            actions: Vec::new(),
        }
    }

    /// Create a new transaction to be used as the initial set of operations for a COB.
    pub fn initial<G, F>(
        message: &str,
        store: &mut Store<T>,
        signer: &G,
        operations: F,
    ) -> Result<(ObjectId, T, Lamport), Error>
    where
        G: Signer,
        F: FnOnce(&mut Self),
        T::Action: Serialize + Clone,
    {
        let actor = *signer.public_key();
        let mut tx = Transaction {
            actor,
            start: Lamport::initial(),
            clock: Lamport::initial(),
            actions: Vec::new(),
        };
        operations(&mut tx);

        let actions = NonEmpty::from_vec(tx.actions)
            .expect("Transaction::initial: transaction must contain at least one operation");
        let (id, cob, clock) = store.create(message, actions, signer)?;

        // The history clock should be in sync with the tx clock.
        assert_eq!(clock, tx.clock);

        Ok((id, cob, clock))
    }

    /// Add an operation to this transaction.
    pub fn push(&mut self, action: T::Action) -> cob::OpId {
        self.actions.push(action);
        OpId::new(self.clock.tick(), self.actor)
    }

    /// Commit transaction.
    ///
    /// Returns a list of operations that can be applied onto an in-memory CRDT.
    pub fn commit<G: Signer>(
        self,
        msg: &str,
        id: ObjectId,
        store: &mut Store<T>,
        signer: &G,
    ) -> Result<(Vec<cob::Op<T::Action>>, Lamport), Error>
    where
        T::Action: Serialize + Clone,
    {
        let actions = NonEmpty::from_vec(self.actions)
            .expect("Transaction::commit: transaction must not be empty");
        let cob = store.update(id, msg, actions.clone(), signer)?;
        let author = self.actor;
        let timestamp = cob.history().timestamp().into();

        // The history clock should be in sync with the tx clock.
        assert_eq!(cob.history().clock(), self.clock.get());

        // Start the clock from where the transcation clock started.
        let mut clock = self.start;
        let ops = actions
            .into_iter()
            .map(|action| cob::Op {
                action,
                author,
                clock: clock.tick(),
                timestamp,
            })
            .collect();

        Ok((ops, clock))
    }
}

pub mod encoding {
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

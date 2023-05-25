//! Generic COB storage.
#![allow(clippy::large_enum_variant)]
#![allow(clippy::type_complexity)]
use std::marker::PhantomData;
use std::ops::ControlFlow;
use std::sync::Arc;

use nonempty::NonEmpty;
use radicle_crdt::Lamport;
use serde::{Deserialize, Serialize};

use crate::cob::op::{Op, Ops};
use crate::cob::{ActorId, Create, EntryId, History, ObjectId, TypeName, Update, Updated};
use crate::git;
use crate::prelude::*;
use crate::storage::git as storage;
use crate::{cob, identity};

/// History type for standard radicle COBs.
pub const HISTORY_TYPE: &str = "radicle";

pub trait HistoryAction {
    /// Parent objects this action depends on. For example, patch revisions
    /// have the commit objects as their parent.
    fn parents(&self) -> Vec<git::Oid> {
        Vec::new()
    }
}

/// A type that can be materialized from an event history.
/// All collaborative objects implement this trait.
pub trait FromHistory: Sized + Default + PartialEq {
    /// The underlying action composing each operation.
    type Action: HistoryAction + for<'de> Deserialize<'de> + Serialize;
    /// Error returned by `apply` function.
    type Error: std::error::Error + Send + Sync + 'static;

    /// The object type name.
    fn type_name() -> &'static TypeName;

    /// Apply a list of operations to the state.
    fn apply<R: ReadRepository>(
        &mut self,
        ops: impl IntoIterator<Item = Op<Self::Action>>,
        repo: &R,
    ) -> Result<(), Self::Error>;

    /// Validate the object. Returns an error if the object is invalid.
    fn validate(&self) -> Result<(), Self::Error>;

    /// Create an object from a history.
    fn from_history<R: ReadRepository>(
        history: &History,
        repo: &R,
    ) -> Result<(Self, Lamport), Self::Error> {
            match Ops::try_from(entry) {
                Ok(Ops(ops)) => {
                    if let Err(err) = acc.apply(ops, repo) {
                        log::warn!("Error applying op to `{}` state: {err}", Self::type_name());
                        return ControlFlow::Break(acc);
                    }
                }
                Err(err) => {
                    log::warn!(
                        "Error decoding ops for `{}` state: {err}",
                        Self::type_name()
                    );
                    return ControlFlow::Break(acc);
                }
            }
            ControlFlow::Continue(acc)
        });

        obj.validate()?;

        Ok((obj, history.clock().into()))
    }

    /// Create an object from individual operations.
    /// Returns an error if any of the operations fails to apply.
    fn from_ops<R: ReadRepository>(
        ops: impl IntoIterator<Item = Op<Self::Action>>,
        repo: &R,
    ) -> Result<Self, Self::Error> {
        let mut state = Self::default();
        state.apply(ops, repo)?;

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
    #[error("apply: {0}")]
    Apply(Arc<dyn std::error::Error + Sync + Send + 'static>),
    #[error("signed refs: {0}")]
    SignRefs(#[from] storage::Error),
    #[error("failed to find reference '{name}': {err}")]
    RefLookup {
        name: git::RefString,
        #[source]
        err: git::Error,
    },
}

impl Error {
    fn apply(e: impl std::error::Error + Sync + Send + 'static) -> Self {
        Self::Apply(Arc::new(e))
    }
}

/// Storage for collaborative objects of a specific type `T` in a single repository.
pub struct Store<'a, T> {
    identity: git::Oid,
    repo: &'a storage::Repository,
    witness: PhantomData<T>,
}

impl<'a, T> AsRef<storage::Repository> for Store<'a, T> {
    fn as_ref(&self) -> &storage::Repository {
        self.repo
    }
}

impl<'a, T> Store<'a, T> {
    /// Open a new generic store.
    pub fn open(repo: &'a storage::Repository) -> Result<Self, Error> {
        let identity = repo.identity()?;

        Ok(Self {
            repo,
            identity: identity.head,
            witness: PhantomData,
        })
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
    ) -> Result<Updated, Error> {
        let actions = actions.into();
        let parents = actions.iter().flat_map(T::Action::parents).collect();
        let changes = actions.try_map(encoding::encode)?;
        let updated = cob::update(
            self.repo,
            signer,
            self.identity,
            parents,
            signer.public_key(),
            Update {
                object_id,
                history_type: HISTORY_TYPE.to_owned(),
                typename: T::type_name().clone(),
                message: message.to_owned(),
                changes,
            },
        )?;

        self.repo.sign_refs(signer).map_err(Error::SignRefs)?;

        Ok(updated)
    }

    /// Create an object.
    pub fn create<G: Signer>(
        &self,
        message: &str,
        actions: impl Into<NonEmpty<T::Action>>,
        signer: &G,
    ) -> Result<(ObjectId, T, Lamport), Error> {
        let actions = actions.into();
        let parents = actions.iter().flat_map(T::Action::parents).collect();
        let contents = actions.try_map(encoding::encode)?;
        let cob = cob::create(
            self.repo,
            signer,
            self.identity,
            parents,
            signer.public_key(),
            Create {
                history_type: HISTORY_TYPE.to_owned(),
                typename: T::type_name().clone(),
                message: message.to_owned(),
                contents,
            },
        )?;
        let (object, clock) = T::from_history(cob.history(), self.repo).map_err(Error::apply)?;

        self.repo.sign_refs(signer).map_err(Error::SignRefs)?;

        Ok((*cob.id(), object, clock))
    }

    /// Get an object.
    pub fn get(&self, id: &ObjectId) -> Result<Option<(T, Lamport)>, Error> {
        let cob = cob::get(self.repo, T::type_name(), id)?;

        if let Some(cob) = cob {
            if cob.manifest().history_type != HISTORY_TYPE {
                return Err(Error::HistoryType(cob.manifest().history_type.clone()));
            }
            let (obj, clock) = T::from_history(cob.history(), self.repo).map_err(Error::apply)?;

            Ok(Some((obj, clock)))
        } else {
            Ok(None)
        }
    }

    /// Return all objects.
    pub fn all(
        &self,
    ) -> Result<impl Iterator<Item = Result<(ObjectId, T, Lamport), Error>> + 'a, Error> {
        let raw = cob::list(self.repo, T::type_name())?;

        Ok(raw.into_iter().map(|o| {
            let (obj, clock) = T::from_history(o.history(), self.repo).map_err(Error::apply)?;
            Ok((*o.id(), obj, clock))
        }))
    }

    /// Return true if the list of issues is empty.
    pub fn is_empty(&self) -> Result<bool, Error> {
        Ok(self.count()? == 0)
    }

    /// Return objects count.
    pub fn count(&self) -> Result<usize, Error> {
        let raw = cob::list(self.repo, T::type_name())?;

        Ok(raw.len())
    }

    /// Remove an object.
    pub fn remove<G: Signer>(&self, id: &ObjectId, signer: &G) -> Result<(), Error> {
        let name = git::refs::storage::cob(signer.public_key(), T::type_name(), id);
        match self
            .repo
            .reference_oid(signer.public_key(), &name.strip_namespace())
        {
            Ok(_) => {
                cob::remove(self.repo, signer.public_key(), T::type_name(), id)?;
                self.repo.sign_refs(signer).map_err(Error::SignRefs)?;
                Ok(())
            }
            Err(git::Error::NotFound(_)) => Ok(()),
            Err(git::Error::Git(err)) if err.code() == git::raw::ErrorCode::NotFound => Ok(()),
            Err(err) => Err(Error::RefLookup {
                name: name.to_ref_string(),
                err,
            }),
        }
    }
}

/// Allows operations to be batched atomically.
#[derive(Debug)]
pub struct Transaction<T: FromHistory> {
    actor: ActorId,
    clock: Lamport,
    actions: Vec<T::Action>,
}

impl<T: FromHistory> Transaction<T> {
    /// Create a new transaction.
    pub fn new(actor: ActorId, clock: Lamport) -> Self {
        Self {
            actor,
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
        F: FnOnce(&mut Self) -> Result<(), Error>,
        T::Action: Serialize + Clone,
    {
        let actor = *signer.public_key();
        let mut tx = Transaction {
            actor,
            // Nb. The clock is never zero.
            clock: Lamport::initial().tick(),
            actions: Vec::new(),
        };
        operations(&mut tx)?;

        let actions = NonEmpty::from_vec(tx.actions)
            .expect("Transaction::initial: transaction must contain at least one operation");
        let (id, cob, clock) = store.create(message, actions, signer)?;

        // The history clock should be in sync with the tx clock.
        assert_eq!(clock, tx.clock);

        Ok((id, cob, clock))
    }

    /// Add an operation to this transaction.
    pub fn push(&mut self, action: T::Action) -> Result<(), Error> {
        self.actions.push(action);

        Ok(())
    }

    /// Commit transaction.
    ///
    /// Returns a list of operations that can be applied onto an in-memory CRDT.
    pub fn commit<G: Signer>(
        mut self,
        msg: &str,
        id: ObjectId,
        store: &mut Store<T>,
        signer: &G,
    ) -> Result<(Vec<cob::Op<T::Action>>, Lamport, EntryId), Error>
    where
        T::Action: Serialize + Clone,
    {
        let actions = NonEmpty::from_vec(self.actions)
            .expect("Transaction::commit: transaction must not be empty");
        let Updated { head, object } = store.update(id, msg, actions.clone(), signer)?;
        let id = EntryId::from(head);
        let author = self.actor;
        let timestamp = object.history().timestamp().into();
        let clock = self.clock.tick();
        let identity = store.identity;

        // The history clock should be in sync with the tx clock.
        assert_eq!(object.history().clock(), self.clock.get());

        // Start the clock from where the transcation clock started.
        let ops = actions
            .into_iter()
            .map(|action| cob::Op {
                id,
                action,
                author,
                clock,
                timestamp,
                identity,
            })
            .collect();

        Ok((ops, clock, id))
    }
}

pub mod encoding {
    use serde::Serialize;

    use crate::canonical::formatter::CanonicalFormatter;

    /// Serialize the change into a byte string.
    pub fn encode<A: Serialize>(action: A) -> Result<Vec<u8>, serde_json::Error> {
        let mut buf = Vec::new();
        let mut serializer =
            serde_json::Serializer::with_formatter(&mut buf, CanonicalFormatter::new());

        action.serialize(&mut serializer)?;

        Ok(buf)
    }
}

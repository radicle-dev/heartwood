//! Generic COB storage.
#![allow(clippy::large_enum_variant)]
#![allow(clippy::type_complexity)]
use std::marker::PhantomData;
use std::ops::ControlFlow;
use std::sync::Arc;

use nonempty::NonEmpty;
use serde::{Deserialize, Serialize};

use crate::cob::common::Timestamp;
use crate::cob::op::Op;
use crate::cob::{ActorId, Create, EntryId, History, ObjectId, TypeName, Update, Updated, Version};
use crate::git;
use crate::prelude::*;
use crate::storage::git as storage;
use crate::storage::SignRepository;
use crate::{cob, identity};

pub trait HistoryAction: std::fmt::Debug {
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
        op: Op<Self::Action>,
        repo: &R,
    ) -> Result<(), Self::Error>;

    /// Validate the object. Returns an error if the object is invalid.
    fn validate(&self) -> Result<(), Self::Error>;

    /// Create an object from a history.
    fn from_history<R: ReadRepository>(history: &History, repo: &R) -> Result<Self, Self::Error> {
        self::from_history::<R, Self>(history, repo)
    }

    /// Create an object from individual operations.
    /// Returns an error if any of the operations fails to apply.
    fn from_ops<R: ReadRepository>(
        ops: impl IntoIterator<Item = Op<Self::Action>>,
        repo: &R,
    ) -> Result<Self, Self::Error> {
        let mut state = Self::default();
        for op in ops {
            state.apply(op, repo)?;
        }
        Ok(state)
    }
}

/// Turn a history into a concrete type, by traversing the history and applying each operation
/// to the state, skipping branches that return errors.
pub fn from_history<R: ReadRepository, T: FromHistory>(
    history: &History,
    repo: &R,
) -> Result<T, T::Error> {
    let obj = history.traverse(T::default(), |mut acc, _, entry| {
        match Op::try_from(entry) {
            Ok(op) => {
                if let Err(err) = acc.apply(op, repo) {
                    log::warn!("Error applying op to `{}` state: {err}", T::type_name());
                    return ControlFlow::Break(acc);
                }
            }
            Err(err) => {
                log::warn!("Error decoding ops for `{}` state: {err}", T::type_name());
                return ControlFlow::Break(acc);
            }
        }
        ControlFlow::Continue(acc)
    });

    obj.validate()?;

    Ok(obj)
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
pub struct Store<'a, T, R> {
    identity: git::Oid,
    repo: &'a R,
    witness: PhantomData<T>,
}

impl<'a, T, R> AsRef<R> for Store<'a, T, R> {
    fn as_ref(&self) -> &R {
        self.repo
    }
}

impl<'a, T, R: ReadRepository> Store<'a, T, R> {
    /// Open a new generic store.
    pub fn open(repo: &'a R) -> Result<Self, Error> {
        let identity = repo.identity()?;

        Ok(Self {
            repo,
            identity: identity.head,
            witness: PhantomData,
        })
    }
}

impl<'a, T, R> Store<'a, T, R>
where
    R: ReadRepository + SignRepository + cob::Store,
    T: FromHistory,
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
                type_name: T::type_name().clone(),
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
    ) -> Result<(ObjectId, T), Error> {
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
                type_name: T::type_name().clone(),
                version: Version::default(),
                message: message.to_owned(),
                contents,
            },
        )?;
        let object = T::from_history(cob.history(), self.repo).map_err(Error::apply)?;

        self.repo.sign_refs(signer).map_err(Error::SignRefs)?;

        Ok((*cob.id(), object))
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

impl<'a, T, R> Store<'a, T, R>
where
    R: ReadRepository + cob::Store,
    T: FromHistory,
    T::Action: Serialize,
{
    /// Get an object.
    pub fn get(&self, id: &ObjectId) -> Result<Option<T>, Error> {
        let cob = cob::get(self.repo, T::type_name(), id)?;

        if let Some(cob) = cob {
            let obj = T::from_history(cob.history(), self.repo).map_err(Error::apply)?;

            Ok(Some(obj))
        } else {
            Ok(None)
        }
    }

    /// Return all objects.
    pub fn all(&self) -> Result<impl Iterator<Item = Result<(ObjectId, T), Error>> + 'a, Error> {
        let raw = cob::list(self.repo, T::type_name())?;

        Ok(raw.into_iter().map(|o| {
            let obj = T::from_history(o.history(), self.repo).map_err(Error::apply)?;
            Ok((*o.id(), obj))
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
}

/// Allows operations to be batched atomically.
#[derive(Debug)]
pub struct Transaction<T: FromHistory> {
    actor: ActorId,
    actions: Vec<T::Action>,
}

impl<T: FromHistory> Transaction<T> {
    /// Create a new transaction.
    pub fn new(actor: ActorId) -> Self {
        Self {
            actor,
            actions: Vec::new(),
        }
    }

    /// Create a new transaction to be used as the initial set of operations for a COB.
    pub fn initial<R, G, F>(
        message: &str,
        store: &mut Store<T, R>,
        signer: &G,
        operations: F,
    ) -> Result<(ObjectId, T), Error>
    where
        G: Signer,
        F: FnOnce(&mut Self) -> Result<(), Error>,
        R: ReadRepository + SignRepository + cob::Store,
        T::Action: Serialize + Clone,
    {
        let actor = *signer.public_key();
        let mut tx = Transaction {
            actor,
            actions: Vec::new(),
        };
        operations(&mut tx)?;

        let actions = NonEmpty::from_vec(tx.actions)
            .expect("Transaction::initial: transaction must contain at least one operation");
        let (id, cob) = store.create(message, actions, signer)?;

        Ok((id, cob))
    }

    /// Add an operation to this transaction.
    pub fn push(&mut self, action: T::Action) -> Result<(), Error> {
        self.actions.push(action);

        Ok(())
    }

    /// Commit transaction.
    ///
    /// Returns an operation that can be applied onto an in-memory state.
    pub fn commit<R, G: Signer>(
        self,
        msg: &str,
        id: ObjectId,
        store: &mut Store<T, R>,
        signer: &G,
    ) -> Result<(cob::Op<T::Action>, EntryId), Error>
    where
        R: ReadRepository + SignRepository + cob::Store,
        T::Action: Serialize + Clone,
    {
        let actions = NonEmpty::from_vec(self.actions)
            .expect("Transaction::commit: transaction must not be empty");
        let Updated {
            head,
            object,
            parents,
        } = store.update(id, msg, actions.clone(), signer)?;
        let id = EntryId::from(head);
        let author = self.actor;
        let timestamp = Timestamp::from_secs(object.history().timestamp());
        let identity = store.identity;
        let manifest = object.manifest().clone();
        let op = cob::Op {
            id,
            actions,
            author,
            timestamp,
            parents,
            identity,
            manifest,
        };

        Ok((op, id))
    }
}

/// Get an object's operations without decoding them.
pub fn ops<R: cob::Store>(
    id: &ObjectId,
    type_name: &TypeName,
    repo: &R,
) -> Result<Vec<Op<Vec<u8>>>, Error> {
    let cob = cob::get(repo, type_name, id)?;

    if let Some(cob) = cob {
        let ops = cob.history().traverse(Vec::new(), |mut ops, _, entry| {
            ops.push(Op::from(entry.clone()));
            ControlFlow::Continue(ops)
        });
        Ok(ops)
    } else {
        Err(Error::NotFound(type_name.clone(), *id))
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

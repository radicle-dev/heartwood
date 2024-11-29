//! Generic COB storage.
#![allow(clippy::large_enum_variant)]
#![allow(clippy::type_complexity)]
use std::fmt::Debug;
use std::marker::PhantomData;

use nonempty::NonEmpty;
use radicle_cob::CollaborativeObject;
use serde::{Deserialize, Serialize};

use crate::cob::op::Op;
use crate::cob::{Create, Embed, EntryId, ObjectId, TypeName, Update, Updated, Uri, Version};
use crate::git;
use crate::node::Login;
use crate::node::NodeSigner;
use crate::prelude::*;
use crate::storage::git as storage;
use crate::storage::SignRepository;
use crate::{cob, identity};

pub trait CobAction: Debug {
    /// Parent objects this action depends on. For example, patch revisions
    /// have the commit objects as their parent.
    fn parents(&self) -> Vec<git::Oid> {
        Vec::new()
    }
}

/// A collaborative object. Can be materialized from an operation history.
pub trait Cob: Sized + PartialEq + Debug {
    /// The underlying action composing each operation.
    type Action: CobAction + for<'de> Deserialize<'de> + Serialize;
    /// Error returned by `apply` function.
    type Error: std::error::Error + Send + Sync + 'static;

    /// The object type name.
    fn type_name() -> &'static TypeName;

    /// Initialize a collarorative object from a root operation.
    fn from_root<R: ReadRepository>(op: Op<Self::Action>, repo: &R) -> Result<Self, Self::Error>;

    /// Apply an operation to the state.
    fn op<'a, R: ReadRepository, I: IntoIterator<Item = &'a cob::Entry>>(
        &mut self,
        op: Op<Self::Action>,
        concurrent: I,
        repo: &R,
    ) -> Result<(), <Self as Cob>::Error>;

    #[cfg(test)]
    /// Create an object from a history.
    fn from_history<R: ReadRepository>(
        history: &crate::cob::History,
        repo: &R,
    ) -> Result<Self, test::HistoryError<Self>> {
        test::from_history::<R, Self>(history, repo)
    }

    #[cfg(test)]
    /// Create an object from individual operations.
    /// Returns an error if any of the operations fails to apply.
    fn from_ops<R: ReadRepository>(
        ops: impl IntoIterator<Item = Op<Self::Action>>,
        repo: &R,
    ) -> Result<Self, Self::Error> {
        let mut ops = ops.into_iter();
        let Some(init) = ops.next() else {
            panic!("FromHistory::from_ops: operations list is empty");
        };
        let mut state = Self::from_root(init, repo)?;
        for op in ops {
            state.op(op, [].into_iter(), repo)?;
        }
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
    Identity(#[from] identity::doc::DocError),
    #[error(transparent)]
    Serialize(#[from] serde_json::Error),
    #[error("object `{1}` of type `{0}` was not found")]
    NotFound(TypeName, ObjectId),
    #[error("signed refs: {0}")]
    SignRefs(Box<storage::RepositoryError>),
    #[error("invalid or unknown embed URI: {0}")]
    EmbedUri(Uri),
    #[error(transparent)]
    Git(git::raw::Error),
    #[error("failed to find reference '{name}': {err}")]
    RefLookup {
        name: git::RefString,
        #[source]
        err: git::raw::Error,
    },
}

/// Storage for collaborative objects of a specific type `T` in a single repository.
pub struct Store<'a, T, R> {
    identity: Option<git::Oid>,
    repo: &'a R,
    witness: PhantomData<T>,
}

impl<'a, T, R> AsRef<R> for Store<'a, T, R> {
    fn as_ref(&self) -> &R {
        self.repo
    }
}

impl<'a, T, R> Store<'a, T, R>
where
    R: ReadRepository + cob::Store,
{
    /// Open a new generic store.
    pub fn open(repo: &'a R) -> Result<Self, Error> {
        Ok(Self {
            repo,
            identity: None,
            witness: PhantomData,
        })
    }

    /// Return a new store with the attached identity.
    pub fn identity(self, identity: git::Oid) -> Self {
        Self {
            repo: self.repo,
            witness: self.witness,
            identity: Some(identity),
        }
    }
}

impl<'a, T, R> Store<'a, T, R>
where
    R: ReadRepository + SignRepository + cob::Store,
    T: Cob + cob::Evaluate<R>,
    T::Action: Serialize,
{
    /// Update an object.
    pub fn update<L: Login>(
        &self,
        object_id: ObjectId,
        message: &str,
        actions: impl Into<NonEmpty<T::Action>>,
        embeds: Vec<Embed<Uri>>,
        login: &L,
    ) -> Result<Updated<T>, Error> {
        let actions = actions.into();
        let related = actions.iter().flat_map(T::Action::parents).collect();
        let changes = actions.try_map(encoding::encode)?;
        let embeds = embeds
            .into_iter()
            .map(|e| {
                Ok::<_, Error>(Embed {
                    content: git::Oid::try_from(&e.content).map_err(Error::EmbedUri)?,
                    name: e.name.clone(),
                })
            })
            .collect::<Result<_, _>>()?;
        let updated = cob::update(
            self.repo,
            login.agent(),
            self.identity,
            related,
            login.node().public_key(),
            Update {
                object_id,
                type_name: T::type_name().clone(),
                message: message.to_owned(),
                embeds,
                changes,
            },
        )?;
        self.repo
            .sign_refs(login.node())
            .map_err(|e| Error::SignRefs(Box::new(e)))?;

        Ok(updated)
    }

    /// Create an object.
    pub fn create<L: Login>(
        &self,
        message: &str,
        actions: impl Into<NonEmpty<T::Action>>,
        embeds: Vec<Embed<Uri>>,
        login: &L,
    ) -> Result<(ObjectId, T), Error> {
        let actions = actions.into();
        let parents = actions.iter().flat_map(T::Action::parents).collect();
        let contents = actions.try_map(encoding::encode)?;
        let embeds = embeds
            .into_iter()
            .map(|e| {
                Ok::<_, Error>(Embed {
                    content: git::Oid::try_from(&e.content).map_err(Error::EmbedUri)?,
                    name: e.name.clone(),
                })
            })
            .collect::<Result<_, _>>()?;
        let cob = cob::create::<T, _, _>(
            self.repo,
            login.agent(),
            self.identity,
            parents,
            login.node().public_key(),
            Create {
                type_name: T::type_name().clone(),
                version: Version::default(),
                message: message.to_owned(),
                embeds,
                contents,
            },
        )?;
        // Nb. We can't sign our refs before the identity refs exist, which are created after
        // the identity COB is created. Therefore we manually sign refs when creating identity
        // COBs.
        if T::type_name() != &*crate::cob::identity::TYPENAME {
            self.repo
                .sign_refs(login.node())
                .map_err(|e| Error::SignRefs(Box::new(e)))?;
        }
        Ok((*cob.id(), cob.object))
    }

    /// Remove an object.
    pub fn remove<G: NodeSigner>(&self, id: &ObjectId, signer: &G) -> Result<(), Error> {
        let name = git::refs::storage::cob(signer.public_key(), T::type_name(), id);
        match self
            .repo
            .reference_oid(signer.public_key(), &name.strip_namespace())
        {
            Ok(_) => {
                cob::remove(self.repo, signer.public_key(), T::type_name(), id)?;
                self.repo
                    .sign_refs(signer)
                    .map_err(|e| Error::SignRefs(Box::new(e)))?;
                Ok(())
            }
            Err(err) if err.code() == git::raw::ErrorCode::NotFound => Ok(()),
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
    T: cob::Evaluate<R> + Cob,
    T::Action: Serialize,
{
    /// Get an object.
    pub fn get(&self, id: &ObjectId) -> Result<Option<T>, Error> {
        cob::get::<T, _>(self.repo, T::type_name(), id)
            .map(|r| r.map(|cob| cob.object))
            .map_err(Error::from)
    }

    /// Return all objects.
    pub fn all(
        &self,
    ) -> Result<impl ExactSizeIterator<Item = Result<(ObjectId, T), Error>> + 'a, Error> {
        let raw = cob::list::<T, _>(self.repo, T::type_name())?;

        Ok(raw.into_iter().map(|o| Ok((*o.id(), o.object))))
    }

    /// Return true if the list of issues is empty.
    pub fn is_empty(&self) -> Result<bool, Error> {
        Ok(self.count()? == 0)
    }

    /// Return objects count.
    pub fn count(&self) -> Result<usize, Error> {
        let raw = cob::list::<T, _>(self.repo, T::type_name())?;

        Ok(raw.len())
    }
}

/// Allows operations to be batched atomically.
#[derive(Debug)]
pub struct Transaction<T: Cob + cob::Evaluate<R>, R> {
    actions: Vec<T::Action>,
    embeds: Vec<Embed<Uri>>,
    repo: PhantomData<R>,
}

impl<T: Cob + cob::Evaluate<R>, R> Default for Transaction<T, R> {
    fn default() -> Self {
        Self {
            actions: Vec::new(),
            embeds: Vec::new(),
            repo: PhantomData,
        }
    }
}

impl<T, R> Transaction<T, R>
where
    T: Cob + cob::Evaluate<R>,
{
    /// Create a new transaction to be used as the initial set of operations for a COB.
    pub fn initial<L, F>(
        message: &str,
        store: &mut Store<T, R>,
        login: &L,
        operations: F,
    ) -> Result<(ObjectId, T), Error>
    where
        L: Login,
        F: FnOnce(&mut Self, &R) -> Result<(), Error>,
        R: ReadRepository + SignRepository + cob::Store,
        T::Action: Serialize + Clone,
    {
        let mut tx = Transaction::default();
        operations(&mut tx, store.as_ref())?;

        let actions = NonEmpty::from_vec(tx.actions)
            .expect("Transaction::initial: transaction must contain at least one action");

        store.create(message, actions, tx.embeds, login)
    }

    /// Add an operation to this transaction.
    pub fn push(&mut self, action: T::Action) -> Result<(), Error> {
        self.actions.push(action);

        Ok(())
    }

    /// Embed media into the transaction.
    pub fn embed(&mut self, embeds: impl IntoIterator<Item = Embed<Uri>>) -> Result<(), Error> {
        self.embeds.extend(embeds);

        Ok(())
    }

    /// Commit transaction.
    ///
    /// Returns an operation that can be applied onto an in-memory state.
    pub fn commit<L: Login>(
        self,
        msg: &str,
        id: ObjectId,
        store: &mut Store<T, R>,
        login: &L,
    ) -> Result<(T, EntryId), Error>
    where
        R: ReadRepository + SignRepository + cob::Store,
        T::Action: Serialize + Clone,
    {
        let actions = NonEmpty::from_vec(self.actions)
            .expect("Transaction::commit: transaction must not be empty");
        let Updated {
            head,
            object: CollaborativeObject { object, .. },
            ..
        } = store.update(id, msg, actions, self.embeds, login)?;

        Ok((object, head))
    }
}

/// Get an object's operations without decoding them.
pub fn ops<R: cob::Store>(
    id: &ObjectId,
    type_name: &TypeName,
    repo: &R,
) -> Result<NonEmpty<Op<Vec<u8>>>, Error> {
    let cob = cob::get::<NonEmpty<cob::Entry>, _>(repo, type_name, id)?;

    if let Some(cob) = cob {
        Ok(cob.object.map(Op::from))
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

#[cfg(test)]
pub mod test {
    use super::*;

    #[derive(Debug, thiserror::Error)]
    pub enum HistoryError<T: Cob> {
        #[error("apply: {0}")]
        Apply(T::Error),
        #[error("operation decoding failed: {0}")]
        Op(#[from] cob::op::OpEncodingError),
    }

    /// Turn a history into a concrete type, by traversing the history and applying each operation
    /// to the state, skipping branches that return errors.
    pub fn from_history<R: ReadRepository, T: Cob>(
        history: &crate::cob::History,
        repo: &R,
    ) -> Result<T, HistoryError<T>> {
        use std::ops::ControlFlow;

        let root = history.root();
        let children = history.children_of(root.id());
        let op = Op::try_from(root)?;
        let initial = T::from_root(op, repo).map_err(HistoryError::Apply)?;
        let obj = history.traverse(initial, &children, |mut acc, _, entry| {
            match Op::try_from(entry) {
                Ok(op) => {
                    if let Err(err) = acc.op(op, [], repo) {
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

        Ok(obj)
    }
}

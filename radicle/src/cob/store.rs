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
use crate::node::device::Device;
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

    /// The outcome of some actions are to be referred later.
    /// For example, one action may create a comment, followed by another
    /// action that may create a reply to the comment, referring to it.
    /// Since actions are stored as part of [`crate::cob::op::Op`],
    /// and operations are the smallest identifiable units,
    /// this may lead to ambiguity.
    /// It would not be possible to to, say, address one particular comment out
    /// of two, if the corresponding actions of creations were part of the
    /// same operation.
    /// To help avoid this, implementations signal whether specific actions
    /// require "their own" identifier.
    /// This allows checking for multiple such actions before creating an
    /// operation.
    fn produces_identifier(&self) -> bool {
        false
    }
}

/// A collaborative object. Can be materialized from an operation history.
pub trait Cob: Sized + PartialEq + Debug {
    /// The underlying action composing each operation.
    type Action: CobAction + for<'de> Deserialize<'de> + Serialize;
    /// Error returned by `apply` function.
    type Error: std::error::Error + Send + Sync + 'static;

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
    ) -> Result<Self, test::HistoryError<Self>>
    where
        Self: CobWithType,
    {
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

/// Implementations are statically associated with a particular
/// type name of a collaborative object.
///
/// In most cases, this trait should be used in tandem with [`Cob`].
pub trait CobWithType {
    /// The type name of the collaborative object type which backs this implementation.
    fn type_name() -> &'static TypeName;
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
    #[error("transaction already contains action {0} which produces an identifier, denying to add action {1} which also produces an identifier")]
    ClashingIdentifiers(String, String),
}

/// Storage for collaborative objects of a specific type `T` in a single repository.
pub struct Store<'a, T, R> {
    identity: Option<git::Oid>,
    repo: &'a R,
    witness: PhantomData<T>,
    type_name: &'a TypeName,
}

impl<T, R> AsRef<R> for Store<'_, T, R> {
    fn as_ref(&self) -> &R {
        self.repo
    }
}

impl<'a, T, R> Store<'a, T, R>
where
    R: ReadRepository + cob::Store,
{
    /// Open a new generic store.
    pub fn open_for(type_name: &'a TypeName, repo: &'a R) -> Result<Self, Error> {
        Ok(Self {
            repo,
            identity: None,
            witness: PhantomData,
            type_name,
        })
    }

    /// Return a new store with the attached identity.
    pub fn identity(self, identity: git::Oid) -> Self {
        Self {
            repo: self.repo,
            witness: self.witness,
            identity: Some(identity),
            type_name: self.type_name,
        }
    }
}

impl<'a, T, R> Store<'a, T, R>
where
    R: ReadRepository + cob::Store,
    T: CobWithType,
{
    /// Open a new generic store.
    pub fn open(repo: &'a R) -> Result<Self, Error> {
        Ok(Self {
            repo,
            identity: None,
            witness: PhantomData,
            type_name: T::type_name(),
        })
    }
}

impl<T, R> Store<'_, T, R>
where
    R: ReadRepository + cob::Store,
    T: Cob + cob::Evaluate<R>,
{
    pub fn transaction(
        &self,
        actions: Vec<T::Action>,
        embeds: Vec<Embed<Uri>>,
    ) -> Transaction<T, R> {
        Transaction::new(self.type_name.clone(), actions, embeds)
    }
}

impl<T, R> Store<'_, T, R>
where
    R: ReadRepository + SignRepository + cob::Store,
    T: Cob + cob::Evaluate<R>,
    T::Action: Serialize,
{
    /// Update an object.
    pub fn update<G>(
        &self,
        type_name: &TypeName,
        object_id: ObjectId,
        message: &str,
        actions: impl Into<NonEmpty<T::Action>>,
        embeds: Vec<Embed<Uri>>,
        signer: &Device<G>,
    ) -> Result<Updated<T>, Error>
    where
        G: crypto::signature::Signer<crypto::Signature>,
    {
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
            signer,
            self.identity,
            related,
            signer.public_key(),
            Update {
                object_id,
                type_name: type_name.clone(),
                message: message.to_owned(),
                embeds,
                changes,
            },
        )?;
        self.repo
            .sign_refs(signer)
            .map_err(|e| Error::SignRefs(Box::new(e)))?;

        Ok(updated)
    }

    /// Create an object.
    pub fn create<G>(
        &self,
        message: &str,
        actions: impl Into<NonEmpty<T::Action>>,
        embeds: Vec<Embed<Uri>>,
        signer: &Device<G>,
    ) -> Result<(ObjectId, T), Error>
    where
        G: crypto::signature::Signer<crypto::Signature>,
    {
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
            signer,
            self.identity,
            parents,
            signer.public_key(),
            Create {
                type_name: self.type_name.clone(),
                version: Version::default(),
                message: message.to_owned(),
                embeds,
                contents,
            },
        )?;
        // Nb. We can't sign our refs before the identity refs exist, which are created after
        // the identity COB is created. Therefore we manually sign refs when creating identity
        // COBs.
        if self.type_name != &*crate::cob::identity::TYPENAME {
            self.repo
                .sign_refs(signer)
                .map_err(|e| Error::SignRefs(Box::new(e)))?;
        }
        Ok((*cob.id(), cob.object))
    }

    /// Remove an object.
    pub fn remove<G>(&self, id: &ObjectId, signer: &Device<G>) -> Result<(), Error>
    where
        G: crypto::signature::Signer<crypto::Signature>,
    {
        let name = git::refs::storage::cob(signer.public_key(), self.type_name, id);
        match self
            .repo
            .reference_oid(signer.public_key(), &name.strip_namespace())
        {
            Ok(_) => {
                cob::remove(self.repo, signer.public_key(), self.type_name, id)?;
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
    T: Cob + cob::Evaluate<R>,
    T::Action: Serialize,
{
    /// Get an object.
    pub fn get(&self, id: &ObjectId) -> Result<Option<T>, Error> {
        cob::get::<T, _>(self.repo, self.type_name, id)
            .map(|r| r.map(|cob| cob.object))
            .map_err(Error::from)
    }

    /// Return all objects.
    pub fn all(
        &self,
    ) -> Result<impl ExactSizeIterator<Item = Result<(ObjectId, T), Error>> + 'a, Error> {
        let raw = cob::list::<T, _>(self.repo, self.type_name)?;

        Ok(raw.into_iter().map(|o| Ok((*o.id(), o.object))))
    }

    /// Return true if the list of issues is empty.
    pub fn is_empty(&self) -> Result<bool, Error> {
        Ok(self.count()? == 0)
    }

    /// Return objects count.
    pub fn count(&self) -> Result<usize, Error> {
        let raw = cob::list::<T, _>(self.repo, self.type_name)?;

        Ok(raw.len())
    }
}

/// Allows operations to be batched atomically.
#[derive(Debug)]
pub struct Transaction<T: Cob + cob::Evaluate<R>, R> {
    actions: Vec<T::Action>,
    embeds: Vec<Embed<Uri>>,

    // Internal state kept for validation of the transaction.
    // If an action that produces an identifier is added to
    // the transaction, then this will track its index,
    // so that adding a second action that produces an identifier
    // can fail with a useful error.
    produces_identifier: Option<usize>,

    repo: PhantomData<R>,
    type_name: TypeName,
}

impl<T: Cob + CobWithType + cob::Evaluate<R>, R> Default for Transaction<T, R> {
    fn default() -> Self {
        Self {
            actions: Vec::new(),
            embeds: Vec::new(),
            produces_identifier: None,
            repo: PhantomData,
            type_name: T::type_name().clone(),
        }
    }
}

impl<T, R> Transaction<T, R>
where
    T: Cob + cob::Evaluate<R>,
{
    pub fn new(type_name: TypeName, actions: Vec<T::Action>, embeds: Vec<Embed<Uri>>) -> Self {
        Self {
            actions,
            embeds,
            produces_identifier: None,
            repo: PhantomData,
            type_name,
        }
    }
}

impl<T, R> Transaction<T, R>
where
    T: Cob + CobWithType + cob::Evaluate<R>,
{
    /// Create a new transaction to be used as the initial set of operations for a COB.
    pub fn initial<G, F, Tx>(
        message: &str,
        store: &mut Store<T, R>,
        signer: &Device<G>,
        operations: F,
    ) -> Result<(ObjectId, T), Error>
    where
        Tx: From<Self>,
        Self: From<Tx>,
        G: crypto::signature::Signer<crypto::Signature>,
        F: FnOnce(&mut Tx, &R) -> Result<(), Error>,
        R: ReadRepository + SignRepository + cob::Store,
        T::Action: Serialize + Clone,
    {
        let mut tx = Tx::from(Transaction::default());
        operations(&mut tx, store.as_ref())?;
        let tx = Self::from(tx);

        let actions = NonEmpty::from_vec(tx.actions)
            .expect("Transaction::initial: transaction must contain at least one action");

        store.create(message, actions, tx.embeds, signer)
    }
}

impl<T, R> Transaction<T, R>
where
    T: Cob + cob::Evaluate<R>,
{
    /// Add an action to this transaction.
    pub fn push(&mut self, action: T::Action) -> Result<(), Error> {
        if action.produces_identifier() {
            if let Some(index) = self.produces_identifier {
                return Err(Error::ClashingIdentifiers(
                    serde_json::to_string(&self.actions[index])?,
                    serde_json::to_string(&action)?,
                ));
            } else {
                self.produces_identifier = Some(self.actions.len())
            }
        }

        self.actions.push(action);

        Ok(())
    }

    /// Add actions to this transaction.
    /// Note that we cannot implement [`std::iter::Extend`] because [`Self::push`]
    /// validates the action being pushed, and therefore is falliable.
    pub fn extend<I: IntoIterator<Item = T::Action>>(&mut self, actions: I) -> Result<(), Error> {
        for action in actions {
            self.push(action)?;
        }
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
    pub fn commit<G>(
        self,
        msg: &str,
        id: ObjectId,
        store: &mut Store<T, R>,
        signer: &Device<G>,
    ) -> Result<(T, EntryId), Error>
    where
        R: ReadRepository + SignRepository + cob::Store,
        T::Action: Serialize + Clone,
        G: crypto::signature::Signer<crypto::Signature>,
    {
        let actions = NonEmpty::from_vec(self.actions)
            .expect("Transaction::commit: transaction must not be empty");
        let Updated {
            head,
            object: CollaborativeObject { object, .. },
            ..
        } = store.update(&self.type_name, id, msg, actions, self.embeds, signer)?;

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
    pub fn from_history<R: ReadRepository, T: Cob + CobWithType>(
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

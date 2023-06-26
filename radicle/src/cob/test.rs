use std::collections::BTreeSet;
use std::marker::PhantomData;
use std::ops::Deref;

use nonempty::NonEmpty;
use serde::Serialize;

use crate::cob::common::clock;
use crate::cob::op::{Op, Ops};
use crate::cob::patch;
use crate::cob::patch::Patch;
use crate::cob::store::encoding;
use crate::cob::{EntryId, History};
use crate::crypto::Signer;
use crate::git;
use crate::git::ext::author::Author;
use crate::git::ext::commit::headers::Headers;
use crate::git::ext::commit::{trailers::OwnedTrailer, Commit};
use crate::git::Oid;
use crate::prelude::Did;
use crate::storage::ReadRepository;
use crate::test::arbitrary;

use super::store::FromHistory;
use super::thread;

/// Convenience type for building histories.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryBuilder<T> {
    history: History,
    resource: Oid,
    witness: PhantomData<T>,
}

impl<T> AsRef<History> for HistoryBuilder<T> {
    fn as_ref(&self) -> &History {
        &self.history
    }
}

impl HistoryBuilder<thread::Thread> {
    pub fn comment<G: Signer>(
        &mut self,
        body: impl ToString,
        reply_to: Option<thread::CommentId>,
        signer: &G,
    ) -> Oid {
        let action = thread::Action::Comment {
            body: body.to_string(),
            reply_to,
        };
        self.commit(&action, signer)
    }
}

impl<T: FromHistory> HistoryBuilder<T>
where
    T::Action: Serialize + Eq + 'static,
{
    pub fn new<G: Signer>(action: &T::Action, signer: &G) -> HistoryBuilder<T> {
        let resource = arbitrary::oid();
        let timestamp = clock::Physical::now().as_secs();
        let (data, root) = encoded::<T, _>(action, timestamp as i64, [], signer);

        Self {
            history: History::new_from_root(
                root,
                *signer.public_key(),
                resource,
                NonEmpty::new(data),
                timestamp,
            ),
            resource,
            witness: PhantomData,
        }
    }

    pub fn root(&self) -> EntryId {
        self.history.root()
    }

    pub fn merge(&mut self, other: Self) {
        self.history.merge(other.history);
    }

    pub fn commit<G: Signer>(&mut self, action: &T::Action, signer: &G) -> git::ext::Oid {
        let timestamp = clock::Physical::now().as_secs();
        let tips = self.tips();
        let (data, oid) = encoded::<T, _>(action, timestamp as i64, tips, signer);

        self.history.extend(
            oid,
            *signer.public_key(),
            self.resource,
            NonEmpty::new(data),
            timestamp,
        );
        oid
    }

    /// Return a sorted list of operations by traversing the history in topological order.
    /// In the case of partial orderings, a random order will be returned, using the provided RNG.
    pub fn sorted(&self, rng: &mut fastrand::Rng) -> Vec<Op<T::Action>> {
        self.history
            .sorted(|a, b| if rng.bool() { a.cmp(b) } else { b.cmp(a) })
            .flat_map(|entry| {
                Ops::try_from(entry).expect("HistoryBuilder::sorted: operations must be valid")
            })
            .collect()
    }

    /// Return `n` permutations of the topological ordering of operations.
    /// *This function will never return if less than `n` permutations exist.*
    pub fn permutations(&self, n: usize) -> impl IntoIterator<Item = Vec<Op<T::Action>>> {
        let mut permutations = BTreeSet::new();
        let mut rng = fastrand::Rng::new();

        while permutations.len() < n {
            permutations.insert(self.sorted(&mut rng));
        }
        permutations.into_iter()
    }
}

impl<A> Deref for HistoryBuilder<A> {
    type Target = History;

    fn deref(&self) -> &Self::Target {
        &self.history
    }
}

/// Create a new test history.
pub fn history<T: FromHistory, G: Signer>(action: &T::Action, signer: &G) -> HistoryBuilder<T>
where
    T::Action: Serialize + Eq + 'static,
{
    HistoryBuilder::new(action, signer)
}

/// An object that can be used to create and sign operations.
pub struct Actor<G> {
    pub signer: G,
    pub clock: clock::Lamport,
}

impl<G: Default> Default for Actor<G> {
    fn default() -> Self {
        Self::new(G::default())
    }
}

impl<G> Actor<G> {
    pub fn new(signer: G) -> Self {
        Self {
            signer,
            clock: clock::Lamport::default(),
        }
    }
}

impl<G: Signer> Actor<G> {
    /// Create a new operation.
    pub fn op_with<A: Clone + Serialize>(
        &mut self,
        action: A,
        clock: clock::Lamport,
        identity: Oid,
    ) -> Op<A> {
        let data = encoding::encode(serde_json::json!({
            "action": action,
            "nonce": fastrand::u64(..),
        }))
        .unwrap();
        let oid = git::raw::Oid::hash_object(git::raw::ObjectType::Blob, &data).unwrap();
        let id = oid.into();
        let author = *self.signer.public_key();
        let timestamp = clock::Physical::now();

        Op {
            id,
            action,
            author,
            clock,
            timestamp,
            identity,
        }
    }

    /// Create a new operation.
    pub fn op<A: Clone + Serialize>(&mut self, action: A) -> Op<A> {
        let clock = self.clock.tick();
        let identity = arbitrary::oid();

        self.op_with(action, clock, identity)
    }

    /// Get the actor's DID.
    pub fn did(&self) -> Did {
        self.signer.public_key().into()
    }
}

impl<G: Signer> Actor<G> {
    /// Create a patch.
    pub fn patch<R: ReadRepository>(
        &mut self,
        title: impl ToString,
        description: impl ToString,
        base: git::Oid,
        oid: git::Oid,
        repo: &R,
    ) -> Result<Patch, patch::Error> {
        Patch::from_ops(
            [
                self.op(patch::Action::Revision {
                    description: description.to_string(),
                    base,
                    oid,
                }),
                self.op(patch::Action::Edit {
                    title: title.to_string(),
                    target: patch::MergeTarget::default(),
                }),
            ],
            repo,
        )
    }
}

/// Encode an action and return its hash.
///
/// Doesn't encode in the same way as we do in production, but attempts to include the same data
/// that feeds into the hash entropy, so that changing any input will change the resulting oid.
pub fn encoded<T: FromHistory, G: Signer>(
    action: &T::Action,
    timestamp: i64,
    parents: impl IntoIterator<Item = Oid>,
    signer: &G,
) -> (Vec<u8>, git::ext::Oid) {
    let data = encoding::encode(action).unwrap();
    let oid = git::raw::Oid::hash_object(git::raw::ObjectType::Blob, &data).unwrap();
    let parents = parents.into_iter().map(|o| *o);
    let author = Author {
        name: "radicle".to_owned(),
        email: signer.public_key().to_human(),
        time: git_ext::author::Time::new(timestamp, 0),
    };
    let commit = Commit::new::<_, _, OwnedTrailer>(
        oid,
        parents,
        author.clone(),
        author,
        Headers::new(),
        String::default(),
        [],
    )
    .to_string();

    let hash = git::raw::Oid::hash_object(git::raw::ObjectType::Commit, commit.as_bytes()).unwrap();

    (data, hash.into())
}

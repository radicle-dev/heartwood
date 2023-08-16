use std::marker::PhantomData;
use std::ops::Deref;

use nonempty::NonEmpty;
use serde::{Deserialize, Serialize};

use crate::cob::op::Op;
use crate::cob::patch;
use crate::cob::patch::Patch;
use crate::cob::store::encoding;
use crate::cob::{Entry, History, Manifest, Timestamp, Version};
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
    time: Timestamp,
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
    T::Action: for<'de> Deserialize<'de> + Serialize + Eq + 'static,
{
    pub fn new<G: Signer>(action: &T::Action, time: Timestamp, signer: &G) -> HistoryBuilder<T> {
        let resource = arbitrary::oid();
        let (data, root) = encoded::<T, _>(action, time, [], signer);
        let manifest = Manifest::new(T::type_name().clone(), Version::default());

        Self {
            history: History::new_from_root(
                root,
                *signer.public_key(),
                resource,
                NonEmpty::new(data),
                time.as_secs(),
                manifest,
            ),
            time,
            resource,
            witness: PhantomData,
        }
    }

    pub fn root(&self) -> &Entry {
        self.history.root()
    }

    pub fn merge(&mut self, other: Self) {
        self.history.merge(other.history);
    }

    pub fn commit<G: Signer>(&mut self, action: &T::Action, signer: &G) -> git::ext::Oid {
        let timestamp = self.time;
        let tips = self.tips();
        let (data, oid) = encoded::<T, _>(action, timestamp, tips, signer);
        let manifest = Manifest::new(T::type_name().clone(), Version::default());

        self.history.extend(
            oid,
            *signer.public_key(),
            self.resource,
            NonEmpty::new(data),
            vec![],
            timestamp.as_secs(),
            manifest,
        );
        oid
    }
}

impl<A> Deref for HistoryBuilder<A> {
    type Target = History;

    fn deref(&self) -> &Self::Target {
        &self.history
    }
}

/// Create a new test history.
pub fn history<T: FromHistory, G: Signer>(
    action: &T::Action,
    time: Timestamp,
    signer: &G,
) -> HistoryBuilder<T>
where
    T::Action: Serialize + Eq + 'static,
{
    HistoryBuilder::new(action, time, signer)
}

/// An object that can be used to create and sign operations.
pub struct Actor<G> {
    pub signer: G,
}

impl<G: Default> Default for Actor<G> {
    fn default() -> Self {
        Self::new(G::default())
    }
}

impl<G> Actor<G> {
    pub fn new(signer: G) -> Self {
        Self { signer }
    }
}

impl<G: Signer> Actor<G> {
    /// Create a new operation.
    pub fn op_with<T: FromHistory>(
        &mut self,
        action: T::Action,
        identity: Oid,
        timestamp: Timestamp,
    ) -> Op<T::Action>
    where
        T::Action: Clone + Serialize,
    {
        let data = encoding::encode(serde_json::json!({
            "action": action,
            "nonce": fastrand::u64(..),
        }))
        .unwrap();
        let oid = git::raw::Oid::hash_object(git::raw::ObjectType::Blob, &data).unwrap();
        let id = oid.into();
        let author = *self.signer.public_key();
        let actions = NonEmpty::new(action);
        let manifest = Manifest::new(T::type_name().clone(), Version::default());
        let parents = vec![];

        Op {
            id,
            actions,
            author,
            parents,
            timestamp,
            identity,
            manifest,
        }
    }

    /// Create a new operation.
    pub fn op<T: FromHistory>(&mut self, action: T::Action) -> Op<T::Action>
    where
        T::Action: Clone + Serialize,
    {
        let identity = arbitrary::oid();
        let timestamp = Timestamp::now();

        self.op_with::<T>(action, identity, timestamp)
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
                self.op::<Patch>(patch::Action::Revision {
                    description: description.to_string(),
                    base,
                    oid,
                    resolves: Default::default(),
                }),
                self.op::<Patch>(patch::Action::Edit {
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
    timestamp: Timestamp,
    parents: impl IntoIterator<Item = Oid>,
    signer: &G,
) -> (Vec<u8>, git::ext::Oid) {
    let data = encoding::encode(action).unwrap();
    let oid = git::raw::Oid::hash_object(git::raw::ObjectType::Blob, &data).unwrap();
    let parents = parents.into_iter().map(|o| *o);
    let author = Author {
        name: "radicle".to_owned(),
        email: signer.public_key().to_human(),
        time: git_ext::author::Time::new(timestamp.as_secs() as i64, 0),
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

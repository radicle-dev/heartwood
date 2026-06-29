use std::marker::PhantomData;
use std::ops::Deref;

use nonempty::NonEmpty;
use serde::{Deserialize, Serialize};

use crate::cob::op::Op;
use crate::cob::store::encoding;
use crate::cob::{Entry, History, Manifest, Timestamp, Version};
use crate::crypto::ExtendedSignature;
use crate::git::{self, Oid};
use crate::prelude::Did;
use crate::profile::env;
use crate::test::arbitrary;

use super::store::{Cob, CobWithType};
use super::thread;

/// Convenience type for building histories.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryBuilder<T> {
    history: History,
    resource: Option<Oid>,
    time: Timestamp,
    witness: PhantomData<T>,
}

impl<T> AsRef<History> for HistoryBuilder<T> {
    fn as_ref(&self) -> &History {
        &self.history
    }
}

impl HistoryBuilder<thread::Thread> {
    pub fn comment(
        &mut self,
        body: impl ToString,
        reply_to: Option<thread::CommentId>,
        signer: &impl crypto::Signer,
    ) -> Oid {
        let action = thread::Action::Comment {
            body: body.to_string(),
            reply_to,
        };
        self.commit(&action, signer)
    }
}

impl<T: Cob> HistoryBuilder<T>
where
    T: CobWithType,
    T::Action: for<'de> Deserialize<'de> + Serialize + Eq + 'static,
{
    pub fn new(
        actions: &[T::Action],
        time: Timestamp,
        signer: &impl crypto::Signer,
    ) -> HistoryBuilder<T> {
        let resource = Some(arbitrary::oid());
        let revision = arbitrary::oid();
        let (contents, oids): (Vec<Vec<u8>>, Vec<Oid>) = actions
            .iter()
            .map(|a| encoded::<T>(a, time, [], signer))
            .unzip();
        let contents = NonEmpty::from_vec(contents).unwrap();
        let root = oids.first().unwrap();
        let manifest = Manifest::new(T::type_name().clone(), Version::default());
        let signature = ExtendedSignature::try_sign(signer, &[0]).unwrap();
        let change = Entry {
            id: *root,
            signature,
            resource,
            contents,
            timestamp: time.as_secs(),
            revision,
            parents: vec![],
            related: vec![],
            manifest,
        };

        Self {
            history: History::new_from_root(change),
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

    pub fn commit(&mut self, action: &T::Action, signer: &impl crypto::Signer) -> crate::git::Oid {
        let timestamp = self.time;
        let tips = self.tips();
        let revision = arbitrary::oid();
        let (data, oid) = encoded::<T>(action, timestamp, tips, signer);
        let manifest = Manifest::new(T::type_name().clone(), Version::default());
        let signature = ExtendedSignature::try_sign(signer, data.as_slice()).unwrap();
        let change = Entry {
            id: oid,
            signature,
            resource: self.resource,
            contents: NonEmpty::new(data),
            timestamp: timestamp.as_secs(),
            revision,
            parents: vec![],
            related: vec![],
            manifest,
        };
        self.history.extend(change);

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
pub fn history<T>(
    actions: &[T::Action],
    time: Timestamp,
    signer: &impl crypto::Signer,
) -> HistoryBuilder<T>
where
    T: Cob + CobWithType,
    T::Action: Serialize + Eq + 'static,
{
    HistoryBuilder::new(actions, time, signer)
}

/// An extension trait that provides convenience methods for creating operations.
pub trait SignerOpExt: crypto::Signer {
    /// Create a new operation.
    fn op_with<T>(
        &mut self,
        actions: impl IntoIterator<Item = T::Action>,
        identity: Option<Oid>,
        timestamp: Timestamp,
    ) -> Op<T::Action>
    where
        T: Cob + CobWithType,
        T::Action: Clone + Serialize,
    {
        let actions = actions.into_iter().collect::<Vec<_>>();
        let data = encoding::encode(serde_json::json!({
            "action": actions,
            "nonce": fastrand::u64(..),
        }))
        .unwrap();
        let oid =
            crate::git::raw::Oid::hash_object(crate::git::raw::ObjectType::Blob, &data).unwrap();
        let id = oid.into();
        let author = self.did().into();
        let actions = NonEmpty::from_vec(actions).unwrap();
        let manifest = Manifest::new(T::type_name().clone(), Version::default());
        let parents = vec![];
        let related = vec![];

        Op {
            id,
            actions,
            author,
            parents,
            related,
            timestamp,
            identity,
            manifest,
        }
    }

    /// Create a new operation.
    fn op<T>(&mut self, actions: impl IntoIterator<Item = T::Action>) -> Op<T::Action>
    where
        T: Cob + CobWithType,
        T::Action: Clone + Serialize,
    {
        let identity = arbitrary::oid();
        let timestamp = env::commit_time();

        self.op_with::<T>(actions, Some(identity), timestamp.into())
    }

    /// Get the [`Did`] corresponding to the verifying key of the signer.
    fn did(&self) -> Did {
        Did::from(self.verifying_key().public_key())
    }
}

impl<Signer> SignerOpExt for Signer where Signer: crypto::Signer {}

/// Encode an action and return its hash.
///
/// Doesn't encode in the same way as we do in production, but attempts to include the same data
/// that feeds into the hash entropy, so that changing any input will change the resulting oid.
fn encoded<T: Cob>(
    action: &T::Action,
    timestamp: Timestamp,
    parents: impl IntoIterator<Item = Oid>,
    signer: &impl crypto::Signer,
) -> (Vec<u8>, crate::git::Oid) {
    use radicle_git_metadata::{
        author::{Author, Time},
        commit::{CommitData, headers::Headers, trailers::OwnedTrailer},
    };

    let data = encoding::encode(action).unwrap();
    let oid = crate::git::raw::Oid::hash_object(crate::git::raw::ObjectType::Blob, &data).unwrap();
    let parents = parents.into_iter().map(|o| o.into());
    let author = Author {
        name: "radicle".to_owned(),
        email: signer.verifying_key().public_key().to_human(),
        time: Time::new(timestamp.as_secs() as i64, 0),
    };
    let commit = CommitData::<git::raw::Oid, git::raw::Oid>::new::<_, _, OwnedTrailer>(
        oid,
        parents,
        author.clone(),
        author,
        Headers::new(),
        String::default(),
        [],
    )
    .to_string();

    let hash =
        crate::git::raw::Oid::hash_object(crate::git::raw::ObjectType::Commit, commit.as_bytes())
            .unwrap();

    (data, hash.into())
}

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::ops::{ControlFlow, Deref, DerefMut};
use std::str::FromStr;

use once_cell::sync::Lazy;
use radicle_crdt as crdt;
use serde::{Deserialize, Serialize};

use crate::cob;
use crate::cob::common::{Reaction, Timestamp};
use crate::cob::store;
use crate::cob::{ActorId, History, Op, OpId, TypeName};
use crate::crypto::Signer;

use crdt::clock::Lamport;
use crdt::gmap::GMap;
use crdt::lwwset::LWWSet;
use crdt::redactable::Redactable;
use crdt::Semilattice;

use super::op::Ops;

/// Type name of a thread.
pub static TYPENAME: Lazy<TypeName> =
    Lazy::new(|| FromStr::from_str("xyz.radicle.thread").expect("type name is valid"));

/// Identifies a comment.
pub type CommentId = OpId;

/// A comment on a discussion thread.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Comment {
    /// The comment body.
    pub body: String,
    /// Comment this is a reply to.
    /// Should always be set, except for the root comment.
    pub reply_to: Option<CommentId>,
    /// When the comment was authored.
    pub timestamp: Timestamp,
}

impl Comment {
    /// Create a new comment.
    pub fn new(body: String, reply_to: Option<CommentId>, timestamp: Timestamp) -> Self {
        Self {
            body,
            reply_to,
            timestamp,
        }
    }
}

impl PartialOrd for Comment {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        if self == other {
            Some(Ordering::Equal)
        } else {
            None
        }
    }
}

/// An action that can be carried out in a change.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Action {
    /// Comment on a thread.
    Comment {
        /// Comment body.
        body: String,
        /// Comment this is a reply to.
        /// Should be [`None`] if it's the top-level comment.
        /// Should be the root [`CommentId`] if it's a top-level comment.
        reply_to: Option<OpId>,
    },
    /// Redact a change. Not all changes can be redacted.
    Redact { id: OpId },
    /// React to a change.
    React {
        to: OpId,
        reaction: Reaction,
        active: bool,
    },
}

impl From<Action> for nonempty::NonEmpty<Action> {
    fn from(action: Action) -> Self {
        Self::new(action)
    }
}

/// A discussion thread.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Thread {
    /// The comments under the thread.
    comments: GMap<CommentId, Redactable<Comment>>,
    /// Reactions to changes.
    reactions: GMap<CommentId, LWWSet<(ActorId, Reaction), Lamport>>,
}

impl store::FromHistory for Thread {
    type Action = Action;

    fn type_name() -> &'static radicle_cob::TypeName {
        &*TYPENAME
    }

    fn from_history(history: &History) -> Result<(Self, Lamport), store::Error> {
        let obj = history.traverse(Thread::default(), |mut acc, entry| {
            if let Ok(Ops(ops)) = Ops::try_from(entry) {
                acc.apply(ops);
                ControlFlow::Continue(acc)
            } else {
                ControlFlow::Break(acc)
            }
        });
        Ok((obj, history.clock().into()))
    }
}

impl Semilattice for Thread {
    fn merge(&mut self, other: Self) {
        self.comments.merge(other.comments);
        self.reactions.merge(other.reactions);
    }
}

impl Deref for Thread {
    type Target = BTreeMap<CommentId, Redactable<Comment>>;

    fn deref(&self) -> &Self::Target {
        &self.comments
    }
}

impl Thread {
    pub fn is_initialized(&self) -> bool {
        !self.comments.is_empty()
    }

    pub fn comment(&self, id: &CommentId) -> Option<&Comment> {
        if let Some(Redactable::Present(comment)) = self.comments.get(id) {
            Some(comment)
        } else {
            None
        }
    }

    pub fn root(&self) -> Option<(&CommentId, &Comment)> {
        self.comments
            .iter()
            .filter_map(|(id, r)| r.get().map(|comment| (id, comment)))
            .next()
    }

    pub fn replies<'a>(
        &'a self,
        to: &'a CommentId,
    ) -> impl Iterator<Item = (&CommentId, &Comment)> {
        self.comments().filter_map(move |(id, c)| {
            if let Some(reply_to) = c.reply_to {
                if &reply_to == to {
                    return Some((id, c));
                }
            }
            None
        })
    }

    pub fn reactions<'a>(
        &'a self,
        to: &'a CommentId,
    ) -> impl Iterator<Item = (&ActorId, &Reaction)> {
        self.reactions
            .get(to)
            .into_iter()
            .flat_map(move |rs| rs.iter())
            .map(|(a, r)| (a, r))
    }

    pub fn apply(&mut self, ops: impl IntoIterator<Item = Op<Action>>) {
        for op in ops.into_iter() {
            let id = op.id();

            match op.action {
                Action::Comment { body, reply_to } => {
                    self.comments.insert(
                        id,
                        Redactable::Present(Comment::new(body, reply_to, op.timestamp)),
                    );
                }
                Action::Redact { id } => {
                    self.comments.insert(id, Redactable::Redacted);
                }
                Action::React {
                    to,
                    reaction,
                    active,
                } => {
                    let key = (op.author, reaction);
                    let reactions = if active {
                        LWWSet::singleton(key, op.clock)
                    } else {
                        let mut set = LWWSet::default();
                        set.remove(key, op.clock);
                        set
                    };
                    self.reactions.insert(to, reactions);
                }
            }
        }
    }

    pub fn comments(&self) -> impl Iterator<Item = (&CommentId, &Comment)> + '_ {
        self.comments.iter().filter_map(|(id, comment)| {
            if let Redactable::Present(c) = comment {
                Some((id, c))
            } else {
                None
            }
        })
    }
}

/// An object that can be used to create and sign changes.
pub struct Actor<G> {
    inner: cob::Actor<G, Action>,
}

impl<G: Default + Signer> Default for Actor<G> {
    fn default() -> Self {
        Self {
            inner: cob::Actor::new(G::default()),
        }
    }
}

impl<G: Signer> Actor<G> {
    pub fn new(signer: G) -> Self {
        Self {
            inner: cob::Actor::new(signer),
        }
    }

    /// Create a new thread.
    pub fn thread(&self) -> Thread {
        Thread::default()
    }

    /// Create a new comment.
    pub fn comment(&mut self, body: &str, reply_to: Option<OpId>) -> Op<Action> {
        self.op(Action::Comment {
            body: String::from(body),
            reply_to,
        })
    }

    /// Create a new redaction.
    pub fn redact(&mut self, id: OpId) -> Op<Action> {
        self.op(Action::Redact { id })
    }
}

impl<G> Deref for Actor<G> {
    type Target = cob::Actor<G, Action>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<G> DerefMut for Actor<G> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

#[cfg(test)]
mod tests {
    use std::{array, iter};

    use crate::crypto::test::signer::MockSigner;
    use nonempty::NonEmpty;
    use pretty_assertions::assert_eq;
    use qcheck::{Arbitrary, TestResult};

    use super::*;
    use crate as radicle;
    use crdt::test::{assert_laws, WeightedGenerator};

    #[derive(Clone)]
    struct Changes<const N: usize> {
        permutations: [Vec<Op<Action>>; N],
    }

    impl<const N: usize> std::fmt::Debug for Changes<N> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            for (i, p) in self.permutations.iter().enumerate() {
                writeln!(
                    f,
                    "{i}: {:#?}",
                    p.iter().map(|c| &c.action).collect::<Vec<_>>()
                )?;
            }
            Ok(())
        }
    }

    impl<const N: usize> Arbitrary for Changes<N> {
        fn arbitrary(g: &mut qcheck::Gen) -> Self {
            let author = ActorId::from([0; 32]);
            let rng = fastrand::Rng::with_seed(u64::arbitrary(g));
            let root = (Lamport::initial(), author);
            let gen =
                WeightedGenerator::<(Lamport, Action), (Lamport, Vec<OpId>)>::new(rng.clone())
                    .variant(3, |(clock, changes), rng| {
                        changes.push((clock.tick(), author));

                        Some((
                            *clock,
                            Action::Comment {
                                body: iter::repeat_with(|| rng.alphabetic()).take(16).collect(),
                                reply_to: Some(root),
                            },
                        ))
                    })
                    .variant(2, |(clock, changes), rng| {
                        if changes.is_empty() {
                            return None;
                        }
                        let to = changes[rng.usize(..changes.len())];

                        Some((
                            clock.tick(),
                            Action::React {
                                to,
                                reaction: Reaction::new('✨').unwrap(),
                                active: rng.bool(),
                            },
                        ))
                    })
                    .variant(2, |(clock, changes), rng| {
                        if changes.is_empty() {
                            return None;
                        }
                        let id = changes[rng.usize(..changes.len())];

                        Some((clock.tick(), Action::Redact { id }))
                    });

            let mut changes = vec![Op {
                action: Action::Comment {
                    body: String::default(),
                    reply_to: None,
                },
                author,
                clock: Lamport::initial(),
                timestamp: Timestamp::now(),
            }];
            let mut permutations: [Vec<Op<Action>>; N] = array::from_fn(|_| Vec::new());

            for (clock, action) in gen.take(g.size()) {
                let timestamp = Timestamp::now() + rng.u64(..60);
                changes.push(Op {
                    action,
                    author,
                    clock,
                    timestamp,
                });
            }

            for p in &mut permutations {
                *p = changes.clone();
                rng.shuffle(&mut changes);
            }

            Changes { permutations }
        }
    }

    #[test]
    fn test_redact_comment() {
        let tmp = tempfile::tempdir().unwrap();
        let (_, signer, repository) = radicle::test::setup::context(&tmp);
        let store =
            radicle::cob::store::Store::<Thread>::open(*signer.public_key(), &repository).unwrap();
        let mut alice = Actor::new(signer);

        let a0 = alice.comment("First comment", None);
        let a1 = alice.comment("Second comment", Some(a0.id()));
        let a2 = alice.comment("Third comment", Some(a0.id()));

        let (id, _, _) = store
            .create("Thread created", a0.action, &alice.signer)
            .unwrap();
        let comment = store
            .update(id, "Thread updated", a1.action, &alice.signer)
            .unwrap();
        store
            .update(id, "Thread updated", a2.action, &alice.signer)
            .unwrap();

        // Redact the second comment.
        let a3 = alice.redact((comment.history().clock().into(), *alice.signer.public_key()));
        store
            .update(id, "Comment redacted", a3.action, &alice.signer)
            .unwrap();

        let (thread, _) = store.get(&id).unwrap().unwrap();
        let (_, comment0) = thread.comments().nth(0).unwrap();
        let (_, comment1) = thread.comments().nth(1).unwrap();

        assert_eq!(thread.comments().count(), 2);
        assert_eq!(comment0.body, "First comment");
        assert_eq!(comment1.body, "Third comment"); // Second comment was redacted.
    }

    #[test]
    fn test_storage() {
        let tmp = tempfile::tempdir().unwrap();
        let (_, signer, repository) = radicle::test::setup::context(&tmp);
        let store =
            radicle::cob::store::Store::<Thread>::open(*signer.public_key(), &repository).unwrap();

        let mut alice = Actor::new(signer);

        let a0 = alice.comment("Thread root", None);
        let a1 = alice.comment("First comment", Some(a0.id()));
        let a2 = alice.comment("Second comment", Some(a0.id()));

        let mut expected = Thread::default();
        expected.apply([a0.clone(), a1.clone(), a2.clone()]);

        let (id, _, _) = store
            .create("Thread created", a0.action, &alice.signer)
            .unwrap();

        let actions = NonEmpty::from_vec(vec![a1.action, a2.action]).unwrap();
        store
            .update(id, "Thread updated", actions, &alice.signer)
            .unwrap();

        let (actual, _) = store.get(&id).unwrap().unwrap();

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_timelines_basic() {
        let mut alice = Actor::<MockSigner>::default();
        let mut bob = Actor::<MockSigner>::default();

        let a0 = alice.comment("Thread root", None);
        let a1 = alice.comment("First comment", Some(a0.id()));
        let a2 = alice.comment("Second comment", Some(a0.id()));

        bob.receive([a0.clone(), a1.clone(), a2.clone()]);
        assert_eq!(
            bob.timeline().collect::<Vec<_>>(),
            alice.timeline().collect::<Vec<_>>()
        );
        assert_eq!(alice.timeline().collect::<Vec<_>>(), vec![&a0, &a1, &a2]);

        bob.reset();
        bob.receive([a0, a2, a1]);
        assert_eq!(
            bob.timeline().collect::<Vec<_>>(),
            alice.timeline().collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_timelines_concurrent() {
        let mut alice = Actor::<MockSigner>::default();
        let mut bob = Actor::<MockSigner>::default();
        let mut eve = Actor::<MockSigner>::default();

        let a0 = alice.comment("Thread root", None);
        let a1 = alice.comment("First comment", Some(a0.id()));

        bob.receive([a0.clone(), a1.clone()]);

        let b0 = bob.comment("Bob's first reply to Alice", Some(a0.id()));
        let b1 = bob.comment("Bob's second reply to Alice", Some(a0.id()));

        eve.receive([a0.clone(), b1.clone(), b0.clone()]);
        let e0 = eve.comment("Eve's first reply to Alice", Some(a0.id()));

        bob.receive([e0.clone()]);
        let b2 = bob.comment("Bob's third reply to Alice", Some(a0.id()));

        eve.receive([b2.clone(), a1.clone()]);
        let e1 = eve.comment("Eve's second reply to Alice", Some(a0.id()));

        alice.receive([b0.clone(), b1.clone(), b2.clone(), e0.clone(), e1.clone()]);
        bob.receive([e1.clone()]);

        let a2 = alice.comment("Second comment", Some(a0.id()));
        eve.receive([a2.clone()]);
        bob.receive([a2.clone()]);

        assert_eq!(alice.ops.len(), 8);
        assert_eq!(bob.ops.len(), 8);
        assert_eq!(eve.ops.len(), 8);

        assert_eq!(
            bob.timeline().collect::<Vec<_>>(),
            alice.timeline().collect::<Vec<_>>()
        );
        assert_eq!(
            eve.timeline().collect::<Vec<_>>(),
            alice.timeline().collect::<Vec<_>>()
        );
        assert_eq!(
            vec![&a0, &a1, &b0, &b1, &e0, &b2, &e1, &a2],
            alice.timeline().collect::<Vec<_>>(),
        );
    }

    #[test]
    fn prop_invariants() {
        fn property(log: Changes<3>) -> TestResult {
            let t = Thread::default();
            let [p1, p2, p3] = log.permutations;

            let mut t1 = t.clone();
            t1.apply(p1);

            let mut t2 = t.clone();
            t2.apply(p2);

            let mut t3 = t;
            t3.apply(p3);

            assert_eq!(t1, t2);
            assert_eq!(t2, t3);
            assert_laws(&t1, &t2, &t3);

            TestResult::passed()
        }
        qcheck::QuickCheck::new()
            .min_tests_passed(100)
            .max_tests(10000)
            .gen(qcheck::Gen::new(7))
            .quickcheck(property as fn(Changes<3>) -> TestResult);
    }
}

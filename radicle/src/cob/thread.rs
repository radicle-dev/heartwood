use std::cmp::Ordering;
use std::ops::{Deref, DerefMut};
use std::str::FromStr;

use once_cell::sync::Lazy;
use radicle_crdt as crdt;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::cob;
use crate::cob::common::{Reaction, Timestamp};
use crate::cob::{ActorId, Op, OpId};
use crate::crypto::Signer;

use crdt::clock::Lamport;
use crdt::{GMap, LWWSet, Max, Redactable, Semilattice};

/// Type name of a thread, as well as the domain for all thread operations.
/// Note that threads are not usually used standalone. They are embeded into other COBs.
pub static TYPENAME: Lazy<cob::TypeName> =
    Lazy::new(|| FromStr::from_str("xyz.radicle.thread").expect("type name is valid"));

/// Error applying an operation onto a state.
#[derive(Error, Debug)]
pub enum OpError {
    /// Causal dependency missing.
    ///
    /// This error indicates that the operations are not being applied
    /// in causal order, which is a requirement for this CRDT.
    ///
    /// For example, this can occur if an operation references anothern operation
    /// that hasn't happened yet.
    #[error("causal dependency {0:?} missing")]
    Missing(OpId),
}

/// Identifies a comment.
pub type CommentId = OpId;

/// A comment edit is just some text and an edit time.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Edit {
    /// When the edit was made.
    pub timestamp: Timestamp,
    /// Edit contents. Replaces previous edits.
    pub body: String,
}

/// A comment on a discussion thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comment {
    /// Comment author.
    author: ActorId,
    /// The comment body.
    edits: GMap<Lamport, Max<Edit>>,
    /// Comment this is a reply to.
    /// Should always be set, except for the root comment.
    reply_to: Option<CommentId>,
}

impl Comment {
    /// Create a new comment.
    pub fn new(
        author: ActorId,
        body: String,
        reply_to: Option<CommentId>,
        timestamp: Timestamp,
    ) -> Self {
        let edit = Edit { body, timestamp };

        Self {
            author,
            edits: GMap::singleton(Lamport::initial(), Max::from(edit)),
            reply_to,
        }
    }

    /// Get the comment body. If there are multiple edits, gets the value at the latest edit.
    pub fn body(&self) -> &str {
        // SAFETY: There is always at least one edit. This is guaranteed by the [`Comment`]
        // constructor.
        #[allow(clippy::unwrap_used)]
        self.edits.values().last().unwrap().get().body.as_str()
    }

    /// Get the comment timestamp, which is the time of the *original* edit. To get the timestamp
    /// of the latest edit, use the [`Comment::edits`] function.
    pub fn timestamp(&self) -> Timestamp {
        // SAFETY: There is always at least one edit. This is guaranteed by the [`Comment`]
        // constructor.
        #[allow(clippy::unwrap_used)]
        self.edits
            .first_key_value()
            .map(|(_, v)| v)
            .unwrap()
            .get()
            .timestamp
    }

    /// Return the comment author.
    pub fn author(&self) -> ActorId {
        self.author
    }

    /// Return the comment this is a reply to. Returns nothing if this is the root comment.
    pub fn reply_to(&self) -> Option<CommentId> {
        self.reply_to
    }

    /// Return the ordered list of edits for this comment, including the original version.
    pub fn edits(&self) -> impl Iterator<Item = &Edit> {
        self.edits.values().map(Max::get)
    }

    /// Add an edit.
    pub fn edit(&mut self, clock: Lamport, body: String, timestamp: Timestamp) {
        self.edits.insert(clock, Edit { body, timestamp }.into())
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
    #[serde(rename_all = "camelCase")]
    Comment {
        /// Comment body.
        body: String,
        /// Comment this is a reply to.
        /// Should be [`None`] if it's the top-level comment.
        /// Should be the root [`CommentId`] if it's a top-level comment.
        reply_to: Option<CommentId>,
    },
    /// Edit a comment.
    Edit { id: CommentId, body: String },
    /// Redact a change. Not all changes can be redacted.
    Redact { id: CommentId },
    /// React to a change.
    React {
        to: CommentId,
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

impl Semilattice for Thread {
    fn merge(&mut self, other: Self) {
        self.comments.merge(other.comments);
        self.reactions.merge(other.reactions);
    }
}

impl Thread {
    pub fn new(id: CommentId, comment: Comment) -> Self {
        Self {
            comments: GMap::singleton(id, Redactable::Present(comment)),
            reactions: GMap::default(),
        }
    }

    pub fn is_initialized(&self) -> bool {
        !self.comments.is_empty()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn len(&self) -> usize {
        self.comments.len()
    }

    pub fn comment(&self, id: &CommentId) -> Option<&Comment> {
        if let Some(Redactable::Present(comment)) = self.comments.get(id) {
            Some(comment)
        } else {
            None
        }
    }

    pub fn first(&self) -> Option<(&CommentId, &Comment)> {
        self.comments().next()
    }

    pub fn last(&self) -> Option<(&CommentId, &Comment)> {
        self.comments().next_back()
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

    pub fn comments(&self) -> impl DoubleEndedIterator<Item = (&CommentId, &Comment)> + '_ {
        self.comments
            .iter()
            .filter_map(|(id, comment)| comment.get().map(|comment| (id, comment)))
    }
}

impl cob::store::FromHistory for Thread {
    type Action = Action;
    type Error = OpError;

    fn type_name() -> &'static radicle_cob::TypeName {
        &*TYPENAME
    }

    fn apply(&mut self, ops: impl IntoIterator<Item = Op<Action>>) -> Result<(), OpError> {
        for op in ops.into_iter() {
            let id = op.id();
            let author = op.author;
            let timestamp = op.timestamp;

            match op.action {
                Action::Comment { body, reply_to } => {
                    self.comments.insert(
                        id,
                        Redactable::Present(Comment::new(author, body, reply_to, timestamp)),
                    );
                }
                Action::Edit { id, body } => {
                    if let Some(Redactable::Present(comment)) = self.comments.get_mut(&id) {
                        comment.edit(op.clock, body, timestamp);
                    } else {
                        return Err(OpError::Missing(id));
                    }
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
        Ok(())
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

    /// Edit a comment.
    pub fn edit(&mut self, id: OpId, body: &str) -> Op<Action> {
        self.op(Action::Edit {
            id,
            body: body.to_owned(),
        })
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
    use std::collections::BTreeSet;
    use std::{array, iter};

    use nonempty::NonEmpty;
    use pretty_assertions::assert_eq;
    use qcheck::{Arbitrary, TestResult};

    use crdt::test::{assert_laws, WeightedGenerator};

    use super::*;
    use crate as radicle;
    use crate::cob::store::FromHistory;
    use crate::cob::test;
    use crate::crypto::test::signer::MockSigner;

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
            let root = OpId::initial(author);
            let gen =
                WeightedGenerator::<(Lamport, Action), (Lamport, BTreeSet<OpId>)>::new(rng.clone())
                    .variant(3, |(clock, comments), rng| {
                        comments.insert(OpId::new(clock.tick(), author));

                        Some((
                            *clock,
                            Action::Comment {
                                body: iter::repeat_with(|| rng.alphabetic()).take(16).collect(),
                                reply_to: Some(root),
                            },
                        ))
                    })
                    .variant(2, |(clock, comments), rng| {
                        if comments.is_empty() {
                            return None;
                        }
                        let id = *comments.iter().nth(rng.usize(..comments.len())).unwrap();

                        Some((
                            *clock,
                            Action::Edit {
                                id,
                                body: iter::repeat_with(|| rng.alphabetic()).take(16).collect(),
                            },
                        ))
                    })
                    .variant(2, |(clock, comments), rng| {
                        if comments.is_empty() {
                            return None;
                        }
                        let to = *comments.iter().nth(rng.usize(..comments.len())).unwrap();

                        Some((
                            clock.tick(),
                            Action::React {
                                to,
                                reaction: Reaction::new('✨').unwrap(),
                                active: rng.bool(),
                            },
                        ))
                    })
                    .variant(2, |(clock, comments), rng| {
                        if comments.is_empty() {
                            return None;
                        }
                        let id = *comments.iter().nth(rng.usize(..comments.len())).unwrap();
                        comments.remove(&id);

                        Some((clock.tick(), Action::Redact { id }))
                    });

            let mut ops = vec![Op::new(
                Action::Comment {
                    body: String::default(),
                    reply_to: None,
                },
                author,
                Timestamp::now(),
                Lamport::initial(),
            )];
            let mut permutations: [Vec<Op<Action>>; N] = array::from_fn(|_| Vec::new());

            for (clock, action) in gen.take(g.size()) {
                let timestamp = Timestamp::now() + rng.u64(..60);
                ops.push(Op::new(action, author, timestamp, clock));
            }

            for p in &mut permutations {
                *p = ops.clone();
                rng.shuffle(&mut ops);
            }

            Changes { permutations }
        }
    }

    mod setup {
        use super::*;
        use crate::storage::git as storage;
        use cob::store::Store;

        pub fn store<'a, G: Signer>(
            signer: &G,
            repo: &'a storage::Repository,
        ) -> Store<'a, Thread> {
            Store::<Thread>::open(*signer.public_key(), repo).unwrap()
        }
    }

    #[test]
    fn test_redact_comment() {
        let tmp = tempfile::tempdir().unwrap();
        let (_, signer, _) = radicle::test::setup::context(&tmp);
        let mut alice = Actor::new(signer);
        let mut thread = Thread::default();

        let a0 = alice.comment("First comment", None);
        let a1 = alice.comment("Second comment", Some(a0.id()));
        let a2 = alice.comment("Third comment", Some(a0.id()));

        thread.apply([a0, a1.clone(), a2]).unwrap();
        assert_eq!(thread.comments().count(), 3);

        // Redact the second comment.
        let a3 = alice.redact(a1.id());
        thread.apply([a3]).unwrap();

        let (_, comment0) = thread.comments().nth(0).unwrap();
        let (_, comment1) = thread.comments().nth(1).unwrap();

        assert_eq!(thread.comments().count(), 2);
        assert_eq!(comment0.body(), "First comment");
        assert_eq!(comment1.body(), "Third comment"); // Second comment was redacted.
    }

    #[test]
    fn test_edit_comment() {
        let mut alice = Actor::<MockSigner>::default();

        let c0 = alice.comment("Hello world!", None);
        let c1 = alice.edit(c0.id(), "Goodbye world.");
        let c2 = alice.edit(c0.id(), "Goodbye world!");

        let mut t1 = Thread::default();
        t1.apply([c0.clone(), c1.clone(), c2.clone()]).unwrap();

        let comment = t1.comment(&c0.id());
        let edits = comment.unwrap().edits().collect::<Vec<_>>();

        assert_eq!(edits[0].body.as_str(), "Hello world!");
        assert_eq!(edits[1].body.as_str(), "Goodbye world.");
        assert_eq!(edits[2].body.as_str(), "Goodbye world!");
        assert_eq!(t1.comment(&c0.id()).unwrap().body(), "Goodbye world!");

        let mut t2 = Thread::default();
        t2.apply([c0, c2, c1]).unwrap(); // Apply in different order.

        assert_eq!(t1, t2);
    }

    #[test]
    fn test_storage() {
        let tmp = tempfile::tempdir().unwrap();
        let (_, signer, repository) = radicle::test::setup::context(&tmp);
        let store = setup::store(&signer, &repository);
        let mut alice = Actor::new(signer);

        let a0 = alice.comment("Thread root", None);
        let a1 = alice.comment("First comment", Some(a0.id()));
        let a2 = alice.comment("Second comment", Some(a0.id()));

        let mut expected = Thread::default();
        expected
            .apply([a0.clone(), a1.clone(), a2.clone()])
            .unwrap();

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
    fn test_histories() {
        let mut alice = Actor::<MockSigner>::default();
        let mut bob = Actor::<MockSigner>::default();
        let mut eve = Actor::<MockSigner>::default();

        let a0 = alice.comment("Alice's comment", None);
        let b0 = bob.comment("Bob's reply", Some(a0.id())); // Bob and Eve's replies are concurrent.
        let e0 = eve.comment("Eve's reply", Some(a0.id()));

        let mut a = test::history::<Thread>(&a0);
        let mut b = a.clone();
        let mut e = a.clone();

        b.append(&b0);
        e.append(&e0);

        a.merge(b);
        a.merge(e);

        let (expected, _) = Thread::from_history(&a).unwrap();
        for permutation in a.permutations(2) {
            let actual = Thread::from_ops(permutation).unwrap();
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn prop_invariants() {
        fn property(log: Changes<3>) -> TestResult {
            let t = Thread::default();
            let [p1, p2, p3] = log.permutations;

            let mut t1 = t.clone();
            if t1.apply(p1).is_err() {
                return TestResult::discard();
            }

            let mut t2 = t.clone();
            if t2.apply(p2).is_err() {
                return TestResult::discard();
            }

            let mut t3 = t;
            if t3.apply(p3).is_err() {
                return TestResult::discard();
            }

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

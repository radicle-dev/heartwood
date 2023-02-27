use std::cmp::Ordering;
use std::str::FromStr;

use once_cell::sync::Lazy;
use radicle_crdt as crdt;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::cob;
use crate::cob::common::{Reaction, Timestamp};
use crate::cob::{ActorId, Op, OpId};

use crdt::clock::Lamport;
use crdt::{GMap, GSet, LWWSet, Max, Redactable, Semilattice};

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
    /// Comment timeline.
    timeline: GSet<(Lamport, OpId)>,
}

impl Semilattice for Thread {
    fn merge(&mut self, other: Self) {
        self.comments.merge(other.comments);
        self.reactions.merge(other.reactions);
        self.timeline.merge(other.timeline);
    }
}

impl Thread {
    pub fn new(id: CommentId, comment: Comment) -> Self {
        Self {
            comments: GMap::singleton(id, Redactable::Present(comment)),
            reactions: GMap::default(),
            timeline: GSet::default(),
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

    pub fn root(&self) -> (&CommentId, &Comment) {
        self.first().expect("Thread::root: thread is empty")
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
        self.timeline.iter().filter_map(|(_, id)| {
            self.comments
                .get(id)
                .and_then(Redactable::get)
                .map(|comment| (id, comment))
        })
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
            let id = op.id;
            let author = op.author;
            let timestamp = op.timestamp;

            self.timeline.insert((op.clock, op.id));

            match op.action {
                Action::Comment { body, reply_to } => {
                    // Since comments are keyed by content hash, we shouldn't re-insert a comment
                    // if it already exists, otherwise this will be resolved via the `merge`
                    // operation of `Redactable`.
                    if self.comments.contains_key(&id) {
                        continue;
                    }
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::ops::{Deref, DerefMut};
    use std::{array, iter};

    use pretty_assertions::assert_eq;
    use qcheck::{Arbitrary, TestResult};

    use crdt::test::{assert_laws, WeightedGenerator};

    use super::*;
    use crate as radicle;
    use crate::cob::store::FromHistory;
    use crate::cob::test;
    use crate::crypto::test::signer::MockSigner;
    use crate::crypto::Signer;

    /// An object that can be used to create and sign changes.
    pub struct Actor<G> {
        inner: cob::test::Actor<G, Action>,
    }

    impl<G: Default + Signer> Default for Actor<G> {
        fn default() -> Self {
            Self {
                inner: cob::test::Actor::new(G::default()),
            }
        }
    }

    impl<G: Signer> Actor<G> {
        pub fn new(signer: G) -> Self {
            Self {
                inner: cob::test::Actor::new(signer),
            }
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

        /// React to a comment.
        pub fn react(&mut self, to: OpId, reaction: Reaction, active: bool) -> Op<Action> {
            self.op(Action::React {
                to,
                reaction,
                active,
            })
        }
    }

    impl<G> Deref for Actor<G> {
        type Target = cob::test::Actor<G, Action>;

        fn deref(&self) -> &Self::Target {
            &self.inner
        }
    }

    impl<G> DerefMut for Actor<G> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.inner
        }
    }

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
            let rng = fastrand::Rng::with_seed(u64::arbitrary(g));
            let gen = WeightedGenerator::<
                (Lamport, Op<Action>),
                (Actor<MockSigner>, Lamport, BTreeSet<OpId>),
            >::new(rng.clone())
            .variant(3, |(actor, clock, comments), rng| {
                let comment = actor.comment(
                    iter::repeat_with(|| rng.alphabetic())
                        .take(4)
                        .collect::<String>()
                        .as_str(),
                    None,
                );
                comments.insert(comment.id);

                Some((*clock, comment))
            })
            .variant(2, |(actor, clock, comments), rng| {
                if comments.is_empty() {
                    return None;
                }
                let id = *comments.iter().nth(rng.usize(..comments.len())).unwrap();
                let edit = actor.edit(
                    id,
                    iter::repeat_with(|| rng.alphabetic())
                        .take(4)
                        .collect::<String>()
                        .as_str(),
                );

                Some((*clock, edit))
            })
            .variant(2, |(actor, clock, comments), rng| {
                if comments.is_empty() {
                    return None;
                }
                let to = *comments.iter().nth(rng.usize(..comments.len())).unwrap();
                let react = actor.react(to, Reaction::new('✨').unwrap(), rng.bool());

                Some((clock.tick(), react))
            })
            .variant(2, |(actor, clock, comments), rng| {
                if comments.is_empty() {
                    return None;
                }
                let id = *comments.iter().nth(rng.usize(..comments.len())).unwrap();
                comments.remove(&id);
                let redact = actor.redact(id);

                Some((clock.tick(), redact))
            });

            let mut ops = vec![Actor::<MockSigner>::default().comment("", None)];
            let mut permutations: [Vec<Op<Action>>; N] = array::from_fn(|_| Vec::new());

            for (_, op) in gen.take(g.size()) {
                ops.push(op);
            }

            for p in &mut permutations {
                *p = ops.clone();
                rng.shuffle(&mut ops);
            }

            Changes { permutations }
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
    fn test_duplicate_comments() {
        let mut alice = Actor::<MockSigner>::default();
        let mut bob = Actor::<MockSigner>::default();

        let a0 = alice.comment("Hello World!", None);
        let b0 = bob.comment("Hello World!", None);

        let mut a = test::history::<Thread>(&a0);
        let mut b = a.clone();

        b.append(&b0);
        a.merge(b);

        let (thread, _) = Thread::from_history(&a).unwrap();

        assert_eq!(thread.comments().count(), 2);

        let (first_id, first) = thread.comments().nth(0).unwrap();
        let (second_id, second) = thread.comments().nth(1).unwrap();

        assert!(first_id != second_id); // The ids are not the same,
        assert_eq!(first.edits, second.edits); // despite the content being the same.
    }

    #[test]
    fn test_duplicate_comments_same_author() {
        let mut alice = Actor::<MockSigner>::default();

        let a0 = alice.comment("Hello World!", None);
        let a1 = alice.comment("Hello World!", None);
        let a2 = alice.comment("Hello World!", None);

        // These simulate two devices sharing the same key.
        let mut h1 = test::history::<Thread>(&a0);
        let mut h2 = h1.clone();
        let mut h3 = h1.clone();

        // Alice writes the same comment on both devices, not realizing what she has done.
        h1.append(&a1);
        h2.append(&a2);

        // Eventually the histories are merged by a third party.
        h3.merge(h1);
        h3.merge(h2);

        let (thread, _) = Thread::from_history(&h3).unwrap();

        // The three comments, distinct yet identical in terms of content, are preserved.
        assert_eq!(thread.comments().count(), 3);

        let (first_id, first) = thread.comments().nth(0).unwrap();
        let (second_id, second) = thread.comments().nth(1).unwrap();
        let (third_id, third) = thread.comments().nth(2).unwrap();

        // Their IDs are not the same.
        assert!(first_id != second_id);
        assert!(second_id != third_id);
        // Their content are the same.
        assert_eq!(first, second);
        assert_eq!(second, third);
    }

    #[test]
    fn test_comment_edit_reinsert() {
        let mut alice = Actor::<MockSigner>::default();
        let mut t1 = Thread::default();
        let mut t2 = Thread::default();

        let a1 = alice.comment("Hello.", None);
        let a2 = alice.edit(a1.id(), "Hello World.");

        t1.apply([a1.clone(), a2.clone(), a1.clone()]).unwrap();
        t2.apply([a1.clone(), a1, a2]).unwrap();

        assert_eq!(t1, t2);
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

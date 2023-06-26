use std::cmp::Ordering;
use std::str::FromStr;

use once_cell::sync::Lazy;
use radicle_crdt as crdt;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::cob;
use crate::cob::common::{Reaction, Timestamp};
use crate::cob::{ActorId, EntryId, Op};
use crate::prelude::ReadRepository;

use crdt::clock::Lamport;
use crdt::{GMap, GSet, LWWSet, Max, Redactable, Semilattice};

/// Type name of a thread, as well as the domain for all thread operations.
/// Note that threads are not usually used standalone. They are embeded into other COBs.
pub static TYPENAME: Lazy<cob::TypeName> =
    Lazy::new(|| FromStr::from_str("xyz.radicle.thread").expect("type name is valid"));

/// Error applying an operation onto a state.
#[derive(Error, Debug)]
pub enum Error {
    /// Causal dependency missing.
    ///
    /// This error indicates that the operations are not being applied
    /// in causal order, which is a requirement for this CRDT.
    ///
    /// For example, this can occur if an operation references anothern operation
    /// that hasn't happened yet.
    #[error("causal dependency {0:?} missing")]
    Missing(EntryId),
    /// Validation error.
    #[error("validation failed: {0}")]
    Validate(&'static str),
    /// Error with comment operation.
    #[error("comment {0} is invalid")]
    Comment(EntryId),
    /// Error with edit operation.
    #[error("edit {0} is invalid")]
    Edit(EntryId),
}

/// Identifies a comment.
pub type CommentId = EntryId;

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
    /// React to a comment.
    React {
        to: CommentId,
        reaction: Reaction,
        active: bool,
    },
}

impl cob::store::HistoryAction for Action {}

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
    timeline: GSet<(Lamport, EntryId)>,
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
    type Error = Error;

    fn type_name() -> &'static radicle_cob::TypeName {
        &*TYPENAME
    }

    fn validate(&self) -> Result<(), Self::Error> {
        if self.comments.is_empty() {
            return Err(Error::Validate("no comments found"));
        }
        Ok(())
    }

    fn apply<R: ReadRepository>(
        &mut self,
        ops: impl IntoIterator<Item = Op<Action>>,
        _repo: &R,
    ) -> Result<(), Error> {
        for op in ops.into_iter() {
            let id = op.id;
            let author = op.author;
            let timestamp = op.timestamp;

            self.timeline.insert((op.clock, op.id));

            match op.action {
                Action::Comment { body, reply_to } => {
                    if body.is_empty() {
                        return Err(Error::Comment(op.id));
                    }
                    // Nb. If a comment is already present, it must be redacted, because the
                    // underlying store guarantees exactly-once delivery of ops.
                    self.comments.insert(
                        id,
                        Redactable::Present(Comment::new(author, body, reply_to, timestamp)),
                    );
                }
                Action::Edit { id, body } => {
                    if body.is_empty() {
                        return Err(Error::Edit(op.id));
                    }
                    // It's possible for a comment to be redacted before we're able to edit it, in
                    // case of a concurrent update.
                    //
                    // However, it's *not* possible for the comment to be absent. Therefore we treat
                    // that as an error.
                    if let Some(redactable) = self.comments.get_mut(&id) {
                        if let Redactable::Present(comment) = redactable {
                            comment.edit(op.clock, body, timestamp);
                        }
                    } else {
                        return Err(Error::Missing(id));
                    }
                }
                Action::Redact { id } => {
                    // Redactions must have observed a comment to be valid.
                    if let Some(comment) = self.comments.get_mut(&id) {
                        comment.merge(Redactable::Redacted);
                    } else {
                        return Err(Error::Missing(id));
                    }
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
    use crate::test::arbitrary;
    use crate::test::arbitrary::gen;
    use crate::test::storage::MockRepository;

    /// An object that can be used to create and sign changes.
    pub struct Actor<G> {
        inner: cob::test::Actor<G>,
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
        pub fn comment(&mut self, body: &str, reply_to: Option<CommentId>) -> Op<Action> {
            self.op(Action::Comment {
                body: String::from(body),
                reply_to,
            })
        }

        /// Create a new redaction.
        pub fn redact(&mut self, id: CommentId) -> Op<Action> {
            self.op(Action::Redact { id })
        }

        /// Edit a comment.
        pub fn edit(&mut self, id: CommentId, body: &str) -> Op<Action> {
            self.op(Action::Edit {
                id,
                body: body.to_owned(),
            })
        }

        /// React to a comment.
        pub fn react(&mut self, to: CommentId, reaction: Reaction, active: bool) -> Op<Action> {
            self.op(Action::React {
                to,
                reaction,
                active,
            })
        }
    }

    impl<G> Deref for Actor<G> {
        type Target = cob::test::Actor<G>;

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
                (Actor<MockSigner>, Lamport, BTreeSet<EntryId>),
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

                Some((clock.tick(), comment))
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
                Some((clock.tick(), edit))
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

            let mut ops = vec![Actor::<MockSigner>::default().comment("Root", None)];
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
        let radicle::test::setup::Context { signer, .. } = radicle::test::setup::Context::new(&tmp);
        let repo = gen::<MockRepository>(1);
        let mut alice = Actor::new(signer);
        let mut thread = Thread::default();

        let a0 = alice.comment("First comment", None);
        let a1 = alice.comment("Second comment", Some(a0.id()));
        let a2 = alice.comment("Third comment", Some(a0.id()));

        thread.apply([a0, a1.clone(), a2], &repo).unwrap();
        assert_eq!(thread.comments().count(), 3);

        // Redact the second comment.
        let a3 = alice.redact(a1.id());
        thread.apply([a3], &repo).unwrap();

        let (_, comment0) = thread.comments().nth(0).unwrap();
        let (_, comment1) = thread.comments().nth(1).unwrap();

        assert_eq!(thread.comments().count(), 2);
        assert_eq!(comment0.body(), "First comment");
        assert_eq!(comment1.body(), "Third comment"); // Second comment was redacted.
    }

    #[test]
    fn test_edit_comment() {
        let mut alice = Actor::<MockSigner>::default();
        let repo = gen::<MockRepository>(1);

        let c0 = alice.comment("Hello world!", None);
        let c1 = alice.edit(c0.id(), "Goodbye world.");
        let c2 = alice.edit(c0.id(), "Goodbye world!");

        let mut t1 = Thread::default();
        t1.apply([c0.clone(), c1.clone(), c2.clone()], &repo)
            .unwrap();

        let comment = t1.comment(&c0.id());
        let edits = comment.unwrap().edits().collect::<Vec<_>>();

        assert_eq!(edits[0].body.as_str(), "Hello world!");
        assert_eq!(edits[1].body.as_str(), "Goodbye world.");
        assert_eq!(edits[2].body.as_str(), "Goodbye world!");
        assert_eq!(t1.comment(&c0.id()).unwrap().body(), "Goodbye world!");

        let mut t2 = Thread::default();
        t2.apply([c0, c2, c1], &repo).unwrap(); // Apply in different order.

        assert_eq!(t1, t2);
    }

    #[test]
    fn test_timeline() {
        let alice = MockSigner::default();
        let bob = MockSigner::default();
        let eve = MockSigner::default();
        let repo = gen::<MockRepository>(1);

        let mut a = test::history::<Thread, _>(
            &Action::Comment {
                body: "Thread root".to_owned(),
                reply_to: None,
            },
            &alice,
        );
        a.comment("Alice comment", Some(a.root()), &alice);

        let mut b = a.clone();
        let b1 = b.comment("Bob comment", Some(a.root()), &bob);

        let mut e = a.clone();
        let e1 = e.comment("Eve comment", Some(a.root()), &eve);

        assert_eq!(a.as_ref().len(), 2);
        assert_eq!(b.as_ref().len(), 3);
        assert_eq!(e.as_ref().len(), 3);

        a.merge(b.clone());
        a.merge(e.clone());

        assert_eq!(a.as_ref().len(), 4);

        b.merge(a.clone());
        b.merge(e.clone());

        e.merge(a.clone());
        e.merge(b.clone());

        assert_eq!(a, b);
        assert_eq!(b, e);

        let (t1, _) = Thread::from_history(&a, &repo).unwrap();
        let (t2, _) = Thread::from_history(&b, &repo).unwrap();
        let (t3, _) = Thread::from_history(&e, &repo).unwrap();

        assert_eq!(t1, t2);
        assert_eq!(t2, t3);

        let timeline1 = t1.comments().collect::<Vec<_>>();
        let timeline2 = t2.comments().collect::<Vec<_>>();
        let timeline3 = t3.comments().collect::<Vec<_>>();

        assert_eq!(timeline1, timeline2);
        assert_eq!(timeline2, timeline3);
        assert_eq!(timeline1.len(), 4);
        assert_eq!(
            timeline1.iter().map(|(_, c)| c.body()).collect::<Vec<_>>(),
            // Since the operations are concurrent, the ordering depends on the ordering between
            // the operation ids.
            if e1 > b1 {
                vec!["Thread root", "Alice comment", "Bob comment", "Eve comment"]
            } else {
                vec!["Thread root", "Alice comment", "Eve comment", "Bob comment"]
            }
        );

        for ops in a.permutations(2) {
            let t = Thread::from_ops(ops, &repo).unwrap();
            assert_eq!(t, t1);
        }
    }

    #[test]
    fn test_duplicate_comments() {
        let repo = gen::<MockRepository>(1);
        let alice = MockSigner::default();
        let bob = MockSigner::default();

        let mut a = test::history::<Thread, _>(
            &Action::Comment {
                body: "Thread root".to_owned(),
                reply_to: None,
            },
            &alice,
        );
        let mut b = a.clone();

        a.comment("Hello World!", None, &alice);
        b.comment("Hello World!", None, &bob);

        a.merge(b);

        let (thread, _) = Thread::from_history(&a, &repo).unwrap();

        assert_eq!(thread.comments().count(), 3);

        let (first_id, first) = thread.comments().nth(1).unwrap();
        let (second_id, second) = thread.comments().nth(2).unwrap();

        assert!(first_id != second_id); // The ids are not the same,
        assert_eq!(first.edits, second.edits); // despite the content being the same.
    }

    #[test]
    fn test_comment_redact_missing() {
        let repo = gen::<MockRepository>(1);
        let mut alice = Actor::<MockSigner>::default();
        let mut t = Thread::default();
        let id = arbitrary::entry_id();

        t.apply([alice.redact(id)], &repo).unwrap_err();
    }

    #[test]
    fn test_comment_edit_missing() {
        let repo = gen::<MockRepository>(1);
        let mut alice = Actor::<MockSigner>::default();
        let mut t = Thread::default();
        let id = arbitrary::entry_id();

        t.apply([alice.edit(id, "Edited")], &repo).unwrap_err();
    }

    #[test]
    fn test_comment_edit_redacted() {
        let repo = gen::<MockRepository>(1);
        let mut alice = Actor::<MockSigner>::default();
        let mut t = Thread::default();

        let a1 = alice.comment("Hi", None);
        let a2 = alice.redact(a1.id);
        let a3 = alice.edit(a1.id, "Edited");

        t.apply([a1, a2, a3], &repo).unwrap();
        assert_eq!(t.comments().count(), 0);
    }

    #[test]
    fn prop_invariants() {
        fn property(repo: MockRepository, log: Changes<3>) -> TestResult {
            let t = Thread::default();
            let [p1, p2, p3] = log.permutations;

            let mut t1 = t.clone();
            if t1.apply(p1, &repo).is_err() {
                return TestResult::discard();
            }

            let mut t2 = t.clone();
            if t2.apply(p2, &repo).is_err() {
                return TestResult::discard();
            }

            let mut t3 = t;
            if t3.apply(p3, &repo).is_err() {
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
            .quickcheck(property as fn(MockRepository, Changes<3>) -> TestResult);
    }
}

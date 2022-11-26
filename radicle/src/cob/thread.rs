use std::collections::btree_map::Entry;
use std::collections::BTreeMap;
use std::ops::{ControlFlow, Deref, DerefMut};
use std::str::FromStr;

use once_cell::sync::Lazy;
use radicle_crdt as crdt;
use serde::{Deserialize, Serialize};

use crate::cob::common::{Reaction, Tag};
use crate::cob::store;
use crate::cob::{History, TypeName};
use crate::crypto::Signer;

use crdt::clock::LClock;
use crdt::lwwreg::LWWReg;
use crdt::lwwset::LWWSet;
use crdt::redactable::Redactable;
use crdt::{ActorId, Change, ChangeId, Semilattice};

/// Type name of a thread.
pub static TYPENAME: Lazy<TypeName> =
    Lazy::new(|| FromStr::from_str("xyz.radicle.thread").expect("type name is valid"));

/// Identifies a comment.
pub type CommentId = ChangeId;

/// A comment on a discussion thread.
#[derive(Default, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Comment {
    /// The comment body.
    pub body: String,
    /// Thread or comment this is a reply to.
    pub reply_to: Option<ChangeId>,
}

impl Comment {
    /// Create a new comment.
    pub fn new(body: String, reply_to: Option<ChangeId>) -> Self {
        Self { body, reply_to }
    }
}

impl PartialOrd for Comment {
    fn partial_cmp(&self, _other: &Self) -> Option<std::cmp::Ordering> {
        None
    }
}

/// An action that can be carried out in a change.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub enum Action {
    /// Comment on a thread.
    Comment { comment: Comment },
    /// Redact a change. Not all changes can be redacted.
    Redact { id: ChangeId },
    /// Add tags to the thread.
    Tag { tags: Vec<Tag> },
    /// Remove tags from the thread.
    Untag { tags: Vec<Tag> },
    /// React to a change.
    React {
        to: ChangeId,
        reaction: Reaction,
        active: bool,
    },
}

/// A discussion thread.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Thread {
    /// The comments under the thread.
    comments: BTreeMap<CommentId, Redactable<Comment>>,
    /// Associated tags.
    tags: BTreeMap<Tag, LWWReg<bool, LClock>>,
    /// Reactions to changes.
    reactions: BTreeMap<CommentId, LWWSet<(ActorId, Reaction), LClock>>,
}

impl store::FromHistory for Thread {
    fn type_name() -> &'static radicle_cob::TypeName {
        &*TYPENAME
    }

    fn from_history(history: &History) -> Result<Self, store::Error> {
        Ok(history.traverse(Thread::default(), |mut acc, entry| {
            if let Ok(change) = Change::decode(entry.contents()) {
                acc.apply([change]);
                ControlFlow::Continue(acc)
            } else {
                ControlFlow::Break(acc)
            }
        }))
    }
}

impl Semilattice for Thread {
    fn merge(&mut self, other: Self) {
        self.comments.merge(other.comments);
        self.tags.merge(other.tags);
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
    pub fn clear(&mut self) {
        self.comments.clear();
    }

    pub fn comment(&self, id: &CommentId) -> Option<&Comment> {
        if let Some(Redactable::Present(comment)) = self.comments.get(id) {
            Some(comment)
        } else {
            None
        }
    }

    pub fn replies<'a>(
        &'a self,
        to: &'a CommentId,
    ) -> impl Iterator<Item = (&CommentId, &Comment)> {
        self.comments().filter_map(move |(id, c)| {
            if let Some(parent) = &c.reply_to {
                if parent == to {
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

    pub fn apply(&mut self, changes: impl IntoIterator<Item = Change<Action>>) {
        for change in changes.into_iter() {
            let id = change.id();

            match change.action {
                Action::Comment { comment } => {
                    let present = Redactable::Present(comment);

                    match self.comments.entry(id) {
                        Entry::Vacant(e) => {
                            e.insert(present);
                        }
                        Entry::Occupied(mut e) => {
                            e.get_mut().merge(present);
                        }
                    }
                }
                Action::Redact { id } => {
                    self.comments
                        .entry(id)
                        .and_modify(|e| e.merge(Redactable::Redacted))
                        .or_insert(Redactable::Redacted);
                }
                Action::Tag { tags } => {
                    for tag in tags {
                        self.tags
                            .entry(tag)
                            .and_modify(|r| r.set(true, change.clock))
                            .or_insert_with(|| LWWReg::new(true, change.clock));
                    }
                }
                Action::Untag { tags } => {
                    for tag in tags {
                        self.tags
                            .entry(tag)
                            .and_modify(|r| r.set(false, change.clock))
                            .or_insert_with(|| LWWReg::new(false, change.clock));
                    }
                }
                Action::React {
                    to,
                    reaction,
                    active,
                } => {
                    self.reactions
                        .entry(to)
                        .and_modify(|reactions| {
                            if active {
                                reactions.insert((change.author, reaction), change.clock);
                            } else {
                                reactions.remove((change.author, reaction), change.clock);
                            }
                        })
                        .or_insert_with(|| {
                            if active {
                                LWWSet::singleton((change.author, reaction), change.clock)
                            } else {
                                let mut set = LWWSet::default();
                                set.remove((change.author, reaction), change.clock);
                                set
                            }
                        });
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

    pub fn tags(&self) -> impl Iterator<Item = &Tag> + '_ {
        self.tags
            .iter()
            .filter_map(|(tag, r)| if *r.get() { Some(tag) } else { None })
    }
}

/// An object that can be used to create and sign changes.
pub struct Actor<G> {
    inner: crdt::Actor<G, Action>,
}

impl<G: Default + Signer> Default for Actor<G> {
    fn default() -> Self {
        Self {
            inner: crdt::Actor::new(G::default()),
        }
    }
}

impl<G: Signer> Actor<G> {
    pub fn new(signer: G) -> Self {
        Self {
            inner: crdt::Actor::new(signer),
        }
    }

    /// Create a new thread.
    pub fn thread(&self) -> Thread {
        Thread::default()
    }

    /// Create a new comment.
    pub fn comment(&mut self, body: &str, parent: Option<ChangeId>) -> Change<Action> {
        self.change(Action::Comment {
            comment: Comment::new(String::from(body), parent),
        })
    }

    /// Add a tag.
    pub fn tag(&mut self, tag: Tag) -> Change<Action> {
        self.change(Action::Tag { tags: vec![tag] })
    }

    /// Remove a tag.
    pub fn untag(&mut self, tag: Tag) -> Change<Action> {
        self.change(Action::Untag { tags: vec![tag] })
    }

    /// Create a new redaction.
    pub fn redact(&mut self, id: ChangeId) -> Change<Action> {
        self.change(Action::Redact { id })
    }
}

impl<G> Deref for Actor<G> {
    type Target = crdt::Actor<G, Action>;

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
    use pretty_assertions::assert_eq;
    use quickcheck::Arbitrary;
    use quickcheck_macros::quickcheck;

    use super::*;
    use crate as radicle;
    use crdt::test::{assert_laws, WeightedGenerator};

    #[derive(Clone)]
    struct Changes<const N: usize> {
        permutations: [Vec<Change<Action>>; N],
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
        fn arbitrary(g: &mut quickcheck::Gen) -> Self {
            let rng = fastrand::Rng::with_seed(u64::arbitrary(g));
            let gen =
                WeightedGenerator::<Action, (Vec<Tag>, Vec<Change<Action>>)>::new(rng.clone())
                    .variant(2, |_, rng| {
                        Some(Action::Comment {
                            comment: Comment {
                                body: iter::repeat_with(|| rng.alphabetic()).take(16).collect(),
                                reply_to: Default::default(),
                            },
                        })
                    })
                    .variant(2, |(_, changes), rng| {
                        if changes.is_empty() {
                            return None;
                        }
                        let to = changes[rng.usize(..changes.len())].id();

                        Some(Action::React {
                            to,
                            reaction: Reaction::new('✨').unwrap(),
                            active: rng.bool(),
                        })
                    })
                    .variant(2, |(_, changes), rng| {
                        if changes.is_empty() {
                            return None;
                        }
                        let id = changes[rng.usize(..changes.len())].id();
                        Some(Action::Redact { id })
                    })
                    .variant(2, |(tags, _), rng| {
                        let tag = if tags.is_empty() || rng.bool() {
                            let tag = iter::repeat_with(|| rng.alphabetic())
                                .take(8)
                                .collect::<String>();
                            let tag = Tag::new(tag).unwrap();
                            tags.push(tag.clone());
                            tag
                        } else {
                            tags[rng.usize(..tags.len())].clone()
                        };
                        Some(Action::Tag { tags: vec![tag] })
                    })
                    .variant(2, |(tags, _), rng| {
                        if tags.is_empty() {
                            return None;
                        }
                        let tag = tags[rng.usize(..tags.len())].clone();
                        Some(Action::Untag { tags: vec![tag] })
                    });

            let mut changes = Vec::new();
            let mut permutations: [Vec<Change<Action>>; N] = array::from_fn(|_| Vec::new());
            let mut clock = LClock::default();
            let author = ActorId::from([0; 32]);

            for action in gen.take(g.size().min(8)) {
                let clock = clock.tick();

                changes.push(Change {
                    action,
                    author,
                    clock,
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
    fn test_storage() {
        let tmp = tempfile::tempdir().unwrap();
        let (_, signer, repository) = radicle::test::setup::context(&tmp);
        let store =
            radicle::cob::store::Store::<Thread>::open(*signer.public_key(), &repository).unwrap();

        let mut alice = Actor::new(signer);

        let a1 = alice.comment("First comment", None);
        let a2 = alice.comment("Second comment", None);

        let mut expected = Thread::default();
        expected.apply([a1.clone(), a2.clone()]);

        let (id, _) = store.create("Thread created", a1, &alice.signer).unwrap();
        store
            .update(id, "Thread updated", a2, &alice.signer)
            .unwrap();

        let actual = store.get(&id).unwrap().unwrap();

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_timelines_basic() {
        let mut alice = Actor::<MockSigner>::default();
        let mut bob = Actor::<MockSigner>::default();

        let a1 = alice.comment("First comment", None);
        let a2 = alice.comment("Second comment", None);

        bob.receive([a1.clone(), a2.clone()]);
        assert_eq!(
            bob.timeline().collect::<Vec<_>>(),
            alice.timeline().collect::<Vec<_>>()
        );
        assert_eq!(alice.timeline().collect::<Vec<_>>(), vec![&a1, &a2]);

        bob.reset();
        bob.receive([a2, a1]);
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

        let a1 = alice.comment("First comment", None);

        bob.receive([a1.clone()]);

        let b0 = bob.comment("Bob's first reply to Alice", None);
        let b1 = bob.comment("Bob's second reply to Alice", None);

        eve.receive([b1.clone(), b0.clone()]);
        let e0 = eve.comment("Eve's first reply to Alice", None);

        bob.receive([e0.clone()]);
        let b2 = bob.comment("Bob's third reply to Alice", None);

        eve.receive([b2.clone(), a1.clone()]);
        let e1 = eve.comment("Eve's second reply to Alice", None);

        alice.receive([b0.clone(), b1.clone(), b2.clone(), e0.clone(), e1.clone()]);
        bob.receive([e1.clone()]);

        let a2 = alice.comment("Second comment", None);
        eve.receive([a2.clone()]);
        bob.receive([a2.clone()]);

        assert_eq!(alice.changes.len(), 7);
        assert_eq!(bob.changes.len(), 7);
        assert_eq!(eve.changes.len(), 7);

        assert_eq!(
            bob.timeline().collect::<Vec<_>>(),
            alice.timeline().collect::<Vec<_>>()
        );
        assert_eq!(
            eve.timeline().collect::<Vec<_>>(),
            alice.timeline().collect::<Vec<_>>()
        );
        assert_eq!(
            vec![&a1, &b0, &b1, &e0, &b2, &e1, &a2],
            alice.timeline().collect::<Vec<_>>(),
        );
    }

    #[quickcheck]
    fn prop_invariants(log: Changes<3>) {
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
    }
}

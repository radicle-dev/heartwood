use std::collections::BTreeMap;
use std::ops::Deref;

use serde::{Deserialize, Serialize};

use radicle::cob::common::Reaction;
use radicle::cob::Timestamp;
use radicle::crypto::{PublicKey, Signature, Signer};
use radicle::hash;

use crate::clock::LClock;
use crate::lwwreg::LWWReg;
use crate::lwwset::LWWSet;

/// Identifies a change.
pub type ChangeId = radicle::hash::Digest;
/// Identifies a tag.
pub type TagId = String;
/// The author of a change.
pub type Author = PublicKey;
/// Alias for `Author`.
pub type ActorId = PublicKey;

/// The `Change` is the unit of replication.
/// Everything that can be done in the system is represented by a `Change` object.
/// Changes are applied to an accumulator to yield a final state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Change {
    /// The action carried out by this change.
    action: Action,
    /// The author of the change.
    author: Author,
    /// The time at which this change was authored.
    timestamp: Timestamp,
    /// Lamport clock.
    clock: LClock,
}

impl Change {
    /// Get the change id.
    pub fn id(&self) -> ChangeId {
        hash::Digest::new(self.encode())
    }

    /// Serialize the change into a byte string.
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        let mut serializer =
            serde_json::Serializer::with_formatter(&mut buf, olpc_cjson::CanonicalFormatter::new());

        self.serialize(&mut serializer).unwrap();

        buf
    }
}

/// Change envelope. Carries signed changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Envelope {
    /// Changes included in this envelope, serialized as JSON.
    pub changes: Vec<u8>,
    /// Signature over the change, by the change author.
    pub signature: Signature,
}

/// An object that can be either present or removed.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Redactable<T> {
    /// When the object is present.
    Present(T),
    /// When the object has been removed.
    #[default]
    Redacted,
}

/// A comment on a discussion thread.
#[derive(Default, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Comment {
    /// The comment body.
    body: String,
    /// Thread or comment this is a reply to.
    reply_to: Option<ChangeId>,
}

impl Comment {
    /// Create a new comment.
    pub fn new(body: String, reply_to: Option<ChangeId>) -> Self {
        Self { body, reply_to }
    }
}

/// An action that can be carried out in a change.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub enum Action {
    /// Comment on a thread.
    Comment { comment: Comment },
    /// Redact a change. Not all changes can be redacted.
    Redact { id: ChangeId },
    /// Add a tag to the thread.
    Tag { tag: TagId },
    /// Remove a tag from the thread.
    Untag { tag: TagId },
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
    comments: BTreeMap<ChangeId, Redactable<Comment>>,
    /// Associated tags.
    tags: BTreeMap<TagId, LWWReg<bool, Timestamp>>,
    /// Reactions to changes.
    reactions: BTreeMap<ChangeId, LWWSet<(ActorId, Reaction), Timestamp>>,
}

impl Deref for Thread {
    type Target = BTreeMap<ChangeId, Redactable<Comment>>;

    fn deref(&self) -> &Self::Target {
        &self.comments
    }
}

impl Thread {
    pub fn clear(&mut self) {
        self.comments.clear();
    }

    pub fn apply(&mut self, changes: impl IntoIterator<Item = Change>) {
        for change in changes.into_iter() {
            let id = change.id();

            match change.action {
                Action::Comment { comment } => {
                    match self.comments.get(&id) {
                        Some(Redactable::Present(_)) => {
                            // Do nothing, the action was already processed,
                            // since a change with the same content-id as this
                            // one exists already.
                        }
                        Some(Redactable::Redacted) => {
                            // Do nothing, the action was redacted.
                        }
                        None => {
                            self.comments.insert(id, Redactable::Present(comment));
                        }
                    }
                }
                Action::Redact { id } => {
                    self.comments.insert(id, Redactable::Redacted);
                }
                Action::Tag { tag } => {
                    self.tags
                        .entry(tag)
                        .and_modify(|r| r.set(true, change.timestamp))
                        .or_insert_with(|| LWWReg::new(true, change.timestamp));
                }
                Action::Untag { tag } => {
                    self.tags
                        .entry(tag)
                        .and_modify(|r| r.set(false, change.timestamp))
                        .or_insert_with(|| LWWReg::new(false, change.timestamp));
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
                                reactions.insert((change.author, reaction), change.timestamp);
                            } else {
                                reactions.remove((change.author, reaction), change.timestamp);
                            }
                        })
                        .or_insert_with(|| {
                            if active {
                                LWWSet::singleton((change.author, reaction), change.timestamp)
                            } else {
                                let mut set = LWWSet::default();
                                set.remove((change.author, reaction), change.timestamp);
                                set
                            }
                        });
                }
            }
        }
    }

    pub fn comments(&self) -> impl Iterator<Item = (&ChangeId, &Comment)> + '_ {
        self.comments.iter().filter_map(|(id, comment)| {
            if let Redactable::Present(c) = comment {
                Some((id, c))
            } else {
                None
            }
        })
    }

    pub fn tags(&self) -> impl Iterator<Item = &TagId> + '_ {
        self.tags
            .iter()
            .filter_map(|(tag, r)| if *r.get() { Some(tag) } else { None })
    }
}

/// An object that can be used to create and sign changes.
#[derive(Default)]
pub struct Actor<G> {
    signer: G,
    clock: LClock,
    changes: BTreeMap<(LClock, PublicKey), Change>,
}

impl<G: Signer> Actor<G> {
    pub fn new(signer: G) -> Self {
        Self {
            signer,
            clock: LClock::default(),
            changes: BTreeMap::default(),
        }
    }

    pub fn receive(&mut self, changes: impl IntoIterator<Item = Change>) -> LClock {
        for change in changes {
            let clock = change.clock;

            self.changes.insert((clock, change.author), change);
            self.clock.merge(clock);
        }
        self.clock
    }

    /// Reset actor state to initial state.
    pub fn reset(&mut self) {
        self.changes.clear();
        self.clock = LClock::default();
    }

    /// Create a new thread.
    pub fn thread(&self) -> Thread {
        Thread::default()
    }

    /// Returned an ordered list of events.
    pub fn timeline(&self) -> impl Iterator<Item = &Change> {
        self.changes.values()
    }

    /// Create a new comment.
    pub fn comment(
        &mut self,
        body: &str,
        timestamp: Timestamp,
        parent: Option<ChangeId>,
    ) -> Change {
        self.change(
            Action::Comment {
                comment: Comment::new(String::from(body), parent),
            },
            timestamp,
        )
    }

    /// Add a tag.
    pub fn tag(&mut self, tag: TagId, timestamp: Timestamp) -> Change {
        self.change(Action::Tag { tag }, timestamp)
    }

    /// Remove a tag.
    pub fn untag(&mut self, tag: TagId, timestamp: Timestamp) -> Change {
        self.change(Action::Untag { tag }, timestamp)
    }

    /// Create a new redaction.
    pub fn redact(&mut self, id: ChangeId, timestamp: Timestamp) -> Change {
        self.change(Action::Redact { id }, timestamp)
    }

    /// Create a new change.
    pub fn change(&mut self, action: Action, timestamp: Timestamp) -> Change {
        let author = *self.signer.public_key();
        let clock = self.clock.tick();
        let change = Change {
            action,
            author,
            timestamp,
            clock,
        };
        self.changes.insert((self.clock, author), change.clone());

        change
    }

    pub fn sign(&self, changes: impl IntoIterator<Item = Change>) -> Envelope {
        let changes = changes.into_iter().collect::<Vec<_>>();
        let json = serde_json::to_value(changes).unwrap();

        let mut buffer = Vec::new();
        let mut serializer = serde_json::Serializer::with_formatter(
            &mut buffer,
            olpc_cjson::CanonicalFormatter::new(),
        );
        json.serialize(&mut serializer).unwrap();

        let signature = self.signer.sign(&buffer);

        Envelope {
            changes: buffer,
            signature,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::ops::ControlFlow;
    use std::str::FromStr;
    use std::{array, iter};

    use itertools::Itertools;
    use pretty_assertions::assert_eq;
    use quickcheck::Arbitrary;
    use quickcheck_macros::quickcheck;
    use radicle::{cob::TypeName, crypto::test::signer::MockSigner, identity::project::Identity};

    use super::*;
    use crate::test::WeightedGenerator;

    #[derive(Clone)]
    struct Changes<const N: usize> {
        permutations: [Vec<Change>; N],
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
            let gen = WeightedGenerator::<Action, (Vec<TagId>, Vec<Change>)>::new(rng.clone())
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
                        tags.push(tag.clone());
                        tag
                    } else {
                        tags[rng.usize(..tags.len())].clone()
                    };
                    Some(Action::Tag { tag })
                })
                .variant(2, |(tags, _), rng| {
                    if tags.is_empty() {
                        return None;
                    }
                    let tag = tags[rng.usize(..tags.len())].clone();
                    Some(Action::Untag { tag })
                });

            let mut changes = Vec::new();
            let mut permutations: [Vec<Change>; N] = array::from_fn(|_| Vec::new());
            let mut clock = LClock::default();
            let author = PublicKey::from([0; 32]);

            for action in gen.take(g.size().min(8)) {
                let timestamp = Timestamp::now() + rng.u64(0..3);
                let clock = clock.tick();

                changes.push(Change {
                    action,
                    author,
                    timestamp,
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
        let (_storage, signer, repository) = radicle::test::setup::context(&tmp);
        let mut alice = Actor::new(signer);
        let project = Identity::load(alice.signer.public_key(), &repository).unwrap();
        let timestamp = Timestamp::now();
        let typename = TypeName::from_str("xyz.radicle.thread").unwrap();

        let a1 = alice.comment("First comment", timestamp + 1, None);
        let a2 = alice.comment("Second comment", timestamp + 2, None);

        let mut expected = Thread::default();
        expected.apply([a1.clone(), a2.clone()]);

        let created = radicle::cob::create(
            &repository,
            &alice.signer,
            &project,
            radicle::cob::Create {
                author: None,
                history_type: radicle::cob::HistoryType::default(),
                contents: a1.encode(),
                typename: typename.clone(),
                message: "Thread created".to_owned(),
            },
        )
        .unwrap();

        radicle::cob::update(
            &repository,
            &alice.signer,
            &project,
            radicle::cob::Update {
                author: None,
                history_type: radicle::cob::HistoryType::default(),
                changes: a2.encode(),
                object_id: *created.id(),
                typename: typename.clone(),
                message: "Thread updated".to_owned(),
            },
        )
        .unwrap();

        let retrieved = radicle::cob::get(&repository, &typename, created.id())
            .unwrap()
            .unwrap();

        let actual: Thread = retrieved
            .history()
            .traverse(Thread::default(), |mut acc, entry| {
                let change: Change = serde_json::from_slice(entry.contents()).unwrap();
                acc.apply([change]);

                ControlFlow::Continue(acc)
            });

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_timelines_basic() {
        let mut alice = Actor::<MockSigner>::default();
        let mut bob = Actor::<MockSigner>::default();
        let timestamp = Timestamp::now();

        let a1 = alice.comment("First comment", timestamp + 1, None);
        let a2 = alice.comment("Second comment", timestamp + 2, None);

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
        let timestamp = Timestamp::now();

        let a1 = alice.comment("First comment", timestamp, None);

        bob.receive([a1.clone()]);

        let b0 = bob.comment("Bob's first reply to Alice", timestamp, None);
        let b1 = bob.comment("Bob's second reply to Alice", timestamp, None);

        eve.receive([b1.clone(), b0.clone()]);
        let e0 = eve.comment("Eve's first reply to Alice", timestamp, None);

        bob.receive([e0.clone()]);
        let b2 = bob.comment("Bob's third reply to Alice", timestamp, None);

        eve.receive([b2.clone(), a1.clone()]);
        let e1 = eve.comment("Eve's second reply to Alice", timestamp, None);

        alice.receive([b0.clone(), b1.clone(), b2.clone(), e0.clone(), e1.clone()]);
        bob.receive([e1.clone()]);

        let a2 = alice.comment("Second comment", timestamp, None);
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
    }

    #[test]
    fn test_invariants() {
        let mut alice = Actor::<MockSigner>::default();
        let bob = Actor::<MockSigner>::default();
        let time = Timestamp::now();

        let t = bob.thread();
        let a0 = alice.comment("Ham", time, None);
        let a1 = alice.comment("Rye", time, None);
        let a2 = alice.comment("Dough", time, Some(a1.id()));
        let a3 = alice.redact(a1.id(), time);
        let a4 = alice.comment("Bread", time, None);

        assert_order_invariance(&t, [&a0, &a1, &a2, &a3, &a4]);
        assert_idempotence(&t, [&a0, &a1, &a2, &a3, &a4]);
    }

    fn assert_order_invariance<'a>(t: &Thread, changes: impl IntoIterator<Item = &'a Change>) {
        let changes = changes.into_iter().cloned().collect::<Vec<_>>();
        let count = changes.len();

        let mut actual = t.clone();
        let mut expected = t.clone();
        expected.clear();
        expected.apply(changes.clone());

        for permutation in changes.into_iter().permutations(count) {
            actual.clear();
            actual.apply(permutation);

            assert_eq!(actual, expected);
        }
    }

    fn assert_idempotence<'a>(t: &Thread, changes: impl IntoIterator<Item = &'a Change>) {
        let changes = changes.into_iter().cloned().collect::<Vec<_>>();

        let mut actual = t.clone();
        let mut expected = t.clone();

        expected.clear();
        expected.apply(changes.clone());

        actual.clear();
        actual.apply(changes.clone());
        actual.apply(changes.clone());
        actual.apply(changes);

        assert_eq!(actual, expected);
    }
}

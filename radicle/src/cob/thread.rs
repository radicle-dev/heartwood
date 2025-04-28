use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;

use once_cell::sync::Lazy;
use serde::{ser::SerializeStruct, Deserialize, Serialize};
use thiserror::Error;

use crate::cob;
use crate::cob::common::{Reaction, Timestamp, Uri};
use crate::cob::store::Cob;
use crate::cob::{op, ActorId, Embed, EntryId, Op};
use crate::git;
use crate::prelude::ReadRepository;

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
    /// The identity doc is missing.
    #[error("identity document missing")]
    MissingIdentity,
    /// Error with comment operation.
    #[error("comment {0} is invalid")]
    Comment(EntryId),
    /// Error with edit operation.
    #[error("edit {0} is invalid")]
    Edit(EntryId),
    /// Object initialization failed.
    #[error("initialization failed: {0}")]
    Init(&'static str),
    #[error("op decoding failed: {0}")]
    Op(#[from] op::OpEncodingError),
}

/// Identifies a comment.
pub type CommentId = EntryId;

/// Reactions to a comment or other action.
pub type Reactions = BTreeSet<(ActorId, Reaction)>;

/// A comment edit is just some text and an edit time.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Edit {
    /// Edit author.
    pub author: ActorId,
    /// When the edit was made.
    pub timestamp: Timestamp,
    /// Edit contents. Replaces previous edits.
    pub body: String,
    /// Edit embed list.
    pub embeds: Vec<Embed<Uri>>,
}

impl Edit {
    /// Create a new edit.
    pub fn new(
        author: ActorId,
        body: String,
        timestamp: Timestamp,
        embeds: Vec<Embed<Uri>>,
    ) -> Self {
        Self {
            author,
            timestamp,
            body,
            embeds,
        }
    }
}

/// The `Infallible` type does not have a `Serialize`/`Deserialize`
/// implementation. The `Never` type imitates `Infallible` and
/// provides the derived implementations.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Never {}

/// A comment on a discussion thread.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Comment<T = Never> {
    /// Comment author.
    pub(in crate::cob) author: ActorId,
    /// The comment body.
    pub(in crate::cob) edits: Vec<Edit>,
    /// Reactions to this comment.
    pub(in crate::cob) reactions: Reactions,
    /// Comment this is a reply to.
    /// Should always be set, except for the root comment.
    pub(in crate::cob) reply_to: Option<CommentId>,
    /// Location of comment, if this is an inline comment.
    pub(in crate::cob) location: Option<T>,
    /// Whether the comment has been resolved.
    pub(in crate::cob) resolved: bool,
}

impl<T: Serialize> Serialize for Comment<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        let mut state = serializer.serialize_struct("Comment", 8)?;
        state.serialize_field("author", &self.author())?;
        if let Some(to) = self.reply_to {
            state.serialize_field("replyTo", &to)?;
        }
        state.serialize_field("reactions", &self.reactions)?;
        state.serialize_field("resolved", &self.resolved)?;
        state.serialize_field("body", self.body())?;
        state.serialize_field("edits", &self.edits)?;
        if let Some(location) = self.location() {
            state.serialize_field("location", &location)?;
        }

        let embeds = self.embeds();
        if !embeds.is_empty() {
            state.serialize_field("embeds", self.embeds())?;
        }
        state.end()
    }
}

impl<L> Comment<L> {
    /// Create a new comment.
    pub fn new(
        author: ActorId,
        body: String,
        reply_to: Option<CommentId>,
        location: Option<L>,
        embeds: Vec<Embed<Uri>>,
        timestamp: Timestamp,
    ) -> Self {
        let edit = Edit::new(author, body, timestamp, embeds);

        Self {
            author,
            reactions: BTreeSet::default(),
            edits: vec![edit],
            reply_to,
            location,
            resolved: false,
        }
    }

    /// Get the comment body. If there are multiple edits, gets the value at the latest edit.
    pub fn body(&self) -> &str {
        // SAFETY: There is always at least one edit. This is guaranteed by the [`Comment`]
        // constructor.
        #[allow(clippy::unwrap_used)]
        self.edits.last().unwrap().body.as_str()
    }

    /// Get the comment timestamp, which is the time of the *original* edit. To get the timestamp
    /// of the latest edit, use the [`Comment::edits`] function.
    pub fn timestamp(&self) -> Timestamp {
        // SAFETY: There is always at least one edit. This is guaranteed by the [`Comment`]
        // constructor.
        #[allow(clippy::unwrap_used)]
        self.edits.first().unwrap().timestamp
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
        self.edits.iter()
    }

    /// Add an edit.
    pub fn edit(
        &mut self,
        author: ActorId,
        body: String,
        embeds: Vec<Embed<Uri>>,
        timestamp: Timestamp,
    ) {
        self.edits.push(Edit::new(author, body, timestamp, embeds));
    }

    /// Comment reactions.
    pub fn reactions(&self) -> BTreeMap<&Reaction, Vec<&ActorId>> {
        self.reactions
            .iter()
            .fold(BTreeMap::new(), |mut acc, (author, reaction)| {
                acc.entry(reaction).or_default().push(author);
                acc
            })
    }

    /// Get comment location, if any.
    pub fn location(&self) -> Option<&L> {
        self.location.as_ref()
    }

    /// Get comment resolution status.
    pub fn is_resolved(&self) -> bool {
        self.resolved
    }

    /// Return the embedded media.
    pub fn embeds(&self) -> &[Embed<Uri>] {
        // SAFETY: There is always at least one edit. This is guaranteed by the [`Comment`]
        // constructor.
        #[allow(clippy::unwrap_used)]
        &self.edits.last().unwrap().embeds
    }

    pub fn resolve(&mut self) {
        self.resolved = true;
    }

    pub fn unresolve(&mut self) {
        self.resolved = false;
    }
}

impl<T: PartialOrd> PartialOrd for Comment<T> {
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

impl cob::store::CobAction for Action {}

impl From<Action> for nonempty::NonEmpty<Action> {
    fn from(action: Action) -> Self {
        Self::new(action)
    }
}

/// A discussion thread.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Thread<T = Comment> {
    /// The comments under the thread.
    pub(crate) comments: BTreeMap<CommentId, Option<T>>,
    /// Comment timeline.
    pub(crate) timeline: Vec<CommentId>,
}

impl<T> Default for Thread<T> {
    fn default() -> Self {
        Self {
            comments: BTreeMap::default(),
            timeline: Vec::default(),
        }
    }
}

impl<T> cob::store::CobWithType for Thread<T> {
    fn type_name() -> &'static radicle_cob::TypeName {
        &TYPENAME
    }
}

impl<T> Thread<T> {
    pub fn new(id: CommentId, comment: T) -> Self {
        Self {
            comments: BTreeMap::from_iter([(id, Some(comment))]),
            timeline: vec![id],
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

    pub fn comment(&self, id: &CommentId) -> Option<&T> {
        self.comments.get(id).and_then(|o| o.as_ref())
    }

    pub fn root(&self) -> Option<(&CommentId, &T)> {
        self.first()
    }

    pub fn first(&self) -> Option<(&CommentId, &T)> {
        self.comments().next()
    }

    pub fn last(&self) -> Option<(&CommentId, &T)> {
        self.comments().next_back()
    }

    pub fn comments(&self) -> impl DoubleEndedIterator<Item = (&CommentId, &T)> + '_ {
        self.timeline.iter().filter_map(|id| {
            self.comments
                .get(id)
                .and_then(|o| o.as_ref())
                .map(|comment| (id, comment))
        })
    }

    pub fn timeline(&self) -> impl DoubleEndedIterator<Item = &EntryId> + '_ {
        self.timeline.iter()
    }
}

impl Thread {
    /// Apply a single action to the thread.
    fn action<R: ReadRepository>(
        &mut self,
        action: Action,
        entry: EntryId,
        author: ActorId,
        timestamp: Timestamp,
        _concurrent: &[&cob::Entry],
        _identity: git::Oid,
        _repo: &R,
    ) -> Result<(), Error> {
        match action {
            Action::Comment { body, reply_to } => {
                comment(self, entry, author, timestamp, body, reply_to, None, vec![])?;
            }
            Action::Edit { id, body } => {
                edit(self, entry, author, id, timestamp, body, vec![])?;
            }
            Action::Redact { id } => {
                redact(self, entry, id)?;
            }
            Action::React {
                to,
                reaction,
                active,
            } => {
                react(self, entry, author, to, reaction, active)?;
            }
        }
        Ok(())
    }
}

impl<L> Thread<Comment<L>> {
    pub fn replies<'a>(
        &'a self,
        to: &'a CommentId,
    ) -> impl Iterator<Item = (&'a CommentId, &'a Comment<L>)> {
        self.comments().filter_map(move |(id, c)| {
            if let Some(reply_to) = c.reply_to {
                if &reply_to == to {
                    return Some((id, c));
                }
            }
            None
        })
    }
}

impl cob::store::Cob for Thread {
    type Action = Action;
    type Error = Error;

    fn from_root<R: ReadRepository>(op: Op<Action>, repo: &R) -> Result<Self, Self::Error> {
        let author = op.author;
        let entry = op.id;
        let timestamp = op.timestamp;
        let identity = op.identity.ok_or(Error::MissingIdentity)?;
        let mut actions = op.actions.into_iter();
        let Some(Action::Comment {
            body,
            reply_to: None,
        }) = actions.next()
        else {
            return Err(Error::Init("missing initial comment"));
        };

        let mut thread = Thread::default();
        comment(
            &mut thread,
            entry,
            author,
            timestamp,
            body,
            None,
            None,
            vec![],
        )?;

        for action in actions {
            thread.action(action, entry, author, timestamp, &[], identity, repo)?;
        }
        Ok(thread)
    }

    fn op<'a, R: ReadRepository, I: IntoIterator<Item = &'a cob::Entry>>(
        &mut self,
        op: Op<Action>,
        concurrent: I,
        repo: &R,
    ) -> Result<(), Error> {
        let identity = op.identity.ok_or(Error::MissingIdentity)?;
        let concurrent = concurrent.into_iter().collect::<Vec<_>>();
        for action in op.actions {
            self.action(
                action,
                op.id,
                op.author,
                op.timestamp,
                &concurrent,
                identity,
                repo,
            )?;
        }
        Ok(())
    }
}

impl<R: ReadRepository> cob::Evaluate<R> for Thread {
    type Error = Error;

    fn init(entry: &cob::Entry, repo: &R) -> Result<Self, Self::Error> {
        let op = Op::try_from(entry)?;
        let object = <Thread as Cob>::from_root(op, repo)?;

        Ok(object)
    }

    fn apply<'a, I: Iterator<Item = (&'a EntryId, &'a cob::Entry)>>(
        &mut self,
        entry: &cob::Entry,
        concurrent: I,
        repo: &R,
    ) -> Result<(), Self::Error> {
        let op = Op::try_from(entry)?;

        self.op(op, concurrent.map(|(_, e)| e), repo)
    }
}

pub fn comment<L>(
    thread: &mut Thread<Comment<L>>,
    id: EntryId,
    author: ActorId,
    timestamp: Timestamp,
    body: String,
    reply_to: Option<CommentId>,
    location: Option<L>,
    embeds: Vec<Embed<Uri>>,
) -> Result<(), Error> {
    if body.is_empty() {
        return Err(Error::Comment(id));
    }
    if let Some(id) = reply_to {
        if !thread.comments.contains_key(&id) {
            return Err(Error::Missing(id));
        }
    }
    debug_assert!(!thread.timeline.contains(&id));
    thread.timeline.push(id);

    // Nb. If a comment is already present, it must be redacted, because the
    // underlying store guarantees exactly-once delivery of ops.
    thread.comments.insert(
        id,
        Some(Comment::new(
            author, body, reply_to, location, embeds, timestamp,
        )),
    );

    Ok(())
}

pub fn edit<L>(
    thread: &mut Thread<Comment<L>>,
    id: EntryId,
    author: ActorId,
    comment: EntryId,
    timestamp: Timestamp,
    body: String,
    embeds: Vec<Embed<Uri>>,
) -> Result<(), Error> {
    if body.is_empty() {
        return Err(Error::Edit(id));
    }
    debug_assert!(!thread.timeline.contains(&id));
    thread.timeline.push(id);

    // It's possible for a comment to be redacted before we're able to edit it, in
    // case of a concurrent update.
    //
    // However, it's *not* possible for the comment to be absent. Therefore we treat
    // that as an error.
    if let Some(comment) = thread.comments.get_mut(&comment) {
        if let Some(comment) = comment {
            comment.edit(author, body, embeds, timestamp);
        }
    } else {
        return Err(Error::Missing(comment));
    }
    Ok(())
}

pub fn redact<T>(thread: &mut Thread<T>, id: EntryId, comment: EntryId) -> Result<(), Error> {
    if let Some(comment) = thread.comments.get_mut(&comment) {
        debug_assert!(!thread.timeline.contains(&id));
        thread.timeline.push(id);

        *comment = None;
    } else {
        return Err(Error::Missing(id));
    }
    Ok(())
}

pub fn react<T>(
    thread: &mut Thread<Comment<T>>,
    id: EntryId,
    author: ActorId,
    comment: EntryId,
    reaction: Reaction,
    active: bool,
) -> Result<(), Error> {
    let key = (author, reaction);
    let Some(comment) = thread.comments.get_mut(&comment) else {
        return Err(Error::Missing(comment));
    };
    if let Some(comment) = comment {
        debug_assert!(!thread.timeline.contains(&id));
        thread.timeline.push(id);

        if active {
            comment.reactions.insert(key);
        } else {
            comment.reactions.remove(&key);
        }
    }
    Ok(())
}

pub fn resolve<T>(
    thread: &mut Thread<Comment<T>>,
    id: EntryId,
    comment: EntryId,
) -> Result<(), Error> {
    let Some(comment) = thread.comments.get_mut(&comment) else {
        return Err(Error::Missing(comment));
    };

    if let Some(comment) = comment {
        debug_assert!(!thread.timeline.contains(&id));
        thread.timeline.push(id);
        comment.resolve();
    }
    Ok(())
}

pub fn unresolve<T>(
    thread: &mut Thread<Comment<T>>,
    id: EntryId,
    comment: EntryId,
) -> Result<(), Error> {
    let Some(comment) = thread.comments.get_mut(&comment) else {
        return Err(Error::Missing(comment));
    };

    if let Some(comment) = comment {
        debug_assert!(!thread.timeline.contains(&id));
        thread.timeline.push(id);
        comment.unresolve();
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::ops::{Deref, DerefMut};

    use pretty_assertions::assert_eq;
    use qcheck_macros::quickcheck;

    use super::*;
    use crate as radicle;
    use crate::cob::store::Cob;
    use crate::cob::test;
    use crate::crypto::test::signer::MockSigner;
    use crate::crypto::Signer;
    use crate::profile::env;
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
            self.op::<Thread>([Action::Comment {
                body: String::from(body),
                reply_to,
            }])
        }

        /// Create a new redaction.
        pub fn redact(&mut self, id: CommentId) -> Op<Action> {
            self.op::<Thread>([Action::Redact { id }])
        }

        /// Edit a comment.
        pub fn edit(&mut self, id: CommentId, body: &str) -> Op<Action> {
            self.op::<Thread>([Action::Edit {
                id,
                body: body.to_owned(),
            }])
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

    #[test]
    fn test_redact_comment() {
        let radicle::test::setup::Node { signer, .. } = radicle::test::setup::Node::default();
        let repo = gen::<MockRepository>(1);
        let mut alice = Actor::new(signer);

        let a0 = alice.comment("First comment", None);
        let a1 = alice.comment("Second comment", Some(a0.id()));
        let a2 = alice.comment("Third comment", Some(a0.id()));

        let mut thread = Thread::from_ops([a0, a1.clone(), a2], &repo).unwrap();
        assert_eq!(thread.comments().count(), 3);

        // Redact the second comment.
        let a3 = alice.redact(a1.id());
        thread.op(a3, [], &repo).unwrap();

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

        let t1 = Thread::from_ops([c0.clone(), c1, c2], &repo).unwrap();

        let comment = t1.comment(&c0.id());
        let edits = comment.unwrap().edits().collect::<Vec<_>>();

        assert_eq!(edits[0].body.as_str(), "Hello world!");
        assert_eq!(edits[1].body.as_str(), "Goodbye world.");
        assert_eq!(edits[2].body.as_str(), "Goodbye world!");
        assert_eq!(t1.comment(&c0.id()).unwrap().body(), "Goodbye world!");
    }

    #[test]
    fn test_timeline() {
        let alice = MockSigner::default();
        let bob = MockSigner::default();
        let eve = MockSigner::default();
        let repo = gen::<MockRepository>(1);
        let time = env::local_time();

        let mut a = test::history::<Thread, _>(
            &[Action::Comment {
                body: "Thread root".to_owned(),
                reply_to: None,
            }],
            time.into(),
            &alice,
        );
        a.comment("Alice comment", Some(*a.root().id()), &alice);

        let mut b = a.clone();
        let b1 = b.comment("Bob comment", Some(*a.root().id()), &bob);

        let mut e = a.clone();
        let e1 = e.comment("Eve comment", Some(*a.root().id()), &eve);

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

        let t1 = Thread::from_history(&a, &repo).unwrap();
        let t2 = Thread::from_history(&b, &repo).unwrap();
        let t3 = Thread::from_history(&e, &repo).unwrap();

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
    }

    #[test]
    fn test_duplicate_comments() {
        let repo = gen::<MockRepository>(1);
        let alice = MockSigner::default();
        let bob = MockSigner::default();
        let time = env::local_time();

        let mut a = test::history::<Thread, _>(
            &[Action::Comment {
                body: "Thread root".to_owned(),
                reply_to: None,
            }],
            time.into(),
            &alice,
        );
        let mut b = a.clone();

        a.comment("Hello World!", None, &alice);
        b.comment("Hello World!", None, &bob);

        a.merge(b);

        let thread = Thread::from_history(&a, &repo).unwrap();

        assert_eq!(thread.comments().count(), 3);

        let (first_id, first) = thread.comments().nth(1).unwrap();
        let (second_id, second) = thread.comments().nth(2).unwrap();

        assert!(first_id != second_id); // The ids are not the same,
        assert_eq!(
            first
                .edits
                .iter()
                .map(|e| (&e.body, e.timestamp))
                .collect::<Vec<_>>(),
            second
                .edits
                .iter()
                .map(|e| (&e.body, e.timestamp))
                .collect::<Vec<_>>(),
        ); // despite the content being the same.
    }

    #[quickcheck]
    fn prop_ordering(timestamp: u64) {
        let repo = gen::<MockRepository>(1);
        let alice = MockSigner::default();
        let bob = MockSigner::default();
        let timestamp = Timestamp::from_secs(timestamp);

        let h0 = test::history::<Thread, _>(
            &[Action::Comment {
                body: "Thread root".to_owned(),
                reply_to: None,
            }],
            timestamp,
            &alice,
        );
        let mut h1 = h0.clone();
        let mut h2 = h0.clone();

        let e1 = h1.commit(
            &Action::Edit {
                id: *h0.root().id(),
                body: String::from("Bye World."),
            },
            &alice,
        );
        let e2 = h2.commit(
            &Action::Edit {
                id: *h0.root().id(),
                body: String::from("Hi World."),
            },
            &bob,
        );

        h1.merge(h2);

        let thread = Thread::from_history(&h1, &repo).unwrap();
        let (_, comment) = thread.comments().next().unwrap();

        // E1 and E2 are concurrent, so the final edit will depend on which is the greater hash.
        if e2 > e1 {
            assert_eq!(comment.body(), "Hi World.");
        } else {
            assert_eq!(comment.body(), "Bye World.");
        }

        let _e3 = h1.commit(
            &Action::Edit {
                id: *h0.root().id(),
                body: String::from("Hoho World!"),
            },
            &alice,
        );
        let thread = Thread::from_history(&h1, &repo).unwrap();
        let (_, comment) = thread.comments().next().unwrap();

        // E3 is causally dependent on E1 and E2, so it always wins.
        assert_eq!(comment.body(), "Hoho World!");
    }

    #[test]
    fn test_comment_redact_missing() {
        let repo = gen::<MockRepository>(1);
        let mut alice = Actor::<MockSigner>::default();
        let mut t = Thread::default();
        let id = arbitrary::entry_id();

        t.op(alice.redact(id), [], &repo).unwrap_err();
    }

    #[test]
    fn test_comment_edit_missing() {
        let repo = gen::<MockRepository>(1);
        let mut alice = Actor::<MockSigner>::default();
        let mut t = Thread::default();
        let id = arbitrary::entry_id();

        t.op(alice.edit(id, "Edited"), [], &repo).unwrap_err();
    }

    #[test]
    fn test_comment_edit_redacted() {
        let repo = gen::<MockRepository>(1);
        let mut alice = Actor::<MockSigner>::default();

        let a1 = alice.comment("Hi", None);
        let a2 = alice.redact(a1.id);
        let a3 = alice.edit(a1.id, "Edited");

        let t = Thread::from_ops([a1, a2, a3], &repo).unwrap();
        assert_eq!(t.comments().count(), 0);
    }
}

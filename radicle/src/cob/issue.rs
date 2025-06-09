pub mod cache;

use std::collections::BTreeSet;
use std::ops::Deref;
use std::str::FromStr;
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::cob;
use crate::cob::common::{Author, Authorization, Label, Reaction, Timestamp, Uri};
use crate::cob::store::Transaction;
use crate::cob::store::{Cob, CobAction};
use crate::cob::thread;
use crate::cob::thread::{Comment, CommentId, Thread};
use crate::cob::{op, store, ActorId, Embed, EntryId, ObjectId, TypeName};
use crate::identity::doc::DocError;
use crate::node::device::Device;
use crate::node::NodeId;
use crate::prelude::{Did, Doc, ReadRepository, RepoId};
use crate::storage::{HasRepoId, RepositoryError, WriteRepository};

pub use cache::Cache;

/// Issue operation.
pub type Op = cob::Op<Action>;

/// Type name of an issue.
pub static TYPENAME: LazyLock<TypeName> =
    LazyLock::new(|| FromStr::from_str("xyz.radicle.issue").expect("type name is valid"));

/// Identifier for an issue.
pub type IssueId = ObjectId;

/// Error updating or creating issues.
#[derive(Error, Debug)]
pub enum Error {
    /// Error loading the identity document.
    #[error("identity doc failed to load: {0}")]
    Doc(#[from] DocError),
    #[error("thread apply failed: {0}")]
    Thread(#[from] thread::Error),
    #[error("store: {0}")]
    Store(#[from] store::Error),
    /// Action not authorized.
    #[error("{0} not authorized to apply {1:?}")]
    NotAuthorized(ActorId, Action),
    /// Action not allowed.
    #[error("action is not allowed: {0}")]
    NotAllowed(EntryId),
    /// Title is invalid.
    #[error("invalid title: {0:?}")]
    InvalidTitle(String),
    /// The identity doc is missing.
    #[error("identity document missing")]
    MissingIdentity,
    /// General error initializing an issue.
    #[error("initialization failed: {0}")]
    Init(&'static str),
    /// Error decoding an operation.
    #[error("op decoding failed: {0}")]
    Op(#[from] op::OpEncodingError),
    #[error("failed to update issue {id} in cache: {err}")]
    CacheUpdate {
        id: IssueId,
        #[source]
        err: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
    #[error("failed to remove issue {id} from cache : {err}")]
    CacheRemove {
        id: IssueId,
        #[source]
        err: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
    #[error("failed to remove issues from cache: {err}")]
    CacheRemoveAll {
        #[source]
        err: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
}

/// Reason why an issue was closed.
#[derive(Debug, Clone, Copy, PartialOrd, Ord, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CloseReason {
    Other,
    Solved,
}

impl std::fmt::Display for CloseReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let reason = match self {
            Self::Other => "unspecified",
            Self::Solved => "solved",
        };
        write!(f, "{reason}")
    }
}

/// Issue state.
#[derive(Debug, Default, Clone, Copy, PartialOrd, Ord, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "status")]
pub enum State {
    /// The issue is closed.
    Closed { reason: CloseReason },
    /// The issue is open.
    #[default]
    Open,
}

impl std::fmt::Display for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Closed { .. } => write!(f, "closed"),
            Self::Open => write!(f, "open"),
        }
    }
}

impl State {
    pub fn lifecycle_message(self) -> String {
        match self {
            Self::Open => "Open issue".to_owned(),
            Self::Closed { .. } => "Close issue".to_owned(),
        }
    }
}

/// Issue state. Accumulates [`Action`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    /// Actors assigned to this issue.
    pub(super) assignees: BTreeSet<Did>,
    /// Title of the issue.
    pub(super) title: String,
    /// Current state of the issue.
    pub(super) state: State,
    /// Associated labels.
    pub(super) labels: BTreeSet<Label>,
    /// Discussion around this issue.
    pub(super) thread: Thread,
}

impl cob::store::CobWithType for Issue {
    fn type_name() -> &'static TypeName {
        &TYPENAME
    }
}

impl store::Cob for Issue {
    type Action = Action;
    type Error = Error;

    fn from_root<R: ReadRepository>(op: Op, repo: &R) -> Result<Self, Self::Error> {
        let doc = op.identity_doc(repo)?.ok_or(Error::MissingIdentity)?;
        let mut actions = op.actions.into_iter();
        let Some(Action::Comment {
            body,
            reply_to: None,
            embeds,
        }) = actions.next()
        else {
            return Err(Error::Init("the first action must be of type `comment`"));
        };
        let comment = Comment::new(op.author, body, None, None, embeds, op.timestamp);
        let thread = Thread::new(op.id, comment);
        let mut issue = Issue::new(thread);

        for action in actions {
            match issue.authorization(&action, &op.author, &doc)? {
                Authorization::Allow => {
                    issue.action(action, op.id, op.author, op.timestamp, &[], &doc, repo)?;
                }
                Authorization::Deny => {
                    return Err(Error::NotAuthorized(op.author, action));
                }
                Authorization::Unknown => {
                    // Note that this shouldn't really happen since there's no concurrency in the
                    // root operation.
                    continue;
                }
            }
        }
        Ok(issue)
    }

    fn op<'a, R: ReadRepository, I: IntoIterator<Item = &'a cob::Entry>>(
        &mut self,
        op: Op,
        concurrent: I,
        repo: &R,
    ) -> Result<(), Error> {
        let doc = op.identity_doc(repo)?.ok_or(Error::MissingIdentity)?;
        let concurrent = concurrent.into_iter().collect::<Vec<_>>();

        for action in op.actions {
            log::trace!(target: "issue", "Applying {} {action:?}", op.id);

            if let Err(e) = self.op_action(
                action,
                op.id,
                op.author,
                op.timestamp,
                &concurrent,
                &doc,
                repo,
            ) {
                log::error!(target: "issue", "Error applying {}: {e}", op.id);
                return Err(e);
            }
        }
        Ok(())
    }
}

impl<R: ReadRepository> cob::Evaluate<R> for Issue {
    type Error = Error;

    fn init(entry: &cob::Entry, repo: &R) -> Result<Self, Self::Error> {
        let op = Op::try_from(entry)?;
        let object = Issue::from_root(op, repo)?;

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

impl Issue {
    /// Construct a new issue.
    pub fn new(thread: Thread) -> Self {
        Self {
            assignees: BTreeSet::default(),
            title: String::default(),
            state: State::default(),
            labels: BTreeSet::default(),
            thread,
        }
    }

    pub fn assignees(&self) -> impl Iterator<Item = &Did> + '_ {
        self.assignees.iter()
    }

    pub fn title(&self) -> &str {
        self.title.as_str()
    }

    pub fn state(&self) -> &State {
        &self.state
    }

    pub fn labels(&self) -> impl Iterator<Item = &Label> {
        self.labels.iter()
    }

    pub fn timestamp(&self) -> Timestamp {
        self.thread
            .comments()
            .next()
            .map(|(_, c)| c)
            .expect("Issue::timestamp: at least one comment is present")
            .timestamp()
    }

    pub fn author(&self) -> Author {
        self.thread
            .comments()
            .next()
            .map(|(_, c)| Author::new(c.author()))
            .expect("Issue::author: at least one comment is present")
    }

    pub fn root(&self) -> (&CommentId, &Comment) {
        self.thread
            .comments()
            .next()
            .expect("Issue::root: at least one comment is present")
    }

    pub fn description(&self) -> &str {
        self.thread
            .comments()
            .next()
            .map(|(_, c)| c.body())
            .expect("Issue::description: at least one comment is present")
    }

    pub fn thread(&self) -> &Thread {
        &self.thread
    }

    pub fn comments(&self) -> impl Iterator<Item = (&CommentId, &thread::Comment)> {
        self.thread.comments()
    }

    /// Get replies to a specific comment.
    pub fn replies_to<'a>(
        &'a self,
        to: &'a CommentId,
    ) -> impl Iterator<Item = (&'a CommentId, &'a thread::Comment)> {
        self.thread.replies(to)
    }

    /// Iterate over all top-level replies. Does not include the top-level root comment.
    /// Use [`Issue::comments`] to get all comments including the "root" comment.
    pub fn replies(&self) -> impl Iterator<Item = (&CommentId, &thread::Comment)> {
        self.comments().skip(1)
    }

    /// Apply authorization rules on issue actions.
    pub fn authorization(
        &self,
        action: &Action,
        actor: &ActorId,
        doc: &Doc,
    ) -> Result<Authorization, Error> {
        if doc.is_delegate(&actor.into()) {
            // A delegate is authorized to do all actions.
            return Ok(Authorization::Allow);
        }
        let author: ActorId = *self.author().id().as_key();
        let outcome = match action {
            // Only delegate can assign someone to an issue.
            Action::Assign { assignees } => {
                if assignees == &self.assignees {
                    // No-op is allowed for backwards compatibility.
                    Authorization::Allow
                } else {
                    Authorization::Deny
                }
            }
            // Issue authors can edit their own issues.
            Action::Edit { .. } => Authorization::from(*actor == author),
            // Issue authors can close or re-open their own issue.
            Action::Lifecycle { state } => Authorization::from(match state {
                State::Closed { .. } => *actor == author,
                State::Open => *actor == author,
            }),
            // Only delegate can label an issue.
            Action::Label { labels } => {
                if labels == &self.labels {
                    // No-op is allowed for backwards compatibility.
                    Authorization::Allow
                } else {
                    Authorization::Deny
                }
            }
            // All roles can comment on an issues
            Action::Comment { .. } => Authorization::Allow,
            // All roles can edit or redact their own comments.
            Action::CommentEdit { id, .. } | Action::CommentRedact { id, .. } => {
                if let Some(comment) = self.thread.comments.get(id) {
                    if let Some(comment) = comment {
                        Authorization::from(*actor == comment.author())
                    } else {
                        Authorization::Unknown
                    }
                } else {
                    return Err(Error::Thread(thread::Error::Missing(*id)));
                }
            }
            // All roles can react to a comment on an issue.
            Action::CommentReact { .. } => Authorization::Allow,
        };
        Ok(outcome)
    }
}

impl Issue {
    fn op_action<R: ReadRepository>(
        &mut self,
        action: Action,
        id: EntryId,
        author: ActorId,
        timestamp: Timestamp,
        concurrent: &[&cob::Entry],
        doc: &Doc,
        repo: &R,
    ) -> Result<(), Error> {
        match self.authorization(&action, &author, doc)? {
            Authorization::Allow => {
                self.action(action, id, author, timestamp, concurrent, doc, repo)
            }
            Authorization::Deny => Err(Error::NotAuthorized(author, action)),
            Authorization::Unknown => Ok(()),
        }
    }

    /// Apply a single action to the issue.
    fn action<R: ReadRepository>(
        &mut self,
        action: Action,
        entry: EntryId,
        author: ActorId,
        timestamp: Timestamp,
        _concurrent: &[&cob::Entry],
        _doc: &Doc,
        _repo: &R,
    ) -> Result<(), Error> {
        match action {
            Action::Assign { assignees } => {
                self.assignees = BTreeSet::from_iter(assignees);
            }
            Action::Edit { title } => {
                if title.contains('\n') || title.contains('\r') {
                    return Err(Error::InvalidTitle(title));
                }
                self.title = title;
            }
            Action::Lifecycle { state } => {
                self.state = state;
            }
            Action::Label { labels } => {
                self.labels = BTreeSet::from_iter(labels);
            }
            Action::Comment {
                body,
                reply_to,
                embeds,
            } => {
                thread::comment(
                    &mut self.thread,
                    entry,
                    author,
                    timestamp,
                    body,
                    reply_to,
                    None,
                    embeds,
                )?;
            }
            Action::CommentEdit { id, body, embeds } => {
                thread::edit(&mut self.thread, entry, author, id, timestamp, body, embeds)?;
            }
            Action::CommentRedact { id } => {
                let (root, _) = self.root();
                if id == *root {
                    return Err(Error::NotAllowed(entry));
                }
                thread::redact(&mut self.thread, entry, id)?;
            }
            Action::CommentReact {
                id,
                reaction,
                active,
            } => {
                thread::react(&mut self.thread, entry, author, id, reaction, active)?;
            }
        }
        Ok(())
    }
}

impl<'a, 'g, R, C> From<IssueMut<'a, 'g, R, C>> for (IssueId, Issue) {
    fn from(value: IssueMut<'a, 'g, R, C>) -> Self {
        (value.id, value.issue)
    }
}

impl Deref for Issue {
    type Target = Thread;

    fn deref(&self) -> &Self::Target {
        &self.thread
    }
}

impl<R: ReadRepository> store::Transaction<Issue, R> {
    /// Assign DIDs to the issue.
    pub fn assign(&mut self, assignees: impl IntoIterator<Item = Did>) -> Result<(), store::Error> {
        self.push(Action::Assign {
            assignees: assignees.into_iter().collect(),
        })
    }

    /// Edit an issue comment.
    pub fn edit_comment(
        &mut self,
        id: CommentId,
        body: impl ToString,
        embeds: Vec<Embed<Uri>>,
    ) -> Result<(), store::Error> {
        self.embed(embeds.clone())?;
        self.push(Action::CommentEdit {
            id,
            body: body.to_string(),
            embeds,
        })
    }

    /// Set the issue title.
    pub fn edit(&mut self, title: impl ToString) -> Result<(), store::Error> {
        self.push(Action::Edit {
            title: title.to_string(),
        })
    }

    /// Redact a comment.
    pub fn redact_comment(&mut self, id: CommentId) -> Result<(), store::Error> {
        self.push(Action::CommentRedact { id })
    }

    /// Lifecycle an issue.
    pub fn lifecycle(&mut self, state: State) -> Result<(), store::Error> {
        self.push(Action::Lifecycle { state })
    }

    /// Comment on an issue.
    pub fn comment<S: ToString>(
        &mut self,
        body: S,
        reply_to: CommentId,
        embeds: Vec<Embed<Uri>>,
    ) -> Result<(), store::Error> {
        self.embed(embeds.clone())?;
        self.push(Action::Comment {
            body: body.to_string(),
            reply_to: Some(reply_to),
            embeds,
        })
    }

    /// Label an issue.
    pub fn label(&mut self, labels: impl IntoIterator<Item = Label>) -> Result<(), store::Error> {
        self.push(Action::Label {
            labels: labels.into_iter().collect(),
        })
    }

    /// React to an issue comment.
    pub fn react(
        &mut self,
        id: CommentId,
        reaction: Reaction,
        active: bool,
    ) -> Result<(), store::Error> {
        self.push(Action::CommentReact {
            id,
            reaction,
            active,
        })
    }

    ////////////////////////////////////////////////////////////////////////////////////////////////

    /// Create the issue thread.
    fn thread<S: ToString>(
        &mut self,
        body: S,
        embeds: impl IntoIterator<Item = Embed<Uri>>,
    ) -> Result<(), store::Error> {
        let embeds = embeds.into_iter().collect::<Vec<_>>();

        self.embed(embeds.clone())?;
        self.push(Action::Comment {
            body: body.to_string(),
            reply_to: None,
            embeds,
        })
    }
}

pub struct IssueMut<'a, 'g, R, C> {
    id: ObjectId,
    issue: Issue,
    store: &'g mut Issues<'a, R>,
    cache: &'g mut C,
}

impl<R, C> std::fmt::Debug for IssueMut<'_, '_, R, C> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("IssueMut")
            .field("id", &self.id)
            .field("issue", &self.issue)
            .finish()
    }
}

impl<R, C> IssueMut<'_, '_, R, C>
where
    R: WriteRepository + cob::Store<Namespace = NodeId>,
    C: cob::cache::Update<Issue>,
{
    /// Reload the issue data from storage.
    pub fn reload(&mut self) -> Result<(), store::Error> {
        self.issue = self
            .store
            .get(&self.id)?
            .ok_or_else(|| store::Error::NotFound(TYPENAME.clone(), self.id))?;

        Ok(())
    }

    /// Get the issue id.
    pub fn id(&self) -> &ObjectId {
        &self.id
    }

    /// Assign one or more actors to an issue.
    pub fn assign<G>(
        &mut self,
        assignees: impl IntoIterator<Item = Did>,
        signer: &Device<G>,
    ) -> Result<EntryId, Error>
    where
        G: crypto::signature::Signer<crypto::Signature>,
    {
        self.transaction("Assign", signer, |tx| tx.assign(assignees))
    }

    /// Set the issue title.
    pub fn edit<G>(&mut self, title: impl ToString, signer: &Device<G>) -> Result<EntryId, Error>
    where
        G: crypto::signature::Signer<crypto::Signature>,
    {
        self.transaction("Edit", signer, |tx| tx.edit(title))
    }

    /// Set the issue description.
    pub fn edit_description<G>(
        &mut self,
        description: impl ToString,
        embeds: impl IntoIterator<Item = Embed<Uri>>,
        signer: &Device<G>,
    ) -> Result<EntryId, Error>
    where
        G: crypto::signature::Signer<crypto::Signature>,
    {
        let (id, _) = self.root();
        let id = *id;
        self.transaction("Edit description", signer, |tx| {
            tx.edit_comment(id, description, embeds.into_iter().collect())
        })
    }

    /// Lifecycle an issue.
    pub fn lifecycle<G>(&mut self, state: State, signer: &Device<G>) -> Result<EntryId, Error>
    where
        G: crypto::signature::Signer<crypto::Signature>,
    {
        self.transaction("Lifecycle", signer, |tx| tx.lifecycle(state))
    }

    /// Comment on an issue.
    pub fn comment<G, S>(
        &mut self,
        body: S,
        reply_to: CommentId,
        embeds: impl IntoIterator<Item = Embed<Uri>>,
        signer: &Device<G>,
    ) -> Result<EntryId, Error>
    where
        G: crypto::signature::Signer<crypto::Signature>,
        S: ToString,
    {
        self.transaction("Comment", signer, |tx| {
            tx.comment(body, reply_to, embeds.into_iter().collect())
        })
    }

    /// Edit a comment.
    pub fn edit_comment<G, S>(
        &mut self,
        id: CommentId,
        body: S,
        embeds: impl IntoIterator<Item = Embed<Uri>>,
        signer: &Device<G>,
    ) -> Result<EntryId, Error>
    where
        G: crypto::signature::Signer<crypto::Signature>,
        S: ToString,
    {
        self.transaction("Edit comment", signer, |tx| {
            tx.edit_comment(id, body, embeds.into_iter().collect())
        })
    }

    /// Redact a comment.
    pub fn redact_comment<G>(&mut self, id: CommentId, signer: &Device<G>) -> Result<EntryId, Error>
    where
        G: crypto::signature::Signer<crypto::Signature>,
    {
        self.transaction("Redact comment", signer, |tx| tx.redact_comment(id))
    }

    /// Label an issue.
    pub fn label<G>(
        &mut self,
        labels: impl IntoIterator<Item = Label>,
        signer: &Device<G>,
    ) -> Result<EntryId, Error>
    where
        G: crypto::signature::Signer<crypto::Signature>,
    {
        self.transaction("Label", signer, |tx| tx.label(labels))
    }

    /// React to an issue comment.
    pub fn react<G>(
        &mut self,
        to: CommentId,
        reaction: Reaction,
        active: bool,
        signer: &Device<G>,
    ) -> Result<EntryId, Error>
    where
        G: crypto::signature::Signer<crypto::Signature>,
    {
        self.transaction("React", signer, |tx| tx.react(to, reaction, active))
    }

    pub fn transaction<G, F>(
        &mut self,
        message: &str,
        signer: &Device<G>,
        operations: F,
    ) -> Result<EntryId, Error>
    where
        G: crypto::signature::Signer<crypto::Signature>,
        F: FnOnce(&mut Transaction<Issue, R>) -> Result<(), store::Error>,
    {
        let mut tx = Transaction::default();
        operations(&mut tx)?;

        let (issue, commit) = tx.commit(message, self.id, &mut self.store.raw, signer)?;
        self.cache
            .update(&self.store.as_ref().id(), &self.id, &issue)
            .map_err(|e| Error::CacheUpdate {
                id: self.id,
                err: e.into(),
            })?;
        self.issue = issue;

        Ok(commit)
    }
}

impl<R, C> Deref for IssueMut<'_, '_, R, C> {
    type Target = Issue;

    fn deref(&self) -> &Self::Target {
        &self.issue
    }
}

pub struct Issues<'a, R> {
    raw: store::Store<'a, Issue, R>,
}

impl<'a, R> Deref for Issues<'a, R> {
    type Target = store::Store<'a, Issue, R>;

    fn deref(&self) -> &Self::Target {
        &self.raw
    }
}

impl<R> HasRepoId for Issues<'_, R>
where
    R: ReadRepository,
{
    fn rid(&self) -> RepoId {
        self.raw.as_ref().id()
    }
}

/// Detailed information on issue states
#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueCounts {
    pub open: usize,
    pub closed: usize,
}

impl IssueCounts {
    /// Total count.
    pub fn total(&self) -> usize {
        self.open + self.closed
    }
}

impl<'a, R> Issues<'a, R>
where
    R: ReadRepository + cob::Store<Namespace = NodeId>,
{
    /// Open an issues store.
    pub fn open(repository: &'a R) -> Result<Self, RepositoryError> {
        let identity = repository.identity_head()?;
        let raw = store::Store::open(repository)?.identity(identity);

        Ok(Self { raw })
    }
}

impl<'a, R> Issues<'a, R>
where
    R: WriteRepository + cob::Store<Namespace = NodeId>,
{
    /// Create a new issue.
    pub fn create<'g, G, C>(
        &'g mut self,
        title: impl ToString,
        description: impl ToString,
        labels: &[Label],
        assignees: &[Did],
        embeds: impl IntoIterator<Item = Embed<Uri>>,
        cache: &'g mut C,
        signer: &Device<G>,
    ) -> Result<IssueMut<'a, 'g, R, C>, Error>
    where
        G: crypto::signature::Signer<crypto::Signature>,
        C: cob::cache::Update<Issue>,
    {
        let (id, issue) = Transaction::initial("Create issue", &mut self.raw, signer, |tx, _| {
            tx.thread(description, embeds)?;
            tx.edit(title)?;

            if !assignees.is_empty() {
                tx.assign(assignees.to_owned())?;
            }
            if !labels.is_empty() {
                tx.label(labels.to_owned())?;
            }
            Ok(())
        })?;
        cache
            .update(&self.raw.as_ref().id(), &id, &issue)
            .map_err(|e| Error::CacheUpdate { id, err: e.into() })?;

        Ok(IssueMut {
            id,
            issue,
            store: self,
            cache,
        })
    }

    /// Remove an issue.
    pub fn remove<C, G>(&self, id: &ObjectId, signer: &Device<G>) -> Result<(), store::Error>
    where
        C: cob::cache::Remove<Issue>,
        G: crypto::signature::Signer<crypto::Signature>,
    {
        self.raw.remove(id, signer)
    }
}

impl<'a, R> Issues<'a, R>
where
    R: ReadRepository + cob::Store,
{
    /// Get an issue.
    pub fn get(&self, id: &ObjectId) -> Result<Option<Issue>, store::Error> {
        self.raw.get(id)
    }

    /// Get an issue mutably.
    pub fn get_mut<'g, C>(
        &'g mut self,
        id: &ObjectId,
        cache: &'g mut C,
    ) -> Result<IssueMut<'a, 'g, R, C>, store::Error> {
        let issue = self
            .raw
            .get(id)?
            .ok_or_else(move || store::Error::NotFound(TYPENAME.clone(), *id))?;

        Ok(IssueMut {
            id: *id,
            issue,
            store: self,
            cache,
        })
    }

    /// Issues count by state.
    pub fn counts(&self) -> Result<IssueCounts, Error> {
        let all = self.all()?;
        let state_groups =
            all.filter_map(|s| s.ok())
                .fold(IssueCounts::default(), |mut state, (_, p)| {
                    match p.state() {
                        State::Open => state.open += 1,
                        State::Closed { .. } => state.closed += 1,
                    }
                    state
                });

        Ok(state_groups)
    }
}

/// Issue action.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Action {
    /// Assign issue to an actor.
    #[serde(rename = "assign")]
    Assign { assignees: BTreeSet<Did> },

    /// Edit issue title.
    #[serde(rename = "edit")]
    Edit { title: String },

    /// Transition to a different state.
    #[serde(rename = "lifecycle")]
    Lifecycle { state: State },

    /// Modify issue labels.
    #[serde(rename = "label")]
    Label { labels: BTreeSet<Label> },

    /// Comment on a thread.
    #[serde(rename_all = "camelCase")]
    #[serde(rename = "comment")]
    Comment {
        /// Comment body.
        body: String,
        /// Comment this is a reply to.
        /// Should be [`None`] if it's the top-level comment.
        /// Should be the root [`CommentId`] if it's a top-level comment.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reply_to: Option<CommentId>,
        /// Embeded content.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        embeds: Vec<Embed<Uri>>,
    },

    /// Edit a comment.
    #[serde(rename = "comment.edit")]
    CommentEdit {
        /// Comment being edited.
        id: CommentId,
        /// New value for the comment body.
        body: String,
        /// New value for the embeds list.
        embeds: Vec<Embed<Uri>>,
    },

    /// Redact a change. Not all changes can be redacted.
    #[serde(rename = "comment.redact")]
    CommentRedact { id: CommentId },

    /// React to a comment.
    #[serde(rename = "comment.react")]
    CommentReact {
        id: CommentId,
        reaction: Reaction,
        active: bool,
    },
}

impl CobAction for Action {
    fn produces_identifier(&self) -> bool {
        matches!(self, Self::Comment { .. })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod test {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::cob::{store::CobWithType, ActorId, Reaction};
    use crate::git::Oid;
    use crate::issue::cache::Issues as _;
    use crate::test::arbitrary;
    use crate::{assert_matches, test};

    #[test]
    fn test_concurrency() {
        let t = test::setup::Network::default();
        let mut issues_alice = Cache::no_cache(&*t.alice.repo).unwrap();
        let mut bob_issues = Cache::no_cache(&*t.bob.repo).unwrap();
        let mut eve_issues = Cache::no_cache(&*t.eve.repo).unwrap();
        let mut issue_alice = issues_alice
            .create(
                "Alice Issue",
                "Alice's comment",
                &[],
                &[],
                [],
                &t.alice.signer,
            )
            .unwrap();
        let id = *issue_alice.id();

        t.bob.repo.fetch(&t.alice);
        t.eve.repo.fetch(&t.alice);

        let mut issue_eve = eve_issues.get_mut(&id).unwrap();
        let mut issue_bob = bob_issues.get_mut(&id).unwrap();

        issue_bob
            .comment("Bob's reply", *id, vec![], &t.bob.signer)
            .unwrap();
        issue_alice
            .comment("Alice's reply", *id, vec![], &t.alice.signer)
            .unwrap();

        assert_eq!(issue_bob.comments().count(), 2);
        assert_eq!(issue_alice.comments().count(), 2);

        t.bob.repo.fetch(&t.alice);
        issue_bob.reload().unwrap();
        assert_eq!(issue_bob.comments().count(), 3);

        t.alice.repo.fetch(&t.bob);
        issue_alice.reload().unwrap();
        assert_eq!(issue_alice.comments().count(), 3);

        let bob_comments = issue_bob
            .comments()
            .map(|(_, c)| c.body())
            .collect::<Vec<_>>();
        let alice_comments = issue_alice
            .comments()
            .map(|(_, c)| c.body())
            .collect::<Vec<_>>();

        assert_eq!(bob_comments, alice_comments);

        t.eve.repo.fetch(&t.alice);

        let eve_reply = issue_eve
            .comment("Eve's reply", *id, vec![], &t.eve.signer)
            .unwrap();

        t.bob.repo.fetch(&t.eve);
        t.alice.repo.fetch(&t.eve);

        issue_alice.reload().unwrap();
        issue_bob.reload().unwrap();
        issue_eve.reload().unwrap();

        assert_eq!(issue_eve.comments().count(), 4);
        assert_eq!(issue_bob.comments().count(), 4);
        assert_eq!(issue_alice.comments().count(), 4);

        let (first, _) = issue_bob.comments().next().unwrap();
        let (last, _) = issue_bob.comments().last().unwrap();

        assert_eq!(*first, *issue_alice.id);
        assert_eq!(*last, eve_reply);
    }

    #[test]
    fn test_ordering() {
        assert!(CloseReason::Solved > CloseReason::Other);
        assert!(
            State::Open
                > State::Closed {
                    reason: CloseReason::Solved
                }
        );
    }

    #[test]
    fn test_issue_create_and_assign() {
        let test::setup::NodeWithRepo { node, repo, .. } = test::setup::NodeWithRepo::default();
        let mut issues = Cache::no_cache(&*repo).unwrap();

        let assignee = Did::from(arbitrary::gen::<ActorId>(1));
        let assignee_two = Did::from(arbitrary::gen::<ActorId>(1));
        let issue = issues
            .create(
                "My first issue",
                "Blah blah blah.",
                &[],
                &[assignee],
                [],
                &node.signer,
            )
            .unwrap();

        let id = issue.id;
        let issue = issues.get(&id).unwrap().unwrap();
        let assignees: Vec<_> = issue.assignees().cloned().collect::<Vec<_>>();

        assert_eq!(1, assignees.len());
        assert!(assignees.contains(&assignee));

        let mut issue = issues.get_mut(&id).unwrap();
        issue
            .assign([assignee, assignee_two], &node.signer)
            .unwrap();

        let id = issue.id;
        let issue = issues.get(&id).unwrap().unwrap();
        let assignees: Vec<_> = issue.assignees().cloned().collect::<Vec<_>>();

        assert_eq!(2, assignees.len());
        assert!(assignees.contains(&assignee));
        assert!(assignees.contains(&assignee_two));
    }

    #[test]
    fn test_issue_create_and_reassign() {
        let test::setup::NodeWithRepo { node, repo, .. } = test::setup::NodeWithRepo::default();
        let mut issues = Cache::no_cache(&*repo).unwrap();

        let assignee = Did::from(arbitrary::gen::<ActorId>(1));
        let assignee_two = Did::from(arbitrary::gen::<ActorId>(1));
        let mut issue = issues
            .create(
                "My first issue",
                "Blah blah blah.",
                &[],
                &[assignee, assignee_two],
                [],
                &node.signer,
            )
            .unwrap();

        issue.assign([assignee_two], &node.signer).unwrap();
        issue.assign([assignee_two], &node.signer).unwrap();
        issue.reload().unwrap();

        let assignees: Vec<_> = issue.assignees().cloned().collect::<Vec<_>>();

        assert_eq!(1, assignees.len());
        assert!(assignees.contains(&assignee_two));

        issue.assign([], &node.signer).unwrap();
        issue.reload().unwrap();

        assert_eq!(0, issue.assignees().count());
    }

    #[test]
    fn test_issue_create_and_get() {
        let test::setup::NodeWithRepo { node, repo, .. } = test::setup::NodeWithRepo::default();
        let mut issues = Cache::no_cache(&*repo).unwrap();
        let created = issues
            .create(
                "My first issue",
                "Blah blah blah.",
                &[],
                &[],
                [],
                &node.signer,
            )
            .unwrap();

        let (id, created) = (created.id, created.issue);
        let issue = issues.get(&id).unwrap().unwrap();

        assert_eq!(created, issue);
        assert_eq!(issue.title(), "My first issue");
        assert_eq!(issue.author().id, Did::from(node.signer.public_key()));
        assert_eq!(issue.description(), "Blah blah blah.");
        assert_eq!(issue.comments().count(), 1);
        assert_eq!(issue.state(), &State::Open);
    }

    #[test]
    fn test_issue_create_and_change_state() {
        let test::setup::NodeWithRepo { node, repo, .. } = test::setup::NodeWithRepo::default();
        let mut issues = Cache::no_cache(&*repo).unwrap();
        let mut issue = issues
            .create(
                "My first issue",
                "Blah blah blah.",
                &[],
                &[],
                [],
                &node.signer,
            )
            .unwrap();

        issue
            .lifecycle(
                State::Closed {
                    reason: CloseReason::Other,
                },
                &node.signer,
            )
            .unwrap();

        let id = issue.id;
        let mut issue = issues.get_mut(&id).unwrap();

        assert_eq!(
            *issue.state(),
            State::Closed {
                reason: CloseReason::Other
            }
        );

        issue.lifecycle(State::Open, &node.signer).unwrap();
        let issue = issues.get(&id).unwrap().unwrap();

        assert_eq!(*issue.state(), State::Open);
    }

    #[test]
    fn test_issue_create_and_unassign() {
        let test::setup::NodeWithRepo { node, repo, .. } = test::setup::NodeWithRepo::default();
        let mut issues = Cache::no_cache(&*repo).unwrap();

        let assignee = Did::from(arbitrary::gen::<ActorId>(1));
        let assignee_two = Did::from(arbitrary::gen::<ActorId>(1));
        let mut issue = issues
            .create(
                "My first issue",
                "Blah blah blah.",
                &[],
                &[assignee, assignee_two],
                [],
                &node.signer,
            )
            .unwrap();
        assert_eq!(2, issue.assignees().count());

        issue.assign([assignee_two], &node.signer).unwrap();
        issue.reload().unwrap();

        let assignees: Vec<_> = issue.assignees().cloned().collect::<Vec<_>>();

        assert_eq!(1, assignees.len());
        assert!(assignees.contains(&assignee_two));
    }

    #[test]
    fn test_issue_edit() {
        let test::setup::NodeWithRepo { node, repo, .. } = test::setup::NodeWithRepo::default();
        let mut issues = Cache::no_cache(&*repo).unwrap();

        let mut issue = issues
            .create(
                "My first issue",
                "Blah blah blah.",
                &[],
                &[],
                [],
                &node.signer,
            )
            .unwrap();

        issue.edit("Sorry typo", &node.signer).unwrap();

        let id = issue.id;
        let issue = issues.get(&id).unwrap().unwrap();
        let r = issue.title();

        assert_eq!(r, "Sorry typo");
    }

    #[test]
    fn test_issue_edit_description() {
        let test::setup::NodeWithRepo { node, repo, .. } = test::setup::NodeWithRepo::default();
        let mut issues = Cache::no_cache(&*repo).unwrap();
        let mut issue = issues
            .create(
                "My first issue",
                "Blah blah blah.",
                &[],
                &[],
                [],
                &node.signer,
            )
            .unwrap();

        issue
            .edit_description("Bob Loblaw law blog", vec![], &node.signer)
            .unwrap();

        let id = issue.id;
        let issue = issues.get(&id).unwrap().unwrap();
        let desc = issue.description();

        assert_eq!(desc, "Bob Loblaw law blog");
    }

    #[test]
    fn test_issue_react() {
        let test::setup::NodeWithRepo { node, repo, .. } = test::setup::NodeWithRepo::default();
        let mut issues = Cache::no_cache(&*repo).unwrap();
        let mut issue = issues
            .create(
                "My first issue",
                "Blah blah blah.",
                &[],
                &[],
                [],
                &node.signer,
            )
            .unwrap();

        let (comment, _) = issue.root();
        let comment = *comment;
        let reaction = Reaction::new('🥳').unwrap();
        issue.react(comment, reaction, true, &node.signer).unwrap();

        let id = issue.id;
        let issue = issues.get(&id).unwrap().unwrap();
        let reactions = issue.comment(&comment).unwrap().reactions();
        let authors = reactions.get(&reaction).unwrap();

        assert_eq!(authors.first().unwrap(), &node.signer.public_key());

        // TODO: Test multiple reactions from same author and different authors
    }

    #[test]
    fn test_issue_reply() {
        let test::setup::NodeWithRepo { node, repo, .. } = test::setup::NodeWithRepo::default();
        let mut issues = Cache::no_cache(&*repo).unwrap();
        let mut issue = issues
            .create(
                "My first issue",
                "Blah blah blah.",
                &[],
                &[],
                [],
                &node.signer,
            )
            .unwrap();
        let (root, _) = issue.root();
        let root = *root;

        let c1 = issue
            .comment("Hi hi hi.", root, vec![], &node.signer)
            .unwrap();
        let c2 = issue
            .comment("Ha ha ha.", root, vec![], &node.signer)
            .unwrap();

        let id = issue.id;
        let mut issue = issues.get_mut(&id).unwrap();
        let (_, reply1) = &issue.replies_to(&root).nth(0).unwrap();
        let (_, reply2) = &issue.replies_to(&root).nth(1).unwrap();

        assert_eq!(reply1.body(), "Hi hi hi.");
        assert_eq!(reply2.body(), "Ha ha ha.");

        issue.comment("Re: Hi.", c1, vec![], &node.signer).unwrap();
        issue.comment("Re: Ha.", c2, vec![], &node.signer).unwrap();
        issue
            .comment("Re: Ha. Ha.", c2, vec![], &node.signer)
            .unwrap();
        issue
            .comment("Re: Ha. Ha. Ha.", c2, vec![], &node.signer)
            .unwrap();

        let issue = issues.get(&id).unwrap().unwrap();

        assert_eq!(issue.replies_to(&c1).nth(0).unwrap().1.body(), "Re: Hi.");
        assert_eq!(issue.replies_to(&c2).nth(0).unwrap().1.body(), "Re: Ha.");
        assert_eq!(
            issue.replies_to(&c2).nth(1).unwrap().1.body(),
            "Re: Ha. Ha."
        );
        assert_eq!(
            issue.replies_to(&c2).nth(2).unwrap().1.body(),
            "Re: Ha. Ha. Ha."
        );
    }

    #[test]
    fn test_issue_label() {
        let test::setup::NodeWithRepo { node, repo, .. } = test::setup::NodeWithRepo::default();
        let mut issues = Cache::no_cache(&*repo).unwrap();
        let bug_label = Label::new("bug").unwrap();
        let ux_label = Label::new("ux").unwrap();
        let wontfix_label = Label::new("wontfix").unwrap();
        let mut issue = issues
            .create(
                "My first issue",
                "Blah blah blah.",
                &[ux_label.clone()],
                &[],
                [],
                &node.signer,
            )
            .unwrap();

        issue
            .label([ux_label.clone(), bug_label.clone()], &node.signer)
            .unwrap();
        issue
            .label(
                [ux_label.clone(), bug_label.clone(), wontfix_label.clone()],
                &node.signer,
            )
            .unwrap();

        let id = issue.id;
        let issue = issues.get(&id).unwrap().unwrap();
        let labels = issue.labels().cloned().collect::<Vec<_>>();

        assert!(labels.contains(&ux_label));
        assert!(labels.contains(&bug_label));
        assert!(labels.contains(&wontfix_label));
    }

    #[test]
    fn test_issue_comment() {
        let test::setup::NodeWithRepo { node, repo, .. } = test::setup::NodeWithRepo::default();
        let author = *node.signer.public_key();
        let mut issues = Cache::no_cache(&*repo).unwrap();
        let mut issue = issues
            .create(
                "My first issue",
                "Blah blah blah.",
                &[],
                &[],
                [],
                &node.signer,
            )
            .unwrap();

        // The root thread op id is always the same.
        let (c0, _) = issue.root();
        let c0 = *c0;

        issue
            .comment("Ho ho ho.", c0, vec![], &node.signer)
            .unwrap();
        issue
            .comment("Ha ha ha.", c0, vec![], &node.signer)
            .unwrap();

        let id = issue.id;
        let issue = issues.get(&id).unwrap().unwrap();
        let (_, c0) = &issue.comments().nth(0).unwrap();
        let (_, c1) = &issue.comments().nth(1).unwrap();
        let (_, c2) = &issue.comments().nth(2).unwrap();

        assert_eq!(c0.body(), "Blah blah blah.");
        assert_eq!(c0.author(), author);
        assert_eq!(c1.body(), "Ho ho ho.");
        assert_eq!(c1.author(), author);
        assert_eq!(c2.body(), "Ha ha ha.");
        assert_eq!(c2.author(), author);
    }

    #[test]
    fn test_issue_comment_redact() {
        let test::setup::NodeWithRepo { node, repo, .. } = test::setup::NodeWithRepo::default();
        let mut issues = Cache::no_cache(&*repo).unwrap();
        let mut issue = issues
            .create(
                "My first issue",
                "Blah blah blah.",
                &[],
                &[],
                [],
                &node.signer,
            )
            .unwrap();

        // The root thread op id is always the same.
        let (c0, _) = issue.root();
        let c0 = *c0;

        let comment = issue
            .comment("Ho ho ho.", c0, vec![], &node.signer)
            .unwrap();
        issue.reload().unwrap();
        assert_eq!(issue.comments().count(), 2);

        issue.redact_comment(comment, &node.signer).unwrap();
        assert_eq!(issue.comments().count(), 1);

        // Can't redact root comment.
        issue.redact_comment(*issue.id, &node.signer).unwrap_err();
    }

    #[test]
    fn test_issue_state_serde() {
        assert_eq!(
            serde_json::to_value(State::Open).unwrap(),
            serde_json::json!({ "status": "open" })
        );

        assert_eq!(
            serde_json::to_value(State::Closed {
                reason: CloseReason::Solved
            })
            .unwrap(),
            serde_json::json!({ "status": "closed", "reason": "solved" })
        );
    }

    #[test]
    fn test_issue_all() {
        let test::setup::NodeWithRepo { node, repo, .. } = test::setup::NodeWithRepo::default();
        let mut issues = Cache::no_cache(&*repo).unwrap();
        issues
            .create("First", "Blah", &[], &[], [], &node.signer)
            .unwrap();
        issues
            .create("Second", "Blah", &[], &[], [], &node.signer)
            .unwrap();
        issues
            .create("Third", "Blah", &[], &[], [], &node.signer)
            .unwrap();

        let issues = issues
            .list()
            .unwrap()
            .map(|r| r.map(|(_, i)| i))
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert_eq!(issues.len(), 3);

        issues.iter().find(|i| i.title() == "First").unwrap();
        issues.iter().find(|i| i.title() == "Second").unwrap();
        issues.iter().find(|i| i.title() == "Third").unwrap();
    }

    #[test]
    fn test_issue_multilines() {
        let test::setup::NodeWithRepo { node, repo, .. } = test::setup::NodeWithRepo::default();
        let mut issues = Cache::no_cache(&*repo).unwrap();
        let created = issues
            .create(
                "My first issue",
                "Blah blah blah.\nYah yah yah",
                &[],
                &[],
                [],
                &node.signer,
            )
            .unwrap();

        let (id, created) = (created.id, created.issue);
        let issue = issues.get(&id).unwrap().unwrap();

        assert_eq!(created, issue);
        assert_eq!(issue.title(), "My first issue");
        assert_eq!(issue.author().id, Did::from(node.signer.public_key()));
        assert_eq!(issue.description(), "Blah blah blah.\nYah yah yah");
        assert_eq!(issue.comments().count(), 1);
        assert_eq!(issue.state(), &State::Open);
    }

    #[test]
    fn test_embeds() {
        let test::setup::NodeWithRepo { node, repo, .. } = test::setup::NodeWithRepo::default();
        let mut issues = Cache::no_cache(&*repo).unwrap();

        let content1 = repo.backend.blob(b"<html>Hello World!</html>").unwrap();
        let content2 = repo.backend.blob(b"<html>Hello Radicle!</html>").unwrap();
        let content3 = repo.backend.blob(b"body { color: red }").unwrap();

        let embed1 = Embed {
            name: String::from("example.html"),
            content: Uri::from(Oid::from(content1)),
        };
        let embed2 = Embed {
            name: String::from("style.css"),
            content: Uri::from(Oid::from(content2)),
        };
        let embed3 = Embed {
            name: String::from("bin"),
            content: Uri::from(Oid::from(content3)),
        };
        let mut issue = issues
            .create(
                "My first issue",
                "Blah blah blah.",
                &[],
                &[],
                [embed1.clone(), embed2.clone()],
                &node.signer,
            )
            .unwrap();

        issue
            .comment(
                "Here's a binary file",
                *issue.id,
                [embed3.clone()],
                &node.signer,
            )
            .unwrap();

        issue.reload().unwrap();

        let (_, c0) = issue.thread().comments().next().unwrap();
        let (_, c1) = issue.thread().comments().next_back().unwrap();

        let e1 = &c0.embeds()[0];
        let e2 = &c0.embeds()[1];
        let e3 = &c1.embeds()[0];

        let b1 = Oid::try_from(&e1.content).unwrap();
        let b2 = Oid::try_from(&e2.content).unwrap();
        let b3 = Oid::try_from(&e3.content).unwrap();

        assert_eq!(b1, Oid::try_from(&embed1.content).unwrap());
        assert_eq!(b2, Oid::try_from(&embed2.content).unwrap());
        assert_eq!(b3, Oid::try_from(&embed3.content).unwrap());
    }

    #[test]
    fn test_embeds_edit() {
        let test::setup::NodeWithRepo { node, repo, .. } = test::setup::NodeWithRepo::default();
        let mut issues = Cache::no_cache(&*repo).unwrap();

        let content1 = repo.backend.blob(b"<html>Hello World!</html>").unwrap();
        let content1_edited = repo.backend.blob(b"<html>Hello Radicle!</html>").unwrap();
        let content2 = repo.backend.blob(b"body { color: red }").unwrap();

        let embed1 = Embed {
            name: String::from("example.html"),
            content: Uri::from(Oid::from(content1)),
        };
        let embed1_edited = Embed {
            name: String::from("style.css"),
            content: Uri::from(Oid::from(content1_edited)),
        };
        let embed2 = Embed {
            name: String::from("bin"),
            content: Uri::from(Oid::from(content2)),
        };
        let mut issue = issues
            .create(
                "My first issue",
                "Blah blah blah.",
                &[],
                &[],
                [embed1, embed2],
                &node.signer,
            )
            .unwrap();

        issue.reload().unwrap();
        issue
            .edit_description("My first issue", [embed1_edited.clone()], &node.signer)
            .unwrap();
        issue.reload().unwrap();

        let (_, c0) = issue.thread().comments().next().unwrap();

        assert_eq!(c0.embeds().len(), 1);

        let e1 = &c0.embeds()[0];
        let b1 = Oid::try_from(&e1.content).unwrap();

        assert_eq!(e1.content, embed1_edited.content);
        assert_eq!(b1, Oid::try_from(&embed1_edited.content).unwrap());
    }

    #[test]
    fn test_invalid_actions() {
        let test::setup::NodeWithRepo { node, repo, .. } = test::setup::NodeWithRepo::default();
        let mut issues = Cache::no_cache(&*repo).unwrap();
        let mut issue = issues
            .create(
                "My first issue",
                "Blah blah blah.",
                &[],
                &[],
                [],
                &node.signer,
            )
            .unwrap();
        let missing = arbitrary::oid();

        issue
            .comment("Invalid", missing, [], &node.signer)
            .unwrap_err();
        assert_eq!(issue.comments().count(), 1);
        issue.reload().unwrap();
        assert_eq!(issue.comments().count(), 1);

        let cob = cob::get::<Issue, _>(&*repo, Issue::type_name(), issue.id())
            .unwrap()
            .unwrap();

        assert_eq!(cob.history().len(), 1);
        assert_eq!(
            cob.history().tips().into_iter().collect::<Vec<_>>(),
            vec![*issue.id]
        );
    }

    #[test]
    fn test_invalid_tx() {
        let test::setup::NodeWithRepo { node, repo, .. } = test::setup::NodeWithRepo::default();
        let mut issues = Cache::no_cache(&*repo).unwrap();
        let mut issue = issues
            .create(
                "My first issue",
                "Blah blah blah.",
                &[],
                &[],
                [],
                &node.signer,
            )
            .unwrap();
        let missing = arbitrary::oid();

        // An invalid comment which points to a missing parent.
        // Even creating it via a transaction will trigger an error.
        let mut tx = Transaction::<Issue, _>::default();
        tx.comment("Invalid comment", missing, vec![]).unwrap();
        tx.commit("Add comment", issue.id, &mut issue.store.raw, &node.signer)
            .unwrap_err();

        issue.reload().unwrap();
        assert_eq!(issue.comments().count(), 1);
    }

    #[test]
    fn test_invalid_tx_reference() {
        let test::setup::NodeWithRepo { node, repo, .. } = test::setup::NodeWithRepo::default();
        let mut issues = Cache::no_cache(&*repo).unwrap();
        let issue = issues
            .create(
                "My first issue",
                "Blah blah blah.",
                &[],
                &[],
                [],
                &node.signer,
            )
            .unwrap();

        // Comments require references, so adding two of them to the same transaction errors.
        let mut tx: Transaction<Issue, test::storage::git::Repository> =
            Transaction::<Issue, _>::default();
        tx.comment("First reply", *issue.id, vec![]).unwrap();
        let err = tx.comment("Second reply", *issue.id, vec![]).unwrap_err();
        assert_matches!(err, cob::store::Error::ClashingIdentifiers(_, _));
    }

    #[test]
    fn test_invalid_cob() {
        use cob::change::Storage as _;
        use cob::object::Storage as _;
        use nonempty::NonEmpty;

        let test::setup::NodeWithRepo { node, repo, .. } = test::setup::NodeWithRepo::default();
        let eve = Device::mock();
        let identity = repo.identity().unwrap().head();
        let missing = arbitrary::oid();
        let type_name = Issue::type_name().clone();
        let mut issues = Cache::no_cache(&*repo).unwrap();
        let mut issue = issues
            .create(
                "My first issue",
                "Blah blah blah.",
                &[],
                &[],
                [],
                &node.signer,
            )
            .unwrap();

        // Initially, there is one node in the DAG.
        let cob = cob::get::<NonEmpty<cob::Entry>, _>(&*repo, &type_name, issue.id())
            .unwrap()
            .unwrap();

        assert_eq!(cob.history.len(), 1);
        assert_eq!(cob.object.len(), 1);

        // We have a valid issue. Now we're going to add an invalid action to it, by bypassing
        // the COB API. We do this using a different key, so that valid actions by
        // our issue author don't overwrite the invalid action, since there is
        // only one ref per COB per user.
        let action = Action::CommentRedact { id: missing };
        let action = cob::store::encoding::encode(action).unwrap();
        let contents = NonEmpty::new(action);
        let invalid = repo
            .store(
                Some(identity),
                vec![],
                &eve,
                cob::change::Template {
                    tips: vec![*issue.id],
                    embeds: vec![],
                    contents: contents.clone(),
                    type_name: type_name.clone(),
                    message: String::from("Add invalid operation"),
                },
            )
            .unwrap();

        repo.update(eve.public_key(), &type_name, &issue.id, &invalid.id)
            .unwrap();

        // If we fetch the COB with its history, *without* trying to interpret it as an issue,
        // we'll see that all entries, including the invalid one are there.
        let cob = cob::get::<NonEmpty<cob::Entry>, _>(&*repo, &type_name, issue.id())
            .unwrap()
            .unwrap();

        assert_eq!(cob.history.len(), 2);
        assert_eq!(cob.object.len(), 2);
        assert_eq!(cob.object.last().contents(), &contents);

        // However, if we try to fetch it as an *issue*, the invalid comment is pruned.
        let cob = cob::get::<Issue, _>(&*repo, &type_name, issue.id())
            .unwrap()
            .unwrap();
        assert_eq!(cob.history.len(), 1);
        assert_eq!(cob.object.comments().count(), 1);
        assert!(cob.object.comment(&issue.id).is_some());

        // Additionally, when adding a *valid* comment, it does not build upon the bad operation.
        issue.reload().unwrap();
        issue
            .comment("Valid comment", *issue.id, vec![], &node.signer)
            .unwrap();
        issue.reload().unwrap();
        assert_eq!(issue.comments().count(), 2);
        assert_eq!(issue.thread.timeline().count(), 2);
        assert_eq!(issue.comments().last().unwrap().1.body(), "Valid comment");

        // The actual DAG contains 3 nodes, but only 2 were loaded as an issue.
        let cob = cob::get::<NonEmpty<cob::Entry>, _>(&*repo, &type_name, issue.id())
            .unwrap()
            .unwrap();

        assert_eq!(cob.history.len(), 3);
        assert_eq!(cob.object.len(), 3);

        // If Eve now writes a valid comment via the `Issue` type, it will overwrite her invalid
        // one, since it won't be loaded as a tip.
        issue
            .comment("Eve's comment", *issue.id, vec![], &eve)
            .unwrap();

        let cob = cob::get::<NonEmpty<cob::Entry>, _>(&*repo, &type_name, issue.id())
            .unwrap()
            .unwrap();

        // There are three nodes still, but they are all valid comments.
        // The invalid comment of Eve was replaced with a valid one.
        assert_eq!(issue.comments().count(), 3);
        assert_eq!(cob.history.len(), 3);
        assert_eq!(cob.object.len(), 3);
    }
}

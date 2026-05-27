use std::collections::BTreeMap;
use std::sync::LazyLock;
use std::{fmt, ops::Deref, str::FromStr};

use crypto::{PublicKey, Signature};
use nonempty::NonEmpty;
use radicle_cob::{Embed, ObjectId, TypeName};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::cob::store::access::WriteAs;
use crate::git;
use crate::git::Oid;
use crate::identity::doc::Doc;
use crate::node::NodeId;
use crate::storage;
use crate::{
    cob,
    cob::{
        ActorId, Timestamp, Uri, op, store,
        store::{Cob, CobAction, Transaction},
    },
    identity::{
        Did,
        doc::{DocError, RepoId},
    },
    storage::{ReadRepository, RepositoryError, WriteRepository},
};

use super::{Author, EntryId};

/// Type name of an identity proposal.
pub static TYPENAME: LazyLock<TypeName> =
    LazyLock::new(|| FromStr::from_str("xyz.radicle.id").expect("type name is valid"));

/// Identity operation.
pub type Op = cob::Op<Action>;

/// Identifier for an identity revision.
pub type RevisionId = EntryId;

pub type IdentityStream<'a> = cob::stream::Stream<'a, Action>;

impl<'a> IdentityStream<'a> {
    pub fn init(identity: ObjectId, store: &'a storage::git::Repository) -> Self {
        let history = cob::stream::CobRange::new(&TYPENAME, &identity);
        Self::new(&store.backend, history, TYPENAME.clone())
    }
}

use creusot_std::{
    logic::{Int, OrdLogic, Seq},
    prelude::*,
};

#[logic(open)]
pub fn sorted_range<T: OrdLogic>(s: Seq<T>, l: Int, u: Int) -> bool {
    pearlite! {
        forall<i, j> l <= i && i < j && j < u ==> s[i] <= s[j]
    }
}

/// Proposal operation.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Action {
    #[serde(rename = "revision")]
    Revision {
        /// Short summary of changes.
        title: cob::Title,
        /// Longer comment on proposed changes.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        description: String,
        /// Blob identifier of the document included in this action as an embed.
        /// Hence, we do not include it as a parent of this action in [`CobAction`].
        blob: Oid,
        /// Parent revision that this revision replaces.
        parent: Option<RevisionId>,
        /// Signature over the revision blob.
        signature: Signature,
    },
    RevisionEdit {
        /// The revision to edit.
        revision: RevisionId,
        /// Short summary of changes.
        title: cob::Title,
        /// Longer comment on proposed changes.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        description: String,
    },
    #[serde(rename = "revision.accept")]
    RevisionAccept {
        revision: RevisionId,
        /// Signature over the blob.
        signature: Signature,
    },
    #[serde(rename = "revision.reject")]
    RevisionReject { revision: RevisionId },
    #[serde(rename = "revision.redact")]
    RevisionRedact { revision: RevisionId },
}

impl CobAction for Action {
    fn produces_identifier(&self) -> bool {
        matches!(self, Self::Revision { .. })
    }
}

/// Error applying an operation onto a state.
#[non_exhaustive]
#[derive(Error, Debug)]
pub enum ApplyError {
    /// Causal dependency missing.
    ///
    /// This error indicates that the operations are not being applied
    /// in causal order, which is a requirement for this CRDT.
    ///
    /// For example, this can occur if an operation references another operation
    /// that hasn't happened yet.
    #[error("causal dependency {0:?} missing")]
    Missing(EntryId),
    /// General error initializing an identity.
    #[error("initialization failed: {0}")]
    Init(&'static str),
    /// Invalid signature over document blob.
    #[error("invalid signature from {0} for blob {1}")]
    InvalidSignature(PublicKey, Oid),
    /// Unauthorized action.
    #[error("not authorized to perform this action")]
    NotAuthorized,
    #[error("parent id is missing from revision")]
    MissingParent,
    #[error("verdict for this revision has already been applied")]
    DuplicateVerdict,
    #[error("revision is in an unexpected state")]
    UnexpectedState,
    #[error("revision has been redacted")]
    Redacted,
    #[error("document does not contain any changes to current identity")]
    DocUnchanged,
    #[error("git: {0}")]
    Git(#[from] git::raw::Error),
    #[error("identity document error: {0}")]
    Doc(#[from] DocError),
    #[error("{author} is not a delegate, and only delegates are allowed to {action}")]
    NonDelegateUnauthorized { author: Did, action: String },
}

impl ApplyError {
    fn non_delegate_unauthorized(author: Did, action: &Action) -> Self {
        let action = match action {
            Action::Revision { .. } => "create a revision",
            Action::RevisionEdit { .. } => "edit a revision",
            Action::RevisionAccept { .. } => "accept a revision",
            Action::RevisionReject { .. } => "reject a revision",
            Action::RevisionRedact { .. } => "redact a revision",
        };
        Self::NonDelegateUnauthorized {
            author,
            action: action.to_string(),
        }
    }
}

/// Error updating or creating proposals.
#[derive(Error, Debug)]
pub enum Error {
    #[error("apply failed: {0}")]
    Apply(#[from] ApplyError),
    #[error("store: {0}")]
    Store(#[from] store::Error),
    #[error("op decoding failed: {0}")]
    Op(#[from] op::OpEncodingError),
    #[error(transparent)]
    Doc(#[from] DocError),
    #[error("revision {0} was not found")]
    NotFound(RevisionId),
}

/// An evolving identity document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Identity {
    /// The canonical identifier for this identity.
    /// This is the object id of the initial document blob.
    pub id: RepoId,
    /// The current revision of the document.
    /// Equal to the head of the identity branch.
    pub current: RevisionId,
    /// The initial revision of the document.
    pub root: RevisionId,

    /// Revisions.
    revisions: BTreeMap<RevisionId, Revision>,
    /// Timeline of events.
    timeline: Vec<EntryId>,
}

impl cob::store::CobWithType for Identity {
    fn type_name() -> &'static TypeName {
        &TYPENAME
    }
}

impl std::ops::Deref for Identity {
    type Target = Revision;

    fn deref(&self) -> &Self::Target {
        self.current()
    }
}

impl Identity {
    pub fn new(revision: Revision) -> Self {
        let root = revision.id;

        Self {
            id: revision.blob.into(),
            root,
            current: root,
            revisions: BTreeMap::from_iter([(root, revision)]),
            timeline: [root].to_vec(),
        }
    }

    pub fn initialize<'a, 'b, Repo, Signer>(
        doc: &Doc,
        store: &'a Repo,
        signer: &'b Signer,
    ) -> Result<IdentityMut<'a, 'b, Repo, Signer>, cob::store::Error>
    where
        Repo: WriteRepository + cob::Store<Namespace = NodeId>,
        Signer: crypto::signature::Keypair<VerifyingKey = crypto::PublicKey>,
        Signer: crypto::signature::Signer<crypto::Signature>,
        Signer: crypto::signature::Signer<crypto::ssh::ExtendedSignature>,
        Signer: crypto::signature::Verifier<crypto::Signature>,
    {
        let mut store = cob::store::Store::open(store, WriteAs::new(signer))?;

        #[allow(clippy::unwrap_used)]
        let title = cob::Title::new("Initial revision").unwrap();

        #[allow(deprecated)]
        let (actions, embeds) = {
            let repo = store.repo();
            let signer = store.signer();
            Transaction::new_revision(title, "", doc, None, repo, signer)?.into_inner()
        };

        let actions = NonEmpty::from_vec(actions)
            .expect("Transaction::initial: transaction must contain at least one action");

        let (id, identity) = store.create("Initialize identity", actions, embeds)?;

        Ok(IdentityMut {
            id,
            identity,
            store,
        })
    }

    pub fn get<Repo>(object: &ObjectId, repo: &Repo) -> Result<Identity, store::Error>
    where
        Repo: ReadRepository + cob::Store,
    {
        use cob::store::CobWithType;

        cob::get::<Self, _>(repo, Self::type_name(), object)
            .map(|r| r.map(|cob| cob.object))?
            .ok_or_else(move || store::Error::NotFound(TYPENAME.clone(), *object))
    }

    /// Get a proposal mutably.
    pub fn get_mut<'a, 'b, Repo, Signer>(
        id: &ObjectId,
        repo: &'a Repo,
        signer: &'b Signer,
    ) -> Result<IdentityMut<'a, 'b, Repo, Signer>, store::Error>
    where
        Repo: WriteRepository + cob::Store<Namespace = NodeId>,
        Signer: crypto::signature::Signer<crypto::Signature>,
    {
        let obj = Self::get(id, repo)?;
        let store = cob::store::Store::open(repo, WriteAs::new(signer))?;

        Ok(IdentityMut {
            id: *id,
            identity: obj,
            store,
        })
    }

    pub fn load<R: ReadRepository + cob::Store>(repo: &R) -> Result<Identity, RepositoryError> {
        let oid = repo.identity_root()?;
        let oid = ObjectId::from(oid);

        Self::get(&oid, repo).map_err(RepositoryError::from)
    }

    pub fn load_mut<'a, 'b, Repo, Signer>(
        repo: &'a Repo,
        signer: &'b Signer,
    ) -> Result<IdentityMut<'a, 'b, Repo, Signer>, RepositoryError>
    where
        Repo: WriteRepository + cob::Store<Namespace = NodeId>,
        Signer: crypto::signature::Signer<crypto::Signature>,
    {
        let oid = repo.identity_root()?;
        let oid = ObjectId::from(oid);

        Self::get_mut(&oid, repo, signer).map_err(RepositoryError::from)
    }
}

impl Identity {
    /// The repository identifier.
    pub fn id(&self) -> RepoId {
        self.id
    }

    /// The current document.
    pub fn doc(&self) -> &Doc {
        &self.current().doc
    }

    /// The current revision.
    pub fn current(&self) -> &Revision {
        self.revision(&self.current)
            .expect("Identity::current: the current revision must always exist")
    }

    /// The initial revision of this identity.
    pub fn root(&self) -> &Revision {
        self.revision(&self.root)
            .expect("Identity::root: the root revision must always exist")
    }

    /// The head of the identity branch. This points to a commit that
    /// contains the current document blob.
    pub fn head(&self) -> Oid {
        self.current
    }

    /// A specific [`Revision`], that may be redacted.
    pub fn revision(&self, revision: &RevisionId) -> Option<&Revision> {
        self.revisions.get(revision)
    }

    /// All the [`Revision`]s that have not been redacted.
    pub fn revisions(&self) -> impl DoubleEndedIterator<Item = &Revision> {
        self.timeline.iter().filter_map(|id| {
            self.revisions
                .get(id)
                .filter(|revision| revision.state != State::Redacted)
        })
    }

    pub fn latest_by(&self, who: &Did) -> Option<&Revision> {
        self.revisions().rev().find(|r| r.author.id() == who)
    }
}

impl store::Cob for Identity {
    type Action = Action;
    type Error = ApplyError;

    fn from_root<R: ReadRepository>(op: Op, repo: &R) -> Result<Self, Self::Error> {
        let mut actions = op.actions.into_iter();
        let Some(Action::Revision {
            title,
            description,
            blob,
            signature,
            parent,
        }) = actions.next()
        else {
            return Err(ApplyError::Init(
                "the first action must be of type `revision`",
            ));
        };
        if parent.is_some() {
            return Err(ApplyError::Init(
                "the initial revision must not have a parent",
            ));
        }
        if actions.next().is_some() {
            return Err(ApplyError::Init(
                "the first operation must contain only one action",
            ));
        }
        let root = Doc::load_at(op.id, repo)?;
        if root.blob != blob {
            return Err(ApplyError::Init("invalid object id specified in revision"));
        }
        if root.blob != *repo.id() {
            return Err(ApplyError::Init(
                "repository root does not match identifier",
            ));
        }
        assert_eq!(root.commit, op.id);

        let founder = root.delegates().first();
        if founder.as_key() != &op.author {
            return Err(ApplyError::Init("delegate does not match committer"));
        }
        // Verify signature against root document. Since there is no previous document,
        // we verify it against itself.
        if root
            .verify_signature(founder, &signature, root.blob)
            .is_err()
        {
            return Err(ApplyError::InvalidSignature(**founder, root.blob));
        }
        let revision = Revision::new(
            root.commit,
            title,
            description,
            op.author.into(),
            root.blob,
            root.doc,
            State::Accepted,
            signature,
            parent,
            op.timestamp,
        );
        Ok(Identity::new(revision))
    }

    fn op<'a, R: ReadRepository, I: IntoIterator<Item = &'a cob::Entry>>(
        &mut self,
        op: Op,
        concurrent: I,
        repo: &R,
    ) -> Result<(), ApplyError> {
        let id = op.id;
        let concurrent = concurrent.into_iter().collect::<Vec<_>>();

        for action in op.actions {
            match self.action(action, id, op.author, op.timestamp, &concurrent, repo) {
                Ok(()) => {}
                // This particular error is returned when there is a mismatch between the expected
                // and the actual state of a revision, which can happen concurrently. Therefore
                // if there are other concurrent ops, it is not fatal and we simply ignore it.
                Err(ApplyError::UnexpectedState) if !concurrent.is_empty() => {}
                // It is not a user error if the revision happens to be redacted by
                // the time this action is processed.
                Err(ApplyError::Redacted) => {}
                Err(other) => return Err(other),
            }
            debug_assert!(!self.timeline.contains(&id));
            self.timeline.push(id);
        }
        Ok(())
    }
}

impl Identity {
    /// Apply a single action to the identity document.
    ///
    /// This function ensures a few things:
    /// * Only delegates can interact with the state.
    /// * There is only ever one accepted revision; this is the "current" revision.
    /// * There can be zero or more active revisions, up to the number of delegates.
    /// * An active revision is one that can be "voted" on.
    /// * Only an active revision can be accepted, rejected or edited.
    fn action<R: ReadRepository>(
        &mut self,
        action: Action,
        entry: EntryId,
        author: ActorId,
        timestamp: Timestamp,
        _concurrent: &[&cob::Entry],
        repo: &R,
    ) -> Result<(), ApplyError> {
        let current = self.current().clone();

        let did = author.into();
        if !current.is_delegate(&did) {
            return Err(ApplyError::non_delegate_unauthorized(did, &action));
        }
        match action {
            Action::RevisionAccept {
                revision,
                signature,
            } => {
                let revision = self.revision_mut(&revision)?;
                match &revision.state {
                    State::Accepted => {
                        log::trace!(
                            "Skipping acceptance of revision {} by {did} because it already is accepted.",
                            revision.id
                        );
                    }
                    State::Rejected => {
                        log::debug!(
                            "Skipping acceptance of revision {} by {did} because it already is rejected.",
                            revision.id
                        );
                    }
                    State::Active => {
                        log::trace!(
                            "Applying acceptance of active revision {} by {did}.",
                            revision.id
                        );
                        revision.accept(author, signature, &current)?;
                        let id = revision.id;
                        self.adopt(id);
                    }
                    State::Redacted => {
                        return Err(ApplyError::Redacted);
                    }
                }
            }
            Action::RevisionReject { revision } => {
                let revision = self.revision_mut(&revision)?;
                match &revision.state {
                    State::Accepted => {
                        log::debug!(
                            "Skipping rejection of revision {} by {did} because it already is accepted.",
                            revision.id
                        );
                    }
                    State::Rejected => {
                        log::trace!(
                            "Skipping rejection of revision {} by {did} because it already is rejected.",
                            revision.id
                        );
                    }
                    State::Active => {
                        log::trace!(
                            "Applying rejection of active revision {} by {did}.",
                            revision.id
                        );
                        revision.reject(author)?;
                    }
                    State::Redacted => {
                        return Err(ApplyError::Redacted);
                    }
                }
            }
            Action::RevisionEdit {
                title,
                description,
                revision,
            } => {
                if revision == self.current {
                    return Err(ApplyError::NotAuthorized);
                }
                let revision = self.revision_mut(&revision)?;
                if !revision.is_active() {
                    log::debug!(
                        "Cannot edit revision {} because it is not active.",
                        revision.id
                    );
                    return Err(ApplyError::UnexpectedState);
                }
                if revision.author.public_key() != &author {
                    log::debug!(
                        "{} cannot edit revision created by {}.",
                        author,
                        revision.author.public_key()
                    );
                    // Since the author never changes, we can safely mark this as invalid.
                    return Err(ApplyError::NotAuthorized);
                }
                assert_eq!(revision.parent, Some(current.id));

                revision.title = title;
                revision.description = description;
            }
            Action::RevisionRedact { revision } => {
                if revision == self.current {
                    log::debug!("Cannot redact current revision {revision}.");
                    return Ok(());
                }
                let revision = self.revision_mut(&revision)?;

                if !revision.is_active() {
                    log::debug!("Cannot redact inactive revision {}.", revision.id);
                    return Ok(());
                }
                if revision.author.public_key() != &author {
                    log::debug!(
                        "{} cannot redact revision created by {}.",
                        author,
                        revision.author.public_key()
                    );
                    // Since the author never changes, we can safely mark this as invalid.
                    return Err(ApplyError::NotAuthorized);
                }
                log::debug!("Redacting revision {}.", revision.id);
                revision.state = State::Redacted;
            }
            Action::Revision {
                title,
                description,
                blob,
                signature,
                parent,
            } => {
                debug_assert_eq!(self.revisions.get(&entry), None, "revision visited twice");

                let doc = repo.blob(blob)?;
                let doc = Doc::from_blob(&doc)?;
                // All revisions but the first one must have a parent.
                let Some(parent) = parent else {
                    return Err(ApplyError::MissingParent);
                };
                let current_oid = self.current;
                let parent = self.revision_mut(&parent)?;
                // We expect the revision to make a change compared to its parent.
                if doc == parent.doc {
                    return Err(ApplyError::DocUnchanged);
                }
                // Verify signature over new blob, using trusted delegates.
                if parent.verify_signature(&author, &signature, blob).is_err() {
                    return Err(ApplyError::InvalidSignature(author, blob));
                }

                // If the parent is already rejected or redacted, this revision is dead on arrival.
                // Furthermore, if the parent is accepted but is NO LONGER the current revision,
                // it means a sibling was already adopted and this is a late-arriving fork.
                let state = match parent.state {
                    State::Rejected | State::Redacted => State::Rejected,
                    State::Accepted if parent.id != current_oid => State::Rejected,
                    _ => State::Active,
                };

                let revision = Revision::new(
                    entry,
                    title,
                    description,
                    author.into(),
                    blob,
                    doc,
                    state,
                    signature,
                    Some(parent.id),
                    timestamp,
                );
                let id = revision.id;

                self.revisions.insert(id, revision);

                if state == State::Active {
                    self.adopt(id);
                }
            }
        }
        Ok(())
    }

    /// Try to adopt an active revision as the current one.
    ///
    /// # Panics
    ///
    /// If the revision with the given ID is not active or lookup from
    /// `self.revisions` returns a revision with a different ID.
    /// If the parent revision of the revision with given ID does not exist.
    fn adopt(&mut self, id: RevisionId) {
        if self.current == id {
            return;
        }

        let Some(candidate) = self.revision(&id) else {
            return;
        };

        // Invariant of this module. The key and ID of value in
        // `self.revisions` must be equal.
        assert_eq!(candidate.id, id);

        // Invariant of this module. This function should only be called on
        // active revisions.
        assert_eq!(candidate.state, State::Active);

        let parent = candidate.parent.expect("revision must have parent");

        if parent != self.current {
            log::debug!(
                "Cannot adopt revision {} because its parent {} is not the current revision {}.",
                id,
                parent,
                self.current
            );
            return;
        }

        let votes = candidate.accepted().count();

        if !self.is_majority(votes) {
            log::trace!(
                "Revision {} has {} votes, but needs {} to be adopted.",
                id,
                votes,
                self.majority()
            );
            return;
        }

        // Reject all sibling revisions and their children.
        let mut reject = Vec::new();

        loop {
            let mut found = false;
            for (revision_id, revision) in self.revisions.iter() {
                let Some(revision_parent) = revision.parent else {
                    continue;
                };

                if (revision_parent == parent || reject.contains(&revision_parent))
                    && !reject.contains(revision_id)
                    && revision.state == State::Active
                    && revision_id != &id
                {
                    log::debug!("Adoption of {} causes {} to be rejected.", id, revision_id);

                    found = true;
                    reject.push(*revision_id);
                }
            }
            if !found {
                break;
            }
        }

        for id in reject {
            if let Some(revision) = self.revisions.get_mut(&id) {
                revision.state = State::Rejected;
            }
        }

        self.current = id;
        self.current_mut().state = State::Accepted;
    }

    /// A specific [`Revision`], mutably.
    ///
    /// # Errors
    ///
    /// Returns `ApplyError::Missing` if the revision is not found.
    fn revision_mut(&mut self, id: &RevisionId) -> Result<&mut Revision, ApplyError> {
        self.revisions.get_mut(id).ok_or(ApplyError::Missing(*id))
    }

    /// The current revision, mutably.
    ///
    /// # Panics
    ///
    /// Panics if the current revision is not found.
    fn current_mut(&mut self) -> &mut Revision {
        let current = self.current;
        self.revision_mut(&current)
            .expect("Identity::current_mut: the current revision must always exist")
    }
}

impl<R: ReadRepository> cob::Evaluate<R> for Identity {
    type Error = Error;

    fn init(entry: &cob::Entry, repo: &R) -> Result<Self, Self::Error> {
        let op = Op::try_from(entry)?;
        let object = Identity::from_root(op, repo)?;

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
            .map_err(Error::Apply)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum Verdict {
    /// An accepting verdict must supply the [`Signature`] over the
    /// new proposed [`Doc`].
    Accept(Signature),
    /// Rejecting the proposed [`Doc`].
    Reject,
}

/// State of a revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum State {
    /// The initial state of any revision.
    ///
    /// If a revision receives a majority of accepting votes, it is adopted and
    /// transitions to [`Self::Accepted`]. Also, all its sibling revisions
    /// transition to [`Self::Rejected`].
    ///
    /// If a revisions receives a majority of rejecting votes,
    /// it transitions to [`Self::Rejected`]. This has no impact on sibling
    /// revisions.
    ///
    /// If a revision is redacted (this can only be done by its authoring
    /// delegate), it transitions to [`Self::Redacted`]. From there, no further
    /// state transitions are possible. This can be viewed as a form of
    /// withdrawal of the revision.
    Active,
    /// The revision was accepted by a majority of delegates.
    /// Accepted revisions cannot be redacted.
    Accepted,
    /// The revision was rejected by a majority of delegates, or
    /// a sibling revision was accepted by a majority of delegates.
    Rejected,
    /// The author decided to redact/withdraw the revision.
    Redacted,
}

impl std::fmt::Display for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Active => write!(f, "active"),
            Self::Accepted => write!(f, "accepted"),
            Self::Rejected => write!(f, "rejected"),
            Self::Redacted => write!(f, "redacted"),
        }
    }
}

/// A new [`Doc`] for an [`Identity`]. The revision can be
/// reviewed by gathering [`Signature`]s for accepting the changes, or
/// rejecting them.
///
/// Once a revision has reached the quorum threshold of the previous
/// [`Identity`] it is then adopted as the current identity.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Revision {
    /// The id of this revision. Points to a commit.
    pub id: RevisionId,
    /// Identity document blob at this revision.
    pub blob: Oid,
    /// Title of the proposal.
    pub title: cob::Title,
    /// State of the revision.
    pub state: State,
    /// Description of the proposal.
    pub description: String,
    /// Author of this proposed revision.
    pub author: Author,
    /// New [`Doc`] that will replace `previous`' document.
    pub doc: Doc,
    /// Physical timestamp of this proposal revision.
    pub timestamp: Timestamp,
    /// Parent revision.
    pub parent: Option<RevisionId>,

    /// Signatures and rejections given by the delegates.
    verdicts: BTreeMap<PublicKey, Verdict>,
}

impl std::ops::Deref for Revision {
    type Target = Doc;

    fn deref(&self) -> &Self::Target {
        &self.doc
    }
}

impl Revision {
    pub fn signatures(&self) -> impl Iterator<Item = (&PublicKey, Signature)> {
        self.verdicts().filter_map(|(key, verdict)| match verdict {
            Verdict::Accept(sig) => Some((key, *sig)),
            Verdict::Reject => None,
        })
    }

    pub fn is_accepted(&self) -> bool {
        matches!(self.state, State::Accepted)
    }

    pub fn is_active(&self) -> bool {
        matches!(self.state, State::Active)
    }

    pub fn verdicts(&self) -> impl Iterator<Item = (&PublicKey, &Verdict)> {
        self.verdicts.iter()
    }

    pub fn accepted(&self) -> impl Iterator<Item = Did> + '_ {
        self.signatures().map(|(key, _)| key.into())
    }

    pub fn rejected(&self) -> impl Iterator<Item = Did> + '_ {
        self.verdicts().filter_map(|(key, v)| match v {
            Verdict::Accept(_) => None,
            Verdict::Reject => Some(key.into()),
        })
    }

    pub fn sign<G: crypto::signature::Signer<crypto::Signature>>(
        &self,
        signer: &G,
    ) -> Result<Signature, DocError> {
        self.doc.signature_of(signer)
    }
}

// Private functions that may not do all the verification. Use with caution.
impl Revision {
    fn new(
        id: RevisionId,
        title: cob::Title,
        description: String,
        author: Author,
        blob: Oid,
        doc: Doc,
        state: State,
        signature: Signature,
        parent: Option<RevisionId>,
        timestamp: Timestamp,
    ) -> Self {
        let verdicts = BTreeMap::from_iter([(*author.public_key(), Verdict::Accept(signature))]);

        Self {
            id,
            title,
            description,
            author,
            blob,
            doc,
            state,
            verdicts,
            parent,
            timestamp,
        }
    }

    fn accept(
        &mut self,
        author: PublicKey,
        signature: Signature,
        current: &Revision,
    ) -> Result<(), ApplyError> {
        // Check that this is a valid signature over the new document blob id.
        if current
            .verify_signature(&author, &signature, self.blob)
            .is_err()
        {
            return Err(ApplyError::InvalidSignature(author, self.blob));
        }
        if self
            .verdicts
            .insert(author, Verdict::Accept(signature))
            .is_some()
        {
            return Err(ApplyError::DuplicateVerdict);
        }
        Ok(())
    }

    fn reject(&mut self, key: PublicKey) -> Result<(), ApplyError> {
        if self.verdicts.insert(key, Verdict::Reject).is_some() {
            return Err(ApplyError::DuplicateVerdict);
        }
        // Mark as rejected if it is impossible for this revision to be accepted
        // with the current delegate set.
        if self.is_active() && self.rejected().count() > self.delegates().len() - self.majority() {
            self.state = State::Rejected;
        }
        Ok(())
    }
}

impl<R: ReadRepository> store::Transaction<Identity, R> {
    pub fn accept(
        &mut self,
        revision: RevisionId,
        signature: Signature,
    ) -> Result<(), store::Error> {
        self.push(Action::RevisionAccept {
            revision,
            signature,
        })
    }

    pub fn reject(&mut self, revision: RevisionId) -> Result<(), store::Error> {
        self.push(Action::RevisionReject { revision })
    }

    pub fn edit(
        &mut self,
        revision: RevisionId,
        title: cob::Title,
        description: impl ToString,
    ) -> Result<(), store::Error> {
        self.push(Action::RevisionEdit {
            revision,
            title,
            description: description.to_string(),
        })
    }

    pub fn redact(&mut self, revision: RevisionId) -> Result<(), store::Error> {
        self.push(Action::RevisionRedact { revision })
    }
}

impl<R: WriteRepository> store::Transaction<Identity, R> {
    pub fn new_revision<G: crypto::signature::Signer<crypto::Signature>>(
        title: cob::Title,
        description: impl ToString,
        doc: &Doc,
        parent: Option<RevisionId>,
        repo: &R,
        signer: &G,
    ) -> Result<Self, store::Error> {
        let mut tx = Transaction::default();

        let (blob, bytes, signature) = doc.sign(signer).map_err(store::Error::Identity)?;
        // Store document blob in repository.
        let embed =
            Embed::<Uri>::store("radicle.json", &bytes, repo.raw()).map_err(store::Error::Git)?;

        debug_assert_eq!(embed.content, Uri::from(blob)); // Make sure we pre-computed the correct OID for the blob.

        // Identity document.
        tx.embed([embed])?;

        // Revision metadata.
        tx.push(Action::Revision {
            title,
            description: description.to_string(),
            blob,
            parent,
            signature,
        })?;

        Ok(tx)
    }
}

pub struct IdentityMut<'a, 'b, Repo, Signer> {
    pub id: ObjectId,

    identity: Identity,
    store: store::Store<'a, Identity, Repo, WriteAs<'b, Signer>>,
}

impl<Repo, Signer> fmt::Debug for IdentityMut<'_, '_, Repo, Signer> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IdentityMut")
            .field("id", &self.id)
            .field("identity", &self.identity)
            .finish()
    }
}

impl<Repo, Signer> IdentityMut<'_, '_, Repo, Signer>
where
    Repo: WriteRepository + cob::Store<Namespace = NodeId>,
    Signer: crypto::signature::Keypair<VerifyingKey = crypto::PublicKey>,
    Signer: crypto::signature::Signer<crypto::Signature>,
    Signer: crypto::signature::Signer<crypto::ssh::ExtendedSignature>,
    Signer: crypto::signature::Verifier<crypto::Signature>,
{
    /// Reload the identity data from storage.
    #[cfg(test)]
    pub fn reload(&mut self) -> Result<(), store::Error> {
        self.identity = self
            .store
            .get(&self.id)?
            .ok_or_else(|| store::Error::NotFound(TYPENAME.clone(), self.id))?;

        Ok(())
    }

    pub fn transaction<F>(&mut self, message: &str, operations: F) -> Result<EntryId, Error>
    where
        F: FnOnce(&mut Transaction<Identity, Repo>, &Repo) -> Result<(), store::Error>,
    {
        let mut tx = Transaction::default();
        operations(&mut tx, self.store.as_ref())?;

        let (doc, commit) = tx.commit(message, self.id, &mut self.store)?;
        self.identity = doc;

        Ok(commit)
    }

    /// Update the identity by proposing a new revision.
    /// If the signer is the only delegate, the revision is accepted automatically.
    pub fn update(
        &mut self,
        title: cob::Title,
        description: impl ToString,
        doc: &Doc,
    ) -> Result<RevisionId, Error> {
        let parent = Some(self.current);

        #[allow(deprecated)]
        let tx = {
            let signer = self.store.signer();
            let repo = self.store.repo();
            Transaction::new_revision(title, description, doc, parent, repo, signer)?
        };
        let (doc, commit) = tx.commit("Propose revision", self.id, &mut self.store)?;
        self.identity = doc;

        Ok(commit)
    }

    /// Accept an active revision.
    pub fn accept(&mut self, revision: &RevisionId) -> Result<EntryId, Error> {
        let id = *revision;
        let revision = self.revision(revision).ok_or(Error::NotFound(id))?;

        #[allow(deprecated)]
        let signature = revision.sign(self.store.signer())?;

        self.transaction("Accept revision", |tx, _| tx.accept(id, signature))
    }

    /// Reject an active revision.
    pub fn reject(&mut self, revision: RevisionId) -> Result<EntryId, Error> {
        self.transaction("Reject revision", |tx, _| tx.reject(revision))
    }

    /// Redact a revision.
    pub fn redact(&mut self, revision: RevisionId) -> Result<EntryId, Error> {
        self.transaction("Redact revision", |tx, _| tx.redact(revision))
    }

    /// Edit an active revision's title or description.
    pub fn edit(
        &mut self,
        revision: RevisionId,
        title: cob::Title,
        description: String,
    ) -> Result<EntryId, Error> {
        self.transaction("Edit revision", |tx, _| {
            tx.edit(revision, title, description)
        })
    }
}

impl<Repo, Signer> Deref for IdentityMut<'_, '_, Repo, Signer> {
    type Target = Identity;

    fn deref(&self) -> &Self::Target {
        &self.identity
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod test {
    use qcheck_macros::quickcheck;

    use crate::cob::{self, Title};
    use crate::crypto::PublicKey;
    use crate::identity::Visibility;
    use crate::identity::did::Did;
    use crate::identity::doc::PayloadId;
    use crate::node::device::Device;
    use crate::rad;
    use crate::storage::ReadStorage;
    use crate::storage::git::Storage;
    use crate::test::fixtures;
    use crate::test::setup::{Network, NodeWithRepo};

    use super::*;

    #[quickcheck]
    fn prop_json_eq_str(pk: PublicKey, proj: RepoId, did: Did) {
        let json = serde_json::to_string(&pk).unwrap();
        assert_eq!(format!("\"{pk}\""), json);

        let json = serde_json::to_string(&proj).unwrap();
        assert_eq!(format!("\"{}\"", proj.urn()), json);

        let json = serde_json::to_string(&did).unwrap();
        assert_eq!(format!("\"{did}\""), json);
    }

    #[test]
    fn test_identity_updates() {
        let NodeWithRepo { node, repo } = NodeWithRepo::default();
        let bob = Device::mock();
        let signer = &node.signer;
        let mut identity = Identity::load_mut(&*repo, signer).unwrap();
        let mut doc = identity.doc().clone().edit();
        let title = Title::new("Identity update").unwrap();
        let description = "";
        let r0 = identity.current;

        // The initial state is accepted.
        assert!(identity.current().is_accepted());
        // Using an identical document to the current one fails.
        identity
            .update(title.clone(), description, &doc.clone().verified().unwrap())
            .unwrap_err();
        assert_eq!(identity.current, r0);

        // Change threshold to `2`, even though there's only one delegate. This should
        // fail as it makes the master branch immutable.
        doc.threshold = 2;
        assert!(doc.clone().verified().is_err());

        // Let's add another delegate.
        doc.delegate(bob.public_key().into());
        // The update should go through now.
        let r1 = identity
            .update(title.clone(), description, &doc.clone().verified().unwrap())
            .unwrap();
        assert!(identity.revision(&r1).unwrap().is_accepted());
        assert_eq!(identity.current, r1);
        // With two delegates now, we need two signatures for any update to go through.
        // So this next update shouldn't be accepted as canonical until the second delegate
        // signs it.
        doc.visibility = Visibility::private([]);
        let r2 = identity
            .update(title.clone(), description, &doc.clone().verified().unwrap())
            .unwrap();
        // R1 is still the head.
        assert_eq!(identity.current, r1);
        assert_eq!(identity.revision(&r2).unwrap().state, State::Active);
        assert_eq!(repo.canonical_identity_head().unwrap(), r1);
        assert_eq!(
            repo.identity_doc().unwrap().visibility(),
            &Visibility::Public
        );
        // Now let's add a signature on R2 from Bob.
        let mut bob_identity = Identity::load_mut(&*repo, &bob).unwrap();
        bob_identity.accept(&r2).unwrap();

        identity.reload().unwrap();

        // R2 is now the head.
        assert_eq!(identity.current, r2);
        assert_eq!(identity.revision(&r2).unwrap().state, State::Accepted);
        assert_eq!(repo.canonical_identity_head().unwrap(), r2);
        assert_eq!(
            repo.canonical_identity_doc().unwrap().visibility(),
            &Visibility::private([])
        );
    }

    #[test]
    fn test_identity_update_rejected() {
        let NodeWithRepo { node, repo } = NodeWithRepo::default();
        let bob = Device::mock();
        let eve = Device::mock();
        let signer = &node.signer;

        let mut identity = Identity::load_mut(&*repo, signer).unwrap();
        let mut doc = identity.doc().clone().edit();
        let description = "";

        // Let's add another delegate.
        doc.delegate(bob.public_key().into());
        let r1 = identity
            .update(
                cob::Title::new("Identity update").unwrap(),
                description,
                &doc.clone().verified().unwrap(),
            )
            .unwrap();
        assert_eq!(identity.current, r1);

        doc.visibility = Visibility::private([]);
        let r2 = identity
            .update(
                cob::Title::new("Make private").unwrap(),
                description,
                &doc.clone().verified().unwrap(),
            )
            .unwrap();

        let mut bob_identity = Identity::load_mut(&*repo, &bob).unwrap();

        // 1/2 rejected means that we can never reach the required 2/2 votes.
        bob_identity.reject(r2).unwrap();
        let r2 = bob_identity.revision(&r2).unwrap();
        assert_eq!(r2.state, State::Rejected);

        // Now let's add another delegate.
        doc.delegate(eve.public_key().into());
        let r3 = identity
            .update(
                cob::Title::new("Add Eve").unwrap(),
                description,
                &doc.clone().verified().unwrap(),
            )
            .unwrap();

        bob_identity.reload().unwrap();
        let _ = bob_identity.accept(&r3).unwrap();

        identity.reload().unwrap();
        assert_eq!(identity.current, r3);

        doc.visibility = Visibility::Public;
        let r3 = identity
            .update(
                cob::Title::new("Make public").unwrap(),
                description,
                &doc.verified().unwrap(),
            )
            .unwrap();

        // 1/3 rejected means that we can still reach the 2/3 required votes.
        bob_identity.reject(r3).unwrap();
        let r3 = identity.revision(&r3).unwrap().clone();
        assert_eq!(r3.state, State::Active); // Still active.

        let mut eve_identity = Identity::load_mut(&*repo, &eve).unwrap();

        // 2/3 rejected means that we can no longer reach the 2/3 required votes.
        eve_identity.reject(r3.id).unwrap();
        let r3 = eve_identity.revision(&r3.id).unwrap();
        assert_eq!(r3.state, State::Rejected);
    }

    #[test]
    fn test_identity_updates_concurrent() {
        let network = Network::default();
        let alice = &network.alice;
        let bob = &network.bob;

        let mut alice_identity = Identity::load_mut(&*alice.repo, &alice.signer).unwrap();
        let mut alice_doc = alice_identity.doc().clone().edit();

        alice_doc.delegate(bob.signer.public_key().into());
        let a1 = alice_identity
            .update(
                cob::Title::new("Add Bob").unwrap(),
                "",
                &alice_doc.clone().verified().unwrap(),
            )
            .unwrap();

        bob.repo.fetch(alice);

        let bob_identity = Identity::load(&*bob.repo).unwrap();
        let bob_doc = bob_identity.doc().clone();
        assert!(bob_doc.is_delegate(&bob.signer.public_key().into()));

        // Alice changes the document without making Bob aware.
        alice_doc.visibility = Visibility::private([]);
        let a2 = alice_identity
            .update(
                cob::Title::new("Change visibility").unwrap(),
                "",
                &alice_doc.clone().clone().verified().unwrap(),
            )
            .unwrap();

        let bob_identity_mut = Identity::load_mut(&*bob.repo, &bob.signer).unwrap();
        assert_eq!(*bob_identity_mut, bob_identity);
        let mut bob_identity = bob_identity_mut;

        // Bob makes the same change without knowing Alice already did.
        let b1 = bob_identity
            .update(
                cob::Title::new("Make private").unwrap(),
                "",
                &alice_doc.verified().unwrap(),
            )
            .unwrap();

        // Bob gets Alice's data.
        bob.repo.fetch(alice);
        bob_identity.reload().unwrap();
        assert_eq!(bob_identity.current, a1);

        // Alice gets Bob's data.
        // There's not enough votes for either of these proposals to pass.
        alice.repo.fetch(bob);
        alice_identity.reload().unwrap();
        assert_eq!(alice_identity.current, a1);
        assert_eq!(bob_identity.revision(&a2).unwrap().state, State::Active);
        assert_eq!(bob_identity.revision(&b1).unwrap().state, State::Active);

        // Now Bob accepts Alice's proposal. This voids his own.
        bob_identity.accept(&a2).unwrap();
        assert_eq!(bob_identity.current, a2);
        assert_eq!(bob_identity.revision(&a1).unwrap().state, State::Accepted);
        assert_eq!(bob_identity.revision(&a2).unwrap().state, State::Accepted);
        assert_eq!(bob_identity.revision(&b1).unwrap().state, State::Rejected);
    }

    #[test]
    fn test_identity_redact_revision() {
        let network = Network::default();
        let alice = &network.alice;
        let bob = &network.bob;
        let eve = &network.eve;

        let mut alice_identity = Identity::load_mut(&*alice.repo, &alice.signer).unwrap();
        let mut alice_doc = alice_identity.doc().clone().edit();

        alice_doc.delegate(bob.signer.public_key().into());
        let a0 = alice_identity.root;
        let a1 = alice_identity
            .update(
                cob::Title::new("Add Bob").unwrap(),
                "Eh.",
                &alice_doc.clone().clone().verified().unwrap(),
            )
            .unwrap();

        alice_doc.visibility = Visibility::private([eve.signer.public_key().into()]);
        let a2 = alice_identity
            .update(
                cob::Title::new("Change visibility").unwrap(),
                "Eh.",
                &alice_doc.verified().unwrap(),
            )
            .unwrap();

        bob.repo.fetch(alice);
        let a3 = cob::stable::with_advanced_timestamp(|| alice_identity.redact(a2).unwrap());
        assert!(alice_identity.revision(&a1).is_some());
        assert_eq!(alice_identity.timeline, [a0, a1, a2, a3]);

        let mut bob_identity = Identity::load_mut(&*bob.repo, &bob.signer).unwrap();
        let b1 = cob::stable::with_advanced_timestamp(|| bob_identity.accept(&a2).unwrap());

        assert_eq!(bob_identity.timeline, [a0, a1, a2, b1]);
        assert_eq!(bob_identity.revision(&a2).unwrap().state, State::Accepted);
        bob.repo.fetch(alice);
        bob_identity.reload().unwrap();

        assert_eq!(bob_identity.timeline, [a0, a1, a2, a3, b1]);
        assert_eq!(bob_identity.revision(&a2).unwrap().state, State::Redacted);
        assert_eq!(bob_identity.current, a1);
    }

    #[test]
    fn test_identity_redact_parent_cascades() {
        let network = Network::default();
        let alice = &network.alice;
        let bob = &network.bob;

        // Alice adds Bob and sets threshold to 2.
        // (This is immediately accepted because the old threshold is 1).
        let mut alice_identity = Identity::load_mut(&*alice.repo, &alice.signer).unwrap();
        let mut alice_doc = alice_identity.doc().clone().edit();
        alice_doc.delegate(bob.signer.public_key().into());
        alice_doc.threshold = 2;
        let _a1 = alice_identity
            .update(
                cob::Title::new("Add Bob").unwrap(),
                "",
                &alice_doc.verified().unwrap(),
            )
            .unwrap();

        // Alice proposes A2. Since the threshold is now 2, it stays Active.
        let mut alice_doc2 = alice_identity.doc().clone().edit();
        alice_doc2.visibility = Visibility::private([]);
        let a2 = alice_identity
            .update(
                cob::Title::new("A2").unwrap(),
                "",
                &alice_doc2.verified().unwrap(),
            )
            .unwrap();

        // Bob fetches and proposes B1 as a CHILD of A2.
        bob.repo.fetch(alice);
        let mut bob_identity = Identity::load_mut(&*bob.repo, &bob.signer).unwrap();

        let mut bob_doc = bob_identity.doc().clone().edit();
        bob_doc.visibility = Visibility::private([alice.signer.public_key().into()]);

        // We use a manual transaction to force B1 to be a child of the Active A2,
        // rather than the Accepted A1.
        let b1 = bob_identity
            .transaction("B1", |tx, repo| {
                *tx = Transaction::new_revision(
                    cob::Title::new("B1").unwrap(),
                    "",
                    &bob_doc.verified().unwrap(),
                    Some(a2),
                    repo,
                    &bob.signer,
                )?;
                Ok(())
            })
            .unwrap();

        // Alice redacts A2.
        alice_identity.redact(a2).unwrap();

        // Bob fetches Alice's redaction.
        bob.repo.fetch(alice);
        bob_identity.reload().unwrap();

        assert_eq!(bob_identity.revision(&a2).unwrap().state, State::Redacted);
        assert_eq!(bob_identity.revision(&b1).unwrap().state, State::Rejected);
    }

    #[test]
    fn eager_staleness() {
        let network = Network::default();
        let alice = &network.alice;
        let bob = &network.bob;
        let eve = &network.eve;

        let mut alice_identity = Identity::load_mut(&*alice.repo, &alice.signer).unwrap();
        let mut alice_doc = alice_identity.doc().clone().edit();

        alice_doc.delegate(bob.signer.public_key().into());
        alice_doc.delegate(eve.signer.public_key().into());

        let a1 = alice_identity // Change description to change traversal order.
            .update(
                cob::Title::new("Add Bob and Eve").unwrap(),
                "Eh#!",
                &alice_doc.clone().verified().unwrap(),
            )
            .unwrap();

        bob.repo.fetch(alice);
        eve.repo.fetch(alice);

        let mut bob_identity = Identity::load_mut(&*bob.repo, &bob.signer).unwrap();
        assert_eq!(bob_identity.current, a1);

        let mut bob_doc = bob_identity.doc().clone().edit();
        bob_doc.visibility = Visibility::private([]);
        let bob_doc = bob_doc.verified().unwrap();
        let b1 = cob::stable::with_advanced_timestamp(|| {
            bob_identity
                .update(
                    cob::Title::new("Make private").unwrap(),
                    "",
                    &bob_doc.clone(),
                )
                .unwrap()
        });

        // Now, bob is very eager to change the description of the repository,
        // and does not wait for Eve to accept `b1`.

        let bob_doc = bob_doc.clone().edit();
        bob_doc
            .project()
            .unwrap()
            .update(None, Some("New Description".to_string()), None)
            .unwrap();
        let bob_doc = bob_doc.verified().unwrap();
        let b2 = cob::stable::with_advanced_timestamp(|| {
            bob_identity
                .update(cob::Title::new("Change Description").unwrap(), "", &bob_doc)
                .unwrap()
        });

        eve.repo.fetch(bob);

        let mut eve_identity = Identity::load_mut(&*eve.repo, &eve.signer).unwrap();

        // Eve is now ready to accept Bob's revision `b1`, but Bob's newest is `b2`.
        cob::stable::with_advanced_timestamp(|| eve_identity.accept(&b1).unwrap());

        // Now that Eve has accepted `b1`, it should be accepted and thus current.
        assert_eq!(eve_identity.current, b1);
        // The sibling revision `b2` should be rejected.
        assert_eq!(eve_identity.revision(&b2).unwrap().state, State::Rejected);
    }

    #[test]
    fn test_identity_remove_delegate_concurrent() {
        let network = Network::default();
        let alice = &network.alice;
        let bob = &network.bob;
        let eve = &network.eve;

        let mut alice_identity = Identity::load_mut(&*alice.repo, &alice.signer).unwrap();
        let mut alice_doc = alice_identity.doc().clone().edit();

        alice_doc.delegate(bob.signer.public_key().into());
        alice_doc.delegate(eve.signer.public_key().into());
        let a0 = alice_identity.root;
        let a1 = alice_identity // Change description to change traversal order.
            .update(
                cob::Title::new("Add Bob and Eve").unwrap(),
                "Eh#!",
                &alice_doc.clone().verified().unwrap(),
            )
            .unwrap();

        alice_doc.rescind(&eve.signer.public_key().into()).unwrap();
        let a2 = alice_identity
            .update(
                cob::Title::new("Remove Eve").unwrap(),
                "",
                &alice_doc.verified().unwrap(),
            )
            .unwrap();

        bob.repo.fetch(eve);
        bob.repo.fetch(alice);
        eve.repo.fetch(bob);

        let mut bob_identity = Identity::load_mut(&*bob.repo, &bob.signer).unwrap();
        let b1 = cob::stable::with_advanced_timestamp(|| bob_identity.accept(&a2).unwrap());
        assert_eq!(bob_identity.current, a2);

        let mut eve_identity = Identity::load_mut(&*eve.repo, &eve.signer).unwrap();
        let mut eve_doc = eve_identity.doc().clone().edit();
        eve_doc.visibility = Visibility::private([eve.signer.public_key().into()]);
        let e1 = cob::stable::with_advanced_timestamp(|| {
            eve_identity
                .update(
                    cob::Title::new("Change visibility").unwrap(),
                    "",
                    &eve_doc.verified().unwrap(),
                )
                .unwrap()
        });
        // Eve's revision is active.
        assert_eq!(eve_identity.timeline, [a0, a1, a2, e1]);
        assert!(eve_identity.revision(&e1).unwrap().is_active());

        //  b1      (Accept "Remove Eve") 2/2
        //  |  e1   (Change visibility)
        //  | /
        //  a2      (Propose "Remove Eve") 1/2
        //  |
        //  a1      (Add Bob and Eve)
        //  |
        //  a0

        eve.repo.fetch(bob);
        eve_identity.reload().unwrap();
        // Now that Eve reloaded, since Bob's vote to remove Eve went through first (b1 < e1),
        // her revision is no longer valid.
        assert_eq!(eve_identity.timeline, [a0, a1, a2, b1]);
        assert_eq!(eve_identity.revision(&e1), None);
        assert!(!eve_identity.is_delegate(&eve.signer.public_key().into()));
    }

    #[test]
    fn test_identity_reject_concurrent() {
        let network = Network::default();
        let alice = &network.alice;
        let bob = &network.bob;
        let eve = &network.eve;

        let mut alice_identity = Identity::load_mut(&*alice.repo, &alice.signer).unwrap();
        let mut alice_doc = alice_identity.doc().clone().edit();

        alice_doc.delegate(bob.signer.public_key().into());
        alice_doc.delegate(eve.signer.public_key().into());
        let a0 = alice_identity.root;
        let a1 = alice_identity
            .update(
                cob::Title::new("Add Bob and Eve").unwrap(),
                "Eh!#",
                &alice_doc.clone().verified().unwrap(),
            )
            .unwrap();

        alice_doc.visibility = Visibility::private([]);
        let a2 = alice_identity
            .update(
                cob::Title::new("Change visibility").unwrap(),
                "",
                &alice_doc.verified().unwrap(),
            )
            .unwrap();

        bob.repo.fetch(eve);
        bob.repo.fetch(alice);
        eve.repo.fetch(bob);

        // Bob accepts alice's revision.
        let mut bob_identity = Identity::load_mut(&*bob.repo, &bob.signer).unwrap();
        let b1 = cob::stable::with_advanced_timestamp(|| bob_identity.accept(&a2).unwrap());

        // Eve rejects the revision, not knowing.
        let mut eve_identity = Identity::load_mut(&*eve.repo, &eve.signer).unwrap();
        let e1 = cob::stable::with_advanced_timestamp(|| eve_identity.reject(a2).unwrap());
        assert!(eve_identity.revision(&a2).unwrap().is_active());

        // Then she submits a new revision.
        let mut eve_doc = eve_identity.doc().clone().edit();
        eve_doc.visibility = Visibility::private([eve.signer.public_key().into()]);
        let e2 = eve_identity
            .update(
                cob::Title::new("Change visibility").unwrap(),
                "",
                &eve_doc.verified().unwrap(),
            )
            .unwrap();

        let eve_revision = eve_identity.revision(&e2).unwrap();
        assert_eq!(eve_revision.state, State::Active);
        assert_eq!(eve_revision.parent, Some(a1));

        //     e2   (Propose "Change visibility") 1/2
        //     |
        //     e1   (Reject "Change visibility")  1/2
        //  b1 |    (Accept "Change visibility")  2/2
        //  | /
        //  a2      (Propose "Change visibility") 1/2
        //  |
        //  a1      (Add Bob and Eve)
        //  |
        //  a0

        // Though the rules are that you cannot reject an already accepted revision,
        // since this update was done concurrently there was no way of knowing. Therefore,
        // an error shouldn't be returned. We simply ignore the rejection.

        eve.repo.fetch(bob);
        eve_identity.reload().unwrap();
        assert_eq!(eve_identity.timeline, [a0, a1, a2, b1, e1, e2]);

        // Her revision is there, but rejected, since a sibling revision was already accepted.
        let e2 = eve_identity.revision(&e2).unwrap();
        assert_eq!(e2.state, State::Rejected);
        assert!(eve_identity.revision(&a2).unwrap().is_accepted());
    }

    #[test]
    fn test_identity_updates_concurrent_outdated() {
        let network = Network::default();
        let alice = &network.alice;
        let bob = &network.bob;
        let eve = &network.eve;

        let mut alice_identity = Identity::load_mut(&*alice.repo, &alice.signer).unwrap();
        let mut alice_doc = alice_identity.doc().clone().edit();

        alice.repo.fetch(bob);
        alice.repo.fetch(eve);
        alice_doc.delegate(bob.signer.public_key().into());
        alice_doc.delegate(eve.signer.public_key().into());
        let a0 = alice_identity.root;
        let a1 = alice_identity
            .update(
                cob::Title::new("Add Bob and Eve").unwrap(),
                "",
                &alice_doc.verified().unwrap(),
            )
            .unwrap();

        bob.repo.fetch(alice);
        eve.repo.fetch(alice);

        let mut bob_identity = Identity::load_mut(&*bob.repo, &bob.signer).unwrap();
        let mut bob_doc = bob_identity.doc().clone().edit();
        assert!(bob_doc.is_delegate(&bob.signer.public_key().into()));

        //  a2 e1
        //  | /
        //  b1
        //  |
        //  a1
        //  |
        //  a0

        // Bob and Alice change the document visibility. Eve is not aware.
        bob_doc.visibility = Visibility::private([]);
        let b1 = bob_identity
            .update(
                cob::Title::new("Change visibility #1").unwrap(),
                "",
                &bob_doc.verified().unwrap(),
            )
            .unwrap();

        alice.repo.fetch(bob);
        eve.repo.fetch(bob);

        // In the meantime, Eve does the same thing on her side.
        let mut eve_identity = Identity::load_mut(&*eve.repo, &eve.signer).unwrap();
        let mut eve_doc = eve_identity.doc().clone().edit();
        eve_doc.visibility = Visibility::private([]);
        let e1 = eve_identity
            .update(
                cob::Title::new("Change visibility #2").unwrap(),
                "Woops",
                &eve_doc.verified().unwrap(),
            )
            .unwrap();
        assert_eq!(eve_identity.revisions().count(), 4);
        assert_eq!(eve_identity.revision(&e1).unwrap().state, State::Active);

        alice_identity.reload().unwrap();
        let a2 = cob::stable::with_advanced_timestamp(|| alice_identity.accept(&b1).unwrap());

        eve.repo.fetch(alice);

        eve_identity.reload().unwrap();

        assert_eq!(eve_identity.timeline, [a0, a1, b1, e1, a2]);
        assert_eq!(eve_identity.revision(&e1).unwrap().state, State::Rejected);
    }

    #[test]
    fn test_identity_cascading_rejections() {
        let network = Network::default();
        let alice = &network.alice;
        let bob = &network.bob;
        let eve = &network.eve;

        let mut alice_identity = Identity::load_mut(&*alice.repo, &alice.signer).unwrap();
        let mut alice_doc = alice_identity.doc().clone().edit();
        alice_doc.delegate(bob.signer.public_key().into());
        alice_doc.delegate(eve.signer.public_key().into());
        alice_doc.threshold = 2;
        let _a1 = alice_identity
            .update(
                cob::Title::new("Add Bob and Eve").unwrap(),
                "",
                &alice_doc.verified().unwrap(),
            )
            .unwrap();

        bob.repo.fetch(alice);
        eve.repo.fetch(alice);

        let mut bob_identity = Identity::load_mut(&*bob.repo, &bob.signer).unwrap();
        let mut bob_doc = bob_identity.doc().clone().edit();
        bob_doc.visibility = Visibility::private([]);
        let b1 = bob_identity
            .update(
                cob::Title::new("B1").unwrap(),
                "",
                &bob_doc.clone().verified().unwrap(),
            )
            .unwrap();

        let mut bob_doc2 = bob_doc.clone();
        bob_doc2.threshold = 3;
        let b2 = bob_identity
            .update(
                cob::Title::new("B2").unwrap(),
                "",
                &bob_doc2.verified().unwrap(),
            )
            .unwrap();

        let mut eve_identity = Identity::load_mut(&*eve.repo, &eve.signer).unwrap();
        let mut eve_doc = eve_identity.doc().clone().edit();
        eve_doc.threshold = 3;
        let e1 = eve_identity
            .update(
                cob::Title::new("E1").unwrap(),
                "",
                &eve_doc.verified().unwrap(),
            )
            .unwrap();

        alice.repo.fetch(eve);
        alice_identity.reload().unwrap();
        alice_identity.accept(&e1).unwrap();

        eve.repo.fetch(bob);
        eve.repo.fetch(alice);
        eve_identity.reload().unwrap();

        //     b2   (Propose "B2")
        //     |
        //     b1   (Propose "B1")
        //  e1 |    (Propose "E1") 2/2 (Accepted)
        //  | /
        //  a1      (Add Bob and Eve)
        //  |
        //  a0

        assert_eq!(eve_identity.current, e1);
        assert_eq!(eve_identity.revision(&e1).unwrap().state, State::Accepted);
        assert_eq!(eve_identity.revision(&b1).unwrap().state, State::Rejected);
        assert_eq!(eve_identity.revision(&b2).unwrap().state, State::Rejected);

        alice.repo.fetch(bob);
        bob.repo.fetch(alice);
        bob.repo.fetch(eve);

        alice_identity.reload().unwrap();
        bob_identity.reload().unwrap();

        assert_eq!(alice_identity.current, e1);
        assert_eq!(alice_identity.revision(&b1).unwrap().state, State::Rejected);
        assert_eq!(alice_identity.revision(&b2).unwrap().state, State::Rejected);

        assert_eq!(bob_identity.current, e1);
        assert_eq!(bob_identity.revision(&b1).unwrap().state, State::Rejected);
        assert_eq!(bob_identity.revision(&b2).unwrap().state, State::Rejected);
    }

    #[test]
    fn test_identity_terminal_states_concurrent() {
        let network = Network::default();
        let alice = &network.alice;
        let bob = &network.bob;
        let eve = &network.eve;

        let mut alice_identity = Identity::load_mut(&*alice.repo, &alice.signer).unwrap();
        let mut alice_doc = alice_identity.doc().clone().edit();
        alice_doc.delegate(bob.signer.public_key().into());
        alice_doc.delegate(eve.signer.public_key().into());
        alice_doc.threshold = 2;
        let a1 = alice_identity
            .update(
                cob::Title::new("Add Bob and Eve").unwrap(),
                "",
                &alice_doc.verified().unwrap(),
            )
            .unwrap();

        bob.repo.fetch(alice);
        eve.repo.fetch(alice);

        let mut bob_identity = Identity::load_mut(&*bob.repo, &bob.signer).unwrap();
        let mut eve_identity = Identity::load_mut(&*eve.repo, &eve.signer).unwrap();

        bob_identity.accept(&a1).unwrap();
        eve_identity.accept(&a1).unwrap();

        alice.repo.fetch(bob);
        alice_identity.reload().unwrap();
        assert_eq!(alice_identity.revision(&a1).unwrap().state, State::Accepted);

        alice.repo.fetch(eve);
        alice_identity.reload().unwrap();
        assert_eq!(alice_identity.revision(&a1).unwrap().state, State::Accepted);

        let mut alice_doc2 = alice_identity.doc().clone().edit();
        alice_doc2.visibility = Visibility::private([]);
        let a2 = alice_identity
            .update(
                cob::Title::new("A2").unwrap(),
                "",
                &alice_doc2.verified().unwrap(),
            )
            .unwrap();

        bob.repo.fetch(alice);
        eve.repo.fetch(alice);
        bob_identity.reload().unwrap();
        eve_identity.reload().unwrap();

        bob_identity.reject(a2).unwrap();
        eve_identity.reject(a2).unwrap();

        alice.repo.fetch(bob);
        alice.repo.fetch(eve);
        alice_identity.reload().unwrap();
        assert_eq!(alice_identity.revision(&a2).unwrap().state, State::Rejected);

        //  a2      (Propose "A2") 1/2 (Rejected by Bob and Eve)
        //  |
        //  a1      (Add Bob and Eve) 3/2 (Accepted by Alice, Bob, Eve)
        //  |
        //  a0

        // Alice tries to accept the rejected revision
        alice_identity.accept(&a2).unwrap();
        assert_eq!(alice_identity.revision(&a2).unwrap().state, State::Rejected);
    }

    #[test]
    fn test_identity_cannot_redact_terminal_states() {
        let network = Network::default();
        let alice = &network.alice;
        let bob = &network.bob;

        let mut alice_identity = Identity::load_mut(&*alice.repo, &alice.signer).unwrap();
        let mut alice_doc = alice_identity.doc().clone().edit();
        alice_doc.delegate(bob.signer.public_key().into());
        alice_doc.threshold = 2;
        let a1 = alice_identity
            .update(
                cob::Title::new("Add Bob").unwrap(),
                "",
                &alice_doc.verified().unwrap(),
            )
            .unwrap();

        bob.repo.fetch(alice);
        let mut bob_identity = Identity::load_mut(&*bob.repo, &bob.signer).unwrap();
        bob_identity.accept(&a1).unwrap();
        alice.repo.fetch(bob);
        alice_identity.reload().unwrap();

        let mut alice_doc2 = alice_identity.doc().clone().edit();
        alice_doc2.visibility = Visibility::private([]);
        let a2 = alice_identity
            .update(
                cob::Title::new("A2").unwrap(),
                "",
                &alice_doc2.verified().unwrap(),
            )
            .unwrap();

        bob.repo.fetch(alice);
        bob_identity.reload().unwrap();

        bob_identity.accept(&a2).unwrap();
        alice_identity.redact(a2).unwrap();

        alice.repo.fetch(bob);
        alice_identity.reload().unwrap();

        assert_eq!(alice_identity.revision(&a2).unwrap().state, State::Redacted);

        let mut alice_doc3 = alice_identity.doc().clone().edit();
        alice_doc3.threshold = 1;
        let a3 = alice_identity
            .update(
                cob::Title::new("A3").unwrap(),
                "",
                &alice_doc3.verified().unwrap(),
            )
            .unwrap();

        bob.repo.fetch(alice);
        bob_identity.reload().unwrap();
        bob_identity.accept(&a3).unwrap();

        alice.repo.fetch(bob);
        alice_identity.reload().unwrap();
        assert_eq!(alice_identity.revision(&a3).unwrap().state, State::Accepted);

        alice_identity.redact(a3).unwrap();
        assert_eq!(alice_identity.revision(&a3).unwrap().state, State::Accepted);

        let mut alice_doc4 = alice_identity.doc().clone().edit();
        alice_doc4.visibility = Visibility::private([]);
        let a4 = alice_identity
            .update(
                cob::Title::new("A4").unwrap(),
                "",
                &alice_doc4.verified().unwrap(),
            )
            .unwrap();

        bob.repo.fetch(alice);
        bob_identity.reload().unwrap();
        bob_identity.reject(a4).unwrap();

        alice.repo.fetch(bob);
        alice_identity.reload().unwrap();
        assert_eq!(alice_identity.revision(&a4).unwrap().state, State::Rejected);

        //  a4      (Propose "A4") 1/2 (Rejected by Bob) -> Redact attempt ignored
        //  |
        //  a3      (Propose "A3") 2/2 (Accepted by Alice, Bob) -> Redact attempt ignored
        //  | \
        //  |  a2   (Propose "A2") 1/2 (Redacted by Alice concurrently with Bob's Accept)
        //  | /
        //  a1      (Add Bob) 2/2 (Accepted by Alice, Bob)
        //  |
        //  a0

        alice_identity.redact(a4).unwrap();
        assert_eq!(alice_identity.revision(&a4).unwrap().state, State::Rejected);
    }

    #[test]
    fn test_valid_identity() {
        let tempdir = tempfile::tempdir().unwrap();
        let mut rng = fastrand::Rng::new();

        let alice = Device::mock_rng(&mut rng);
        let bob = Device::mock_rng(&mut rng);
        let eve = Device::mock_rng(&mut rng);

        let storage = Storage::open(tempdir.path().join("storage"), fixtures::user()).unwrap();
        let (id, _, _, _) =
            fixtures::project(tempdir.path().join("copy"), &storage, &alice).unwrap();

        // Bob and Eve fork the project from Alice.
        rad::fork_remote(id, alice.public_key(), &bob, &storage).unwrap();
        rad::fork_remote(id, alice.public_key(), &eve, &storage).unwrap();

        let repo = storage.repository(id).unwrap();
        let mut identity = Identity::load_mut(&repo, &alice).unwrap();
        let doc = identity.doc().clone();
        let prj = doc.project().unwrap();
        let mut doc = doc.edit();

        // Make a change to the description and sign it.
        let desc = prj.description().to_owned() + "!";
        let prj = prj.update(None, desc, None).unwrap();
        doc.payload.insert(PayloadId::project(), prj.clone().into());
        identity
            .update(
                cob::Title::new("Update description").unwrap(),
                "",
                &doc.clone().verified().unwrap(),
            )
            .unwrap();

        // Add Bob as a delegate, and sign it.
        doc.delegate(bob.public_key().into());
        doc.threshold = 2;
        identity
            .update(
                cob::Title::new("Add bob").unwrap(),
                "",
                &doc.clone().verified().unwrap(),
            )
            .unwrap();

        // Add Eve as a delegate.
        doc.delegate(eve.public_key().into());

        // Update with both Bob and Alice's signature.
        let revision = identity
            .update(
                cob::Title::new("Add eve").unwrap(),
                "",
                &doc.clone().verified().unwrap(),
            )
            .unwrap();

        let mut bob_identity = Identity::load_mut(&repo, &bob).unwrap();
        bob_identity.accept(&revision).unwrap();

        // Update description again with signatures by Eve and Bob.
        let desc = prj.description().to_owned() + "?";
        let prj = prj.update(None, desc, None).unwrap();
        doc.payload.insert(PayloadId::project(), prj.into());
        let revision = bob_identity
            .update(
                cob::Title::new("Update description again").unwrap(),
                "Bob's repository",
                &doc.verified().unwrap(),
            )
            .unwrap();

        let mut eve_identity = Identity::load_mut(&repo, &eve).unwrap();
        eve_identity.accept(&revision).unwrap();

        let identity: Identity = Identity::load(&repo).unwrap();
        let root = repo.identity_root().unwrap();
        let doc = repo.identity_doc_at(revision).unwrap();

        assert_eq!(identity.signatures().count(), 2);
        assert_eq!(identity.revisions().count(), 5);
        assert_eq!(identity.id(), id);
        assert_eq!(identity.root().id, root);
        assert_eq!(identity.current().blob, doc.blob);
        assert_eq!(identity.current().description.as_str(), "Bob's repository");
        assert_eq!(identity.head(), revision);
        assert_eq!(identity.doc(), &*doc);
        assert_eq!(
            identity.doc().project().unwrap().description(),
            "Acme's repository!?"
        );

        assert_eq!(doc.project().unwrap().description(), "Acme's repository!?");
    }
}

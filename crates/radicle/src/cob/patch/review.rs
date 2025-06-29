use std::collections::{BTreeMap, BTreeSet};
use std::convert::Infallible;
use std::fmt;

use amplify::confinement::Collection;
use nonempty::NonEmpty;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::cob;
use crate::cob::{thread::CommentId, CodeLocation, Embed, Label, Uri};
use crate::node::{device::Device, NodeId};
use crate::storage::{ReadRepository, SignRepository};

use super::{Error as PatchError, Patch, PatchMut, ReviewId, RevisionId, Verdict};

/// A [`Draft`] review gathers the information required for submitting a review
/// to a [`Patch`].
///
/// A [`Draft`] is started using [`Draft::new`] or [`Draft::default`], which can
/// then be added to via its several methods.
///
/// Once a [`Draft`] is ready and fully-resolved, see [Embeds][#embeds] below,
/// the draft can be published using [`Draft::publish`].
///
/// # Comments
///
/// Note that comments of a [`Draft`] are different to the comment of a
/// [`Thread`]. Similarly to [`Draft`], this [`Comment`] is also the required
/// data to create a thread [comment].
///
/// Draft comments are kept track via a `CommentIndex`, which is simply a
/// monotonic counter. This allows the removal and editing of draft comments.
///
/// # Embeds
///
/// Note that the [`Draft`] has a generic parameter, `EmbedContent`, that allows
/// the type of embeds to be open to the caller. This is paired with the
/// [`EmbedStore`] trait to allow the caller to define where they may store
/// draft embeds, e.g. on disk. The store must implement [`EmbedStore::resolve`]
/// which allows the [`Draft`] to contain comments with fully-resolved embeds of
/// type `Embed<Uri>`.
///
/// [`Thread`]: crate::cob::Thread
/// [comment]: crate::cob::Comment
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Draft<EmbedContent> {
    verdict: Option<Verdict>,
    summary: Option<String>,
    labels: BTreeSet<Label>,
    comments: BTreeMap<CommentIndex, Comment<EmbedContent>>,
}

/// The result of publishing a [`Draft`] review, via [`Draft::publish`].
pub struct Published {
    review_id: ReviewId,
    comments: Vec<CommentId>,
}

impl Published {
    /// Get the [`ReviewId`] of the published [`Review`].
    ///
    /// [`Review`]: crate::cob::patch::Review
    pub fn review(&self) -> ReviewId {
        self.review_id
    }

    /// Get the [`CommentId`]s of the published [`Review`].
    ///
    /// [`Review`]: crate::cob::patch::Review
    pub fn comments(&self) -> &[CommentId] {
        &self.comments
    }
}

/// A [`CommentIndex`] keeps track of comments in a [`Draft`].
///
/// Can be used for editing or removing existing draft comments.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CommentIndex {
    index: usize,
}

impl fmt::Display for CommentIndex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.index)
    }
}

/// A draft [`Comment`] that contains the required data for adding a review
/// comment.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Comment<E> {
    body: String,
    location: Option<CodeLocation>,
    reply_to: Option<CommentId>,
    embeds: Vec<E>,
}

impl<E> Comment<E> {
    /// Construct a new [`Comment`] with a text `body`, an optional [`CodeLocation`], an optional `CommentId` for replying to a given comment, and a series of `embeds`.
    ///
    /// See [`Draft`] for more information on the `E` parameter.
    pub fn new<S>(
        body: S,
        location: impl Into<Option<CodeLocation>>,
        // TODO(finto): does this make sense in the context of creating a new
        // review comment?
        reply_to: impl Into<Option<CommentId>>,
        embeds: Vec<E>,
    ) -> Self
    where
        S: ToString,
    {
        Self {
            body: body.to_string(),
            location: location.into(),
            reply_to: reply_to.into(),
            embeds,
        }
    }

    fn resolve_embeds<S>(
        self,
        store: &S,
        repository: &git2::Repository,
    ) -> Result<Comment<Embed<Uri>>, S::Error>
    where
        S: EmbedStore<E>,
    {
        let embeds = self
            .embeds
            .into_iter()
            .map(|embed| store.resolve(embed, repository))
            .collect::<Result<_, _>>()?;
        Ok(Comment {
            body: self.body,
            location: self.location,
            reply_to: self.reply_to,
            embeds,
        })
    }
}

impl<EC> Draft<EC> {
    pub fn new() -> Self {
        Draft {
            verdict: None,
            summary: None,
            labels: BTreeSet::new(),
            comments: BTreeMap::new(),
        }
    }

    pub fn with_verdict(&mut self, verdict: impl Into<Option<Verdict>>) {
        self.verdict = verdict.into();
    }

    pub fn with_summary<S, B>(&mut self, summary: S)
    where
        S: Into<Option<B>>,
        B: ToString,
    {
        self.summary = summary.into().map(|body| body.to_string());
    }

    pub fn with_comment(&mut self, comment: Comment<EC>) -> CommentIndex {
        let ix = CommentIndex {
            index: self.comments.len(),
        };
        self.comments.insert(ix, comment);
        ix
    }

    pub fn with_comments(
        &mut self,
        comments: impl IntoIterator<Item = Comment<EC>>,
    ) -> Vec<CommentIndex> {
        comments
            .into_iter()
            .map(|comment| self.with_comment(comment))
            .collect()
    }

    pub fn with_labels(&mut self, labels: impl IntoIterator<Item = Label>) {
        self.labels.extend(labels);
    }

    pub fn remove_labels<'a>(&mut self, labels: impl Iterator<Item = &'a Label>) {
        for label in labels {
            self.labels.remove(label);
        }
    }

    pub fn edit_comment(&mut self, ix: &CommentIndex, comment: Comment<EC>) -> Option<Comment<EC>>
    where
        EC: Clone,
    {
        match self.comments.get_mut(ix) {
            Some(old) => {
                let old_ = old.clone();
                *old = comment;
                Some(old_)
            }
            None => None,
        }
    }

    pub fn remove_comment(&mut self, ix: &CommentIndex) -> Option<Comment<EC>> {
        self.comments.remove(ix)
    }

    pub fn resolve_embeds<S>(self, store: &S, repository: &git2::Repository) -> Ready<S::Error>
    where
        S: EmbedStore<EC>,
    {
        let mut resolved = Vec::with_capacity(self.comments.len());
        let mut comments = BTreeMap::with_capacity(self.comments.len());
        let mut unresolved: Option<NonEmpty<ResolveError<S::Error>>> = None;
        for (ix, comment) in self.comments {
            match comment.resolve_embeds(store, repository) {
                Ok(comment) => {
                    resolved.extend(comment.embeds.clone());
                    comments.insert(ix, comment);
                }
                Err(err) => {
                    let err = ResolveError {
                        comment_index: ix,
                        err,
                    };
                    match unresolved {
                        Some(ref mut errs) => errs.push(err),
                        None => unresolved = Some(NonEmpty::new(err)),
                    }
                }
            }
        }

        unresolved
            .map(|unresolved| Ready::Resolving {
                resolved,
                unresolved,
            })
            .unwrap_or(Ready::Draft(Draft {
                verdict: self.verdict,
                summary: self.summary,
                labels: self.labels,
                comments,
            }))
    }
}

impl Draft<Embed<Uri>> {
    pub fn publish<'a, 'g, R, C, G>(
        self,
        patch: &mut PatchMut<'a, 'g, R, C>,
        revision: RevisionId,
        signer: &Device<G>,
    ) -> Result<Published, PatchError>
    where
        C: cob::cache::Update<Patch>,
        R: ReadRepository + SignRepository + cob::Store<Namespace = NodeId>,
        G: crypto::signature::Signer<crypto::Signature>,
    {
        let labels = self.labels.into_iter().collect();
        let review_id = patch.review(revision, self.verdict, self.summary, labels, signer)?;
        let mut comments = Vec::with_capacity(self.comments.len());
        for (_, comment) in self.comments {
            let cid = patch.review_comment(
                review_id,
                comment.body,
                comment.location,
                comment.reply_to,
                comment.embeds,
                signer,
            )?;
            comments.push(cid);
        }

        Ok(Published {
            review_id,
            comments,
        })
    }
}

pub enum Resolving<E, A> {
    Resolved(Embed<Uri>),
    Annotated(Annotated<E, A>),
}

pub struct Annotated<E, A> {
    embed: E,
    annotation: A,
}

impl<E, A> Annotated<E, A> {
    pub fn embed(&self) -> &E {
        &self.embed
    }

    pub fn into_embed(self) -> E {
        self.embed
    }

    pub fn annotation(&self) -> &A {
        &self.annotation
    }
}

fn resolve<E, S>(draft: Draft<E>) -> Draft<Resolving<E, S::Error>>
where
    S: EmbedStore<E>,
{
    todo!()
}

fn ready<E, A>(draft: Draft<Resolving<E, A>>) -> Option<Draft<Embed<Uri>>> {
    let comments = draft.comments.into_iter().map(|(ix, c)| todo!()).collect();
    Some(Draft {
        verdict: draft.verdict,
        summary: draft.summary,
        labels: draft.labels,
        comments,
    })
}

/// The result of calling [`Draft::resolve_embeds`].
pub enum Ready<E>
where
    E: std::error::Error + Send + Sync + 'static,
{
    /// The [`Draft`] is ready to be published, since all [`Embed`]s were
    /// resolved.
    Draft(Draft<Embed<Uri>>),
    /// The [`Draft`] is not fully resolved, since some of the [`Embed`]s
    /// failed.
    Resolving {
        resolved: Vec<Embed<Uri>>,
        unresolved: NonEmpty<ResolveError<E>>,
    },
}

/// Error when resolving a [`Comment`]'s series of embeds.
#[derive(Debug, Error)]
#[error("failed to resolve the embeds of comment at index {comment_index} due to: {err}")]
pub struct ResolveError<E>
where
    E: std::error::Error + Send + Sync + 'static,
{
    comment_index: CommentIndex,
    err: E,
}

/// A store that keeps track of [`Embed`]s outside of a Git repository.
pub trait EmbedStore<E> {
    type Error: std::error::Error + Send + Sync + 'static;

    /// Resolve and store the outside embed, `E`, in the `repository`,
    /// constructing the final [`Embed`].
    fn resolve(&self, embed: E, repository: &git2::Repository) -> Result<Embed<Uri>, Self::Error>;
}

/// [`Resolved`] implements [`EmbedStore`] for `Embed<Uri>`, which does the
/// simplest thing possible and returns the `embed` itself.
///
/// Can be used in conjunction with other [`EmbedStore`]s if some [`Embed`]s are
/// already resolved.
pub struct Resolved {}

impl EmbedStore<Embed<Uri>> for Resolved {
    type Error = Infallible;

    fn resolve(&self, embed: Embed<Uri>, _: &git2::Repository) -> Result<Embed<Uri>, Self::Error> {
        Ok(embed)
    }
}

#[cfg(test)]
mod test {
    use std::io;
    use std::io::{Read, Write};
    use std::path::{Path, PathBuf};

    use tempfile::TempDir;

    use crate::cob::cache::NoCache;
    use crate::patch::{Cache, Patches};
    use crate::storage::WriteRepository;
    use crate::test;
    use crate::{cob, git};

    use super::*;

    pub struct MediaStore {
        dir: TempDir,
    }

    impl MediaStore {
        fn new() -> io::Result<Self> {
            let dir = tempfile::tempdir()?;
            Ok(Self { dir })
        }

        fn put<P>(&mut self, name: P, bytes: &[u8]) -> io::Result<PathBuf>
        where
            P: AsRef<Path>,
        {
            let file_path = self.dir.path().join(name);
            let mut file = std::fs::File::create(&file_path)?;
            file.write(bytes)?;
            Ok(file_path)
        }

        fn get<P>(&self, path: P) -> io::Result<Vec<u8>>
        where
            P: AsRef<Path>,
        {
            let file_path = self.dir.path().join(path);
            let mut file = std::fs::File::open(file_path)?;
            let mut buf = Vec::new();
            file.read_to_end(&mut buf)?;
            Ok(buf)
        }
    }

    impl EmbedStore<PathBuf> for MediaStore {
        type Error = io::Error;

        fn resolve(
            &self,
            embed: PathBuf,
            repository: &git2::Repository,
        ) -> Result<cob::Embed<cob::Uri>, Self::Error> {
            let name = embed
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "embed path did not have a file name or is invalid UTF-8",
                ))?;
            let content = self.get(&embed)?;
            cob::Embed::store(name, &content, repository)
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
        }
    }

    pub fn create_patch<'a, 'g, R, G>(
        alice: &test::setup::NodeWithRepo,
        patches: &'g mut Cache<Patches<'a, R>, NoCache>,
        signer: &Device<G>,
    ) -> PatchMut<'a, 'g, R, NoCache>
    where
        R: WriteRepository + cob::Store<Namespace = NodeId>,
        G: crypto::signature::Signer<crypto::Signature>,
    {
        let checkout = alice.repo.checkout();
        let branch = checkout.branch_with([("README", b"Hello World!")]);
        patches
            .create(
                "First Patch",
                "Creating a first patch",
                cob::patch::MergeTarget::Delegates,
                branch.base,
                branch.oid,
                &[],
                signer,
            )
            .unwrap()
    }

    #[test]
    fn test_draft_review() {
        let mut media_store = MediaStore::new().unwrap();
        let signer = Device::mock();
        let alice = test::setup::NodeWithRepo::default();
        let mut patches = Cache::no_cache(&*alice.repo).unwrap();

        let mut draft = Draft::<PathBuf>::new();
        draft.with_verdict(Verdict::Accept);
        draft.with_summary("L G T M");
        let embed = media_store.put("file.png", b"Definitely png data").unwrap();
        draft.with_comment(Comment::new(
            "Bikeshedding",
            CodeLocation {
                commit: git::raw::Oid::zero().into(),
                path: Path::new("file.rs").to_path_buf(),
                old: None,
                new: None,
            },
            None,
            vec![embed],
        ));
        draft.with_labels([Label::new("todo").unwrap()].into_iter());
        let ready = match draft.resolve_embeds(&media_store, &alice.repo.backend) {
            Ready::Draft(draft) => draft,
            Ready::Resolving { unresolved, .. } => {
                panic!("Failed to resolve embeds {unresolved:?}")
            }
        };

        let mut patch = create_patch(&alice, &mut patches, &signer);
        let (revision, _) = patch.latest();
        let published = ready.publish(&mut patch, revision, &signer).unwrap();

        let (_, revision) = patch.latest();
        assert!(
            patch.reviews.contains_key(&published.review()),
            "missing published review"
        );
        let review = revision.reviews.values().next().unwrap();
        assert_eq!(
            &review.comments().map(|(cid, _)| *cid).collect::<Vec<_>>(),
            published.comments(),
        )
    }
}

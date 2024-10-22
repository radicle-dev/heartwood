//! Review builder.
//!
//! This module enables a user to review a patch by interactively viewing and accepting diff hunks.
//! The interaction and output is modeled around `git add -p`.
//!
//! To implement this behavior, we keep a hidden Git tree object that tracks the state of the
//! repository including the accepted hunks. Thus, every time a diff hunk is accepted, it is applied
//! to that tree. We call that tree the "brain", as it tracks what the code reviewer has reviewed.
//!
//! The brain starts out equalling the tree of the base branch, and eventually, when the brain
//! matches the tree of the patch being reviewed (by accepting hunks), we can say that the patch has
//! been fully reviewed.
//!
use std::collections::VecDeque;
use std::fmt::Write as _;
use std::ops::{Deref, Not, Range};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::{fmt, io};

use radicle::cob;
use radicle::cob::patch::{PatchId, Revision, Verdict};
use radicle::cob::{CodeLocation, CodeRange};
use radicle::git;
use radicle::prelude::*;
use radicle::storage::git::{cob::DraftStore, Repository};
use radicle_git_ext::Oid;
use radicle_surf::diff::*;
use radicle_term::{Element, Terminal, VStack};

use crate::git::pretty_diff::ToPretty;
use crate::git::pretty_diff::{Blob, Blobs, Repo};
use crate::git::unified_diff::{self, FileHeader};
use crate::git::unified_diff::{Encode, HunkHeader};
use crate::terminal as term;
use crate::terminal::highlight::Highlighter;

/// Help message shown to user.
const HELP: &str = "\
y - accept this hunk
n - ignore this hunk
c - comment on this hunk
j - leave this hunk undecided, see next hunk
k - leave this hunk undecided, see previous hunk
s - split the current hunk into smaller hunks
q - quit; do not accept this hunk nor any of the remaining ones
? - print help";

/// A terminal or file where the review UI output can be written to.
trait PromptWriter: io::Write {
    /// Is the writer a terminal?
    fn is_terminal(&self) -> bool;
}

impl PromptWriter for Box<dyn PromptWriter> {
    fn is_terminal(&self) -> bool {
        self.deref().is_terminal()
    }
}

impl<T: io::Write + io::IsTerminal> PromptWriter for T {
    fn is_terminal(&self) -> bool {
        <Self as io::IsTerminal>::is_terminal(self)
    }
}

/// The actions that a user can carry out on a review item.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ReviewAction {
    Accept,
    Ignore,
    Comment,
    Split,
    Next,
    Previous,
    Help,
    Quit,
}

impl ReviewAction {
    /// Ask the user what action to take.
    fn prompt(
        mut input: impl io::BufRead,
        mut output: impl io::Write,
        prompt: impl fmt::Display,
    ) -> io::Result<Option<Self>> {
        write!(&mut output, "{prompt} ")?;

        let mut s = String::new();
        input.read_line(&mut s)?;

        if s.trim().is_empty() {
            return Ok(None);
        }
        Self::from_str(s.trim())
            .map(Some)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))
    }
}

impl std::fmt::Display for ReviewAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Accept => write!(f, "y"),
            Self::Ignore => write!(f, "n"),
            Self::Comment => write!(f, "c"),
            Self::Split => write!(f, "s"),
            Self::Next => write!(f, "j"),
            Self::Previous => write!(f, "k"),
            Self::Help => write!(f, "?"),
            Self::Quit => write!(f, "q"),
        }
    }
}

impl FromStr for ReviewAction {
    type Err = io::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "y" => Ok(Self::Accept),
            "n" => Ok(Self::Ignore),
            "c" => Ok(Self::Comment),
            "s" => Ok(Self::Split),
            "j" => Ok(Self::Next),
            "k" => Ok(Self::Previous),
            "?" => Ok(Self::Help),
            "q" => Ok(Self::Quit),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("invalid action '{s}'"),
            )),
        }
    }
}

/// A single review item. Can be a hunk or eg. a file move.
/// Files are usually split into multiple review items.
#[derive(Debug)]
pub enum ReviewItem {
    FileAdded {
        path: PathBuf,
        header: FileHeader,
        new: DiffFile,
        hunk: Option<Hunk<Modification>>,
    },
    FileDeleted {
        path: PathBuf,
        header: FileHeader,
        old: DiffFile,
        hunk: Option<Hunk<Modification>>,
    },
    FileModified {
        path: PathBuf,
        header: FileHeader,
        old: DiffFile,
        new: DiffFile,
        hunk: Option<Hunk<Modification>>,
    },
    FileMoved {
        moved: Moved,
    },
    FileCopied {
        copied: Copied,
    },
    FileEofChanged {
        path: PathBuf,
        header: FileHeader,
        old: DiffFile,
        new: DiffFile,
        eof: EofNewLine,
    },
    FileModeChanged {
        path: PathBuf,
        header: FileHeader,
        old: DiffFile,
        new: DiffFile,
    },
}

impl ReviewItem {
    fn hunk(&self) -> Option<&Hunk<Modification>> {
        match self {
            Self::FileAdded { hunk, .. } => hunk.as_ref(),
            Self::FileDeleted { hunk, .. } => hunk.as_ref(),
            Self::FileModified { hunk, .. } => hunk.as_ref(),
            _ => None,
        }
    }

    fn hunk_header(&self) -> Option<HunkHeader> {
        self.hunk().and_then(|h| HunkHeader::try_from(h).ok())
    }

    fn paths(&self) -> (Option<(&Path, Oid)>, Option<(&Path, Oid)>) {
        match self {
            Self::FileAdded { path, new, .. } => (None, Some((path, new.oid))),
            Self::FileDeleted { path, old, .. } => (Some((path, old.oid)), None),
            Self::FileMoved { moved } => (
                Some((&moved.old_path, moved.old.oid)),
                Some((&moved.new_path, moved.new.oid)),
            ),
            Self::FileCopied { copied } => (
                Some((&copied.old_path, copied.old.oid)),
                Some((&copied.new_path, copied.new.oid)),
            ),
            Self::FileModified { path, old, new, .. } => {
                (Some((path, old.oid)), Some((path, new.oid)))
            }
            Self::FileEofChanged { path, old, new, .. } => {
                (Some((path, old.oid)), Some((path, new.oid)))
            }
            Self::FileModeChanged { path, old, new, .. } => {
                (Some((path, old.oid)), Some((path, new.oid)))
            }
        }
    }

    fn file_header(&self) -> FileHeader {
        match self {
            Self::FileAdded { header, .. } => header.clone(),
            Self::FileDeleted { header, .. } => header.clone(),
            Self::FileMoved { moved } => FileHeader::Moved {
                old_path: moved.old_path.clone(),
                new_path: moved.new_path.clone(),
            },
            Self::FileCopied { copied } => FileHeader::Copied {
                old_path: copied.old_path.clone(),
                new_path: copied.new_path.clone(),
            },
            Self::FileModified { header, .. } => header.clone(),
            Self::FileEofChanged { header, .. } => header.clone(),
            Self::FileModeChanged { header, .. } => header.clone(),
        }
    }

    fn blobs<R: Repo>(&self, repo: &R) -> Blobs<(PathBuf, Blob)> {
        let (old, new) = self.paths();
        Blobs::from_paths(old, new, repo)
    }

    fn pretty<R: Repo>(&self, repo: &R) -> Box<dyn Element> {
        let mut hi = Highlighter::default();
        let blobs = self.blobs(repo);
        let highlighted = blobs.highlight(&mut hi);
        let header = self.file_header();

        match self {
            Self::FileMoved { moved } => moved.pretty(&mut hi, &header, repo),
            Self::FileCopied { copied } => copied.pretty(&mut hi, &header, repo),
            Self::FileModified { hunk, .. }
            | Self::FileAdded { hunk, .. }
            | Self::FileDeleted { hunk, .. } => {
                let header = header.pretty(&mut hi, &None, repo);
                let vstack = term::VStack::default()
                    .border(Some(term::colors::FAINT))
                    .padding(1)
                    .child(header);

                if let Some(hunk) = hunk {
                    let hunk = hunk.pretty(&mut hi, &highlighted, repo);
                    if !hunk.is_empty() {
                        return vstack.divider().merge(hunk).boxed();
                    }
                }
                vstack
            }
            Self::FileEofChanged { eof, .. } => match eof {
                EofNewLine::NewMissing => {
                    VStack::default().child(term::Label::new("`\\n` missing at end-of-file"))
                }
                EofNewLine::OldMissing => {
                    VStack::default().child(term::Label::new("`\\n` added at end-of-file"))
                }
                _ => VStack::default(),
            },
            Self::FileModeChanged { .. } => VStack::default(),
        }
        .boxed()
    }
}

/// Queue of items (usually hunks) left to review.
#[derive(Default)]
pub struct ReviewQueue {
    /// Hunks left to review.
    queue: VecDeque<(usize, ReviewItem)>,
}

impl ReviewQueue {
    /// Add a file to the queue.
    /// Mostly splits files into individual review items (eg. hunks) to review.
    fn add_file(&mut self, file: FileDiff) {
        let header = FileHeader::from(&file);

        match file {
            FileDiff::Moved(moved) => {
                self.add_item(ReviewItem::FileMoved { moved });
            }
            FileDiff::Copied(copied) => {
                self.add_item(ReviewItem::FileCopied { copied });
            }
            FileDiff::Added(a) => {
                self.add_item(ReviewItem::FileAdded {
                    path: a.path,
                    header: header.clone(),
                    new: a.new,
                    hunk: if let DiffContent::Plain {
                        hunks: Hunks(mut hs),
                        ..
                    } = a.diff
                    {
                        hs.pop()
                    } else {
                        None
                    },
                });
            }
            FileDiff::Deleted(d) => {
                self.add_item(ReviewItem::FileDeleted {
                    path: d.path,
                    header: header.clone(),
                    old: d.old,
                    hunk: if let DiffContent::Plain {
                        hunks: Hunks(mut hs),
                        ..
                    } = d.diff
                    {
                        hs.pop()
                    } else {
                        None
                    },
                });
            }
            FileDiff::Modified(m) => {
                if m.old.mode != m.new.mode {
                    self.add_item(ReviewItem::FileModeChanged {
                        path: m.path.clone(),
                        header: header.clone(),
                        old: m.old.clone(),
                        new: m.new.clone(),
                    });
                }
                match m.diff {
                    DiffContent::Empty => {
                        // Likely a file mode change, which is handled above.
                    }
                    DiffContent::Binary => {
                        self.add_item(ReviewItem::FileModified {
                            path: m.path.clone(),
                            header: header.clone(),
                            old: m.old.clone(),
                            new: m.new.clone(),
                            hunk: None,
                        });
                    }
                    DiffContent::Plain {
                        hunks: Hunks(hunks),
                        eof,
                        ..
                    } => {
                        for hunk in hunks {
                            self.add_item(ReviewItem::FileModified {
                                path: m.path.clone(),
                                header: header.clone(),
                                old: m.old.clone(),
                                new: m.new.clone(),
                                hunk: Some(hunk),
                            });
                        }
                        if let EofNewLine::OldMissing | EofNewLine::NewMissing = eof {
                            self.add_item(ReviewItem::FileEofChanged {
                                path: m.path.clone(),
                                header: header.clone(),
                                old: m.old.clone(),
                                new: m.new.clone(),
                                eof,
                            })
                        }
                    }
                }
            }
        }
    }

    fn add_item(&mut self, item: ReviewItem) {
        self.queue.push_back((self.queue.len(), item));
    }
}

impl From<Diff> for ReviewQueue {
    fn from(diff: Diff) -> Self {
        let mut queue = Self::default();
        for file in diff.into_files() {
            queue.add_file(file);
        }
        queue
    }
}

impl std::ops::Deref for ReviewQueue {
    type Target = VecDeque<(usize, ReviewItem)>;

    fn deref(&self) -> &Self::Target {
        &self.queue
    }
}

impl std::ops::DerefMut for ReviewQueue {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.queue
    }
}

impl Iterator for ReviewQueue {
    type Item = (usize, ReviewItem);

    fn next(&mut self) -> Option<Self::Item> {
        self.queue.pop_front()
    }
}

/// Builds a review for a single file.
/// Adjusts line deltas when a hunk is ignored.
pub struct FileReviewBuilder {
    header: FileHeader,
    delta: i32,
}

impl FileReviewBuilder {
    fn new(item: &ReviewItem) -> Self {
        Self {
            header: item.file_header(),
            delta: 0,
        }
    }

    fn set_item(&mut self, item: &ReviewItem) -> &mut Self {
        let header = item.file_header();
        if self.header != header {
            self.header = header;
            self.delta = 0;
        }
        self
    }

    fn ignore_item(&mut self, item: &ReviewItem) {
        if let Some(h) = item.hunk_header() {
            self.delta += h.new_size as i32 - h.old_size as i32;
        }
    }

    fn item_diff(&mut self, item: ReviewItem) -> Result<git::raw::Diff, Error> {
        let mut buf = Vec::new();
        let mut writer = unified_diff::Writer::new(&mut buf);
        writer.encode(&self.header)?;

        if let (Some(h), Some(mut header)) = (item.hunk(), item.hunk_header()) {
            header.old_line_no -= self.delta as u32;
            header.new_line_no -= self.delta as u32;

            let h = Hunk {
                header: header.to_unified_string()?.as_bytes().to_owned().into(),
                lines: h.lines.clone(),
                old: h.old.clone(),
                new: h.new.clone(),
            };
            writer.encode(&h)?;
        }
        drop(writer);

        git::raw::Diff::from_buffer(&buf).map_err(Error::from)
    }
}

/// Represents the reviewer's brain, ie. what they have seen or not seen in terms
/// of changes introduced by a patch.
pub struct Brain<'a> {
    /// Where the review draft is being stored.
    refname: git::Namespaced<'a>,
    /// The commit pointed to by the ref.
    head: git::raw::Commit<'a>,
    /// The tree of accepted changes pointed to by the head commit.
    accepted: git::raw::Tree<'a>,
}

impl<'a> Brain<'a> {
    /// Create a new brain in the repository.
    fn new(
        patch: PatchId,
        remote: &NodeId,
        base: git::raw::Commit,
        repo: &'a git::raw::Repository,
    ) -> Result<Self, git::raw::Error> {
        let refname = Self::refname(&patch, remote);
        let author = repo.signature()?;
        let oid = repo.commit(
            Some(refname.as_str()),
            &author,
            &author,
            &format!("Review for {patch}"),
            &base.tree()?,
            // TODO: Verify this is necessary, shouldn't matter.
            &[&base],
        )?;
        let head = repo.find_commit(oid)?;
        let tree = head.tree()?;

        Ok(Self {
            refname,
            head,
            accepted: tree,
        })
    }

    /// Return the content identifier of this brain. This represents the state of the
    /// accepted hunks, ie. the git tree.
    fn cid(&self) -> Oid {
        self.accepted.id().into()
    }

    /// Load an existing brain from the repository.
    fn load(
        patch: PatchId,
        remote: &NodeId,
        repo: &'a git::raw::Repository,
    ) -> Result<Self, git::raw::Error> {
        // TODO: Validate this leads to correct UX for potentially abandoned drafts on
        // past revisions.
        let refname = Self::refname(&patch, remote);
        let head = repo.find_reference(&refname)?.peel_to_commit()?;
        let tree = head.tree()?;

        Ok(Self {
            refname,
            head,
            accepted: tree,
        })
    }

    /// Accept changes to the brain.
    fn accept(
        &mut self,
        diff: git::raw::Diff,
        repo: &'a git::raw::Repository,
    ) -> Result<(), git::raw::Error> {
        let mut index = repo.apply_to_tree(&self.accepted, &diff, None)?;
        let accepted = index.write_tree_to(repo)?;
        self.accepted = repo.find_tree(accepted)?;

        // Update review with new brain.
        let head = self.head.amend(
            Some(&self.refname),
            None,
            None,
            None,
            None,
            Some(&self.accepted),
        )?;
        self.head = repo.find_commit(head)?;

        Ok(())
    }

    /// Get the brain's refname given the patch and remote.
    fn refname(patch: &PatchId, remote: &NodeId) -> git::Namespaced<'a> {
        git::refs::storage::draft::review(remote, patch)
    }
}

/// Builds a patch review interactively, across multiple files.
pub struct ReviewBuilder<'a, G> {
    /// Patch being reviewed.
    patch_id: PatchId,
    /// Signer.
    signer: G,
    /// Stored copy of repository.
    repo: &'a Repository,
    /// Single hunk review.
    hunk: Option<usize>,
    /// Verdict for review items.
    verdict: Option<Verdict>,
}

impl<'a, G: Signer> ReviewBuilder<'a, G> {
    /// Create a new review builder.
    pub fn new(patch_id: PatchId, signer: G, repo: &'a Repository) -> Self {
        Self {
            patch_id,
            signer,
            repo,
            hunk: None,
            verdict: None,
        }
    }

    /// Review a single hunk. Set to `None` to review all hunks.
    pub fn hunk(mut self, hunk: Option<usize>) -> Self {
        self.hunk = hunk;
        self
    }

    /// Give this verdict to all review items. Set to `None` to not give a verdict.
    pub fn verdict(mut self, verdict: Option<Verdict>) -> Self {
        self.verdict = verdict;
        self
    }

    /// Run the review builder for the given revision.
    pub fn run(self, revision: &Revision, opts: &mut git::raw::DiffOptions, term: Terminal) -> anyhow::Result<()> {
        let repo = self.repo.raw();
        let signer = &self.signer;
        let base = repo.find_commit((*revision.base()).into())?;
        let patch_id = self.patch_id;
        let tree = {
            let commit = repo.find_commit(revision.head().into())?;
            commit.tree()?
        };

        let stdout = io::stdout().lock();
        let mut stdin = io::stdin().lock();
        let mut writer: Box<dyn PromptWriter> = if self.hunk.is_some() || !stdout.is_terminal() {
            Box::new(stdout)
        } else {
            Box::new(io::stderr().lock())
        };
        let mut brain = if let Ok(b) = Brain::load(self.patch_id, signer.public_key(), repo) {
            term::success!(
                term,
                "Loaded existing review {} for patch {}",
                term::format::secondary(term::format::parens(term::format::oid(b.head.id()))),
                term::format::tertiary(&patch_id)
            );
            b
        } else {
            Brain::new(self.patch_id, signer.public_key(), base, repo)?
        };
        let diff = self.diff(&brain.accepted, &tree, repo, opts)?;
        let drafts = DraftStore::new(self.repo, *signer.public_key());
        let mut patches = cob::patch::Cache::no_cache(&drafts)?;
        let mut patch = patches.get_mut(&patch_id)?;
        let mut queue = ReviewQueue::from(diff);

        if queue.is_empty() {
            term::success!("All hunks have been reviewed");
            return Ok(());
        }

        let review = if let Some(r) = revision.review_by(signer.public_key()) {
            r.id()
        } else {
            patch.review(
                revision.id(),
                // This is amended before the review is finalized, if all hunks are
                // accepted. We can't set this to `None`, as that will be invalid without
                // a review summary.
                Some(Verdict::Reject),
                None,
                vec![],
                signer,
            )?
        };

        // File review for the current file. Starts out as `None` and is set on the first hunk.
        // Keeps track of deltas for hunk offsets.
        let mut file: Option<FileReviewBuilder> = None;
        let total = queue.len();

        while let Some((ix, item)) = queue.next() {
            if let Some(hunk) = self.hunk {
                if hunk != ix + 1 {
                    continue;
                }
            }
            let progress = term::format::secondary(format!("({}/{total})", ix + 1));
            let file = match file.as_mut() {
                Some(fr) => fr.set_item(&item),
                None => file.insert(FileReviewBuilder::new(&item)),
            };
            term::element::write_to(
                &item.pretty(repo),
                &mut writer,
                term::Constraint::from_env().unwrap_or_default(),
            )?;

            // Prompts the user for action on the above hunk.
            match self.prompt(&mut stdin, &mut writer, progress) {
                // When a hunk is accepted, we convert it to unified diff format,
                // and apply it to the `brain`.
                Some(ReviewAction::Accept) => {
                    // Compute hunk diff and update brain by applying it.
                    let diff = file.item_diff(item)?;
                    brain.accept(diff, repo)?;

                    if self.hunk.is_some() {
                        term::success!(term, "Updated brain to {}", brain.cid());
                    }
                }
                Some(ReviewAction::Ignore) => {
                    // Do nothing. Hunk will be reviewable again next time.
                    file.ignore_item(&item);
                }
                Some(ReviewAction::Comment) => {
                    let (old, new) = item.paths();
                    let path = old.or(new);

                    if let (Some(hunk), Some((path, _))) = (item.hunk(), path) {
                        let builder = CommentBuilder::new(revision.head(), path.to_path_buf());
                        let comments = builder.edit(hunk)?;

                        patch.transaction("Review comments", signer, |tx| {
                            for comment in comments {
                                tx.review_comment(
                                    review,
                                    comment.body,
                                    Some(comment.location),
                                    None,   // Not a reply.
                                    vec![], // No embeds.
                                )?;
                            }
                            Ok(())
                        })?;
                    } else {
                        term.eprintln(
                            term::format::tertiary(
                                "Commenting on binary blobs is not yet implemented"
                            )
                            .bold()
                        );
                        queue.push_front((ix, item));
                    }
                }
                Some(ReviewAction::Split) => {
                    term.eprintln(
                        term::format::tertiary("Splitting is not yet implemented").bold()
                    );
                    queue.push_front((ix, item));
                }
                Some(ReviewAction::Next) => {
                    queue.push_back((ix, item));
                }
                Some(ReviewAction::Previous) => {
                    queue.push_front((ix, item));

                    if let Some(e) = queue.pop_back() {
                        queue.push_front(e);
                    }
                }
                Some(ReviewAction::Quit) => {
                    break;
                }
                Some(ReviewAction::Help) => {
                    term.eprintln(term::format::tertiary(HELP).bold());
                    queue.push_front((ix, item));
                }
                None => {
                    term.eprintln(
                        term::format::secondary(format!(
                            "{} hunk(s) remaining to review",
                            queue.len() + 1
                        ))
                    );
                    queue.push_front((ix, item));
                }
            }
        }

        Ok(())
    }

    fn diff(
        &self,
        brain: &git::raw::Tree<'_>,
        tree: &git::raw::Tree<'_>,
        repo: &'a git::raw::Repository,
        opts: &mut git::raw::DiffOptions,
    ) -> Result<Diff, Error> {
        let mut find_opts = git::raw::DiffFindOptions::new();
        find_opts.exact_match_only(true);
        find_opts.all(true);
        find_opts.copies(false); // We don't support finding copies at the moment.

        let mut diff = repo.diff_tree_to_tree(Some(brain), Some(tree), Some(opts))?;
        diff.find_similar(Some(&mut find_opts))?;

        let diff = Diff::try_from(diff)?;

        Ok(diff)
    }

    fn prompt(
        &self,
        mut input: impl io::BufRead,
        output: &mut impl PromptWriter,
        progress: impl fmt::Display,
    ) -> Option<ReviewAction> {
        if let Some(v) = self.verdict {
            match v {
                Verdict::Accept => Some(ReviewAction::Accept),
                Verdict::Reject => Some(ReviewAction::Ignore),
            }
        } else if output.is_terminal() {
            let prompt = term::format::secondary("Accept this hunk? [y,n,c,j,k,q,?]").bold();

            ReviewAction::prompt(&mut input, output, format!("{progress} {prompt}"))
                .unwrap_or(Some(ReviewAction::Help))
        } else {
            Some(ReviewAction::Ignore)
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct ReviewComment {
    location: CodeLocation,
    body: String,
}

#[derive(thiserror::Error, Debug)]
enum Error {
    #[error(transparent)]
    Diff(#[from] unified_diff::Error),
    #[error(transparent)]
    Surf(#[from] radicle_surf::diff::git::error::Diff),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Format(#[from] std::fmt::Error),
    #[error(transparent)]
    Git(#[from] git::raw::Error),
}

#[derive(Debug)]
struct CommentBuilder {
    commit: Oid,
    path: PathBuf,
    comments: Vec<ReviewComment>,
}

impl CommentBuilder {
    fn new(commit: Oid, path: PathBuf) -> Self {
        Self {
            commit,
            path,
            comments: Vec::new(),
        }
    }

    fn edit(mut self, hunk: &Hunk<Modification>) -> Result<Vec<ReviewComment>, Error> {
        let mut input = String::new();
        for line in hunk.to_unified_string()?.lines() {
            writeln!(&mut input, "> {line}")?;
        }
        let output = term::Editor::comment()
            .extension("diff")
            .initial(input)?
            .edit()?;

        if let Some(output) = output {
            let header = HunkHeader::try_from(hunk)?;
            self.add_hunk(header, &output);
        }
        Ok(self.comments())
    }

    fn add_hunk(&mut self, hunk: HunkHeader, input: &str) -> &mut Self {
        let lines = input.trim().lines().map(|l| l.trim());
        let (mut old_line, mut new_line) = (hunk.old_line_no as usize, hunk.new_line_no as usize);
        let (mut old_start, mut new_start) = (old_line, new_line);
        let mut comment = String::new();

        for line in lines {
            if line.starts_with('>') {
                if !comment.is_empty() {
                    self.add_comment(
                        &hunk,
                        &comment,
                        old_start..old_line - 1,
                        new_start..new_line - 1,
                    );

                    old_start = old_line - 1;
                    new_start = new_line - 1;

                    comment.clear();
                }
                match line.trim_start_matches('>').trim_start().chars().next() {
                    Some('-') => old_line += 1,
                    Some('+') => new_line += 1,
                    _ => {
                        old_line += 1;
                        new_line += 1;
                    }
                }
            } else {
                comment.push_str(line);
                comment.push('\n');
            }
        }
        if !comment.is_empty() {
            self.add_comment(
                &hunk,
                &comment,
                old_start..old_line - 1,
                new_start..new_line - 1,
            );
        }
        self
    }

    fn add_comment(
        &mut self,
        hunk: &HunkHeader,
        comment: &str,
        mut old_range: Range<usize>,
        mut new_range: Range<usize>,
    ) {
        // Empty lines between quoted text can generate empty comments
        // that should be filtered out.
        if comment.trim().is_empty() {
            return;
        }
        // Top-level comment, it should apply to the whole hunk.
        if old_range.is_empty() && new_range.is_empty() {
            old_range = hunk.old_line_no as usize..(hunk.old_line_no + hunk.old_size + 1) as usize;
            new_range = hunk.new_line_no as usize..(hunk.new_line_no + hunk.new_size + 1) as usize;
        }
        let old_range = old_range
            .is_empty()
            .not()
            .then_some(old_range)
            .map(|range| CodeRange::Lines { range });
        let new_range = (new_range)
            .is_empty()
            .not()
            .then_some(new_range)
            .map(|range| CodeRange::Lines { range });

        self.comments.push(ReviewComment {
            location: CodeLocation {
                commit: self.commit,
                path: self.path.clone(),
                old: old_range,
                new: new_range,
            },
            body: comment.trim().to_owned(),
        });
    }

    fn comments(self) -> Vec<ReviewComment> {
        self.comments
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_review_comments_basic() {
        let input = r#"
> @@ -2559,18 +2560,18 @@ where
>                  // Only consider onion addresses if configured.
>                  AddressType::Onion => self.config.onion.is_some(),
>                  AddressType::Dns | AddressType::Ipv4 | AddressType::Ipv6 => true,
> -            })
> -            .take(wanted)
> -            .collect::<Vec<_>>(); // # -2564

Comment #1.

> +            });
>
> -        if available.len() < target {
> -            log::warn!( # -2567
> +        // Peers we are going to attempt connections to.
> +        let connect = available.take(wanted).collect::<Vec<_>>();

Comment #2.

> +        if connect.len() < wanted {
> +            log::debug!(
>                  target: "service",
> -                "Not enough available peers to connect to (available={}, target={target})",
> -                available.len()

Comment #3.

> +                "Not enough available peers to connect to (available={}, wanted={wanted})",

Comment #4.

> +                connect.len()
>              );
>          }
> -        for (id, ka) in available {
> +        for (id, ka) in connect {
>              self.connect(id, ka.addr.clone());
>          }
>     }

Comment #5.

"#;

        let commit = Oid::from_str("a32c4b93e2573fd83b15ac1ad6bf1317dc8fd760").unwrap();
        let path = PathBuf::from_str("main.rs").unwrap();
        let expected = &[
            (ReviewComment {
                location: CodeLocation {
                    commit,
                    path: path.clone(),
                    old: Some(CodeRange::Lines { range: 2559..2565 }),
                    new: Some(CodeRange::Lines { range: 2560..2563 }),
                },
                body: "Comment #1.".to_owned(),
            }),
            (ReviewComment {
                location: CodeLocation {
                    commit,
                    path: path.clone(),
                    old: Some(CodeRange::Lines { range: 2565..2568 }),
                    new: Some(CodeRange::Lines { range: 2563..2567 }),
                },
                body: "Comment #2.".to_owned(),
            }),
            (ReviewComment {
                location: CodeLocation {
                    commit,
                    path: path.clone(),
                    old: Some(CodeRange::Lines { range: 2568..2571 }),
                    new: Some(CodeRange::Lines { range: 2567..2570 }),
                },
                body: "Comment #3.".to_owned(),
            }),
            (ReviewComment {
                location: CodeLocation {
                    commit,
                    path: path.clone(),
                    old: None,
                    new: Some(CodeRange::Lines { range: 2570..2571 }),
                },
                body: "Comment #4.".to_owned(),
            }),
            (ReviewComment {
                location: CodeLocation {
                    commit,
                    path: path.clone(),
                    old: Some(CodeRange::Lines { range: 2571..2577 }),
                    new: Some(CodeRange::Lines { range: 2571..2578 }),
                },
                body: "Comment #5.".to_owned(),
            }),
        ];

        let mut builder = CommentBuilder::new(commit, path.clone());
        builder.add_hunk(
            HunkHeader {
                old_line_no: 2559,
                old_size: 18,
                new_line_no: 2560,
                new_size: 18,
                text: vec![],
            },
            input,
        );
        let actual = builder.comments();

        assert_eq!(actual.len(), expected.len(), "{actual:#?}");

        for (left, right) in actual.iter().zip(expected) {
            assert_eq!(left, right);
        }
    }

    #[test]
    fn test_review_comments_multiline() {
        let input = r#"
> @@ -2559,9 +2560,7 @@ where
>                  // Only consider onion addresses if configured.
>                  AddressType::Onion => self.config.onion.is_some(),
>                  AddressType::Dns | AddressType::Ipv4 | AddressType::Ipv6 => true,
> -            })
> -            .take(wanted)
> -            .collect::<Vec<_>>(); // # -2564

Blah blah blah blah blah blah blah.
Blah blah blah.

Blaah blaah blaah blaah blaah blaah blaah.
blaah blaah blaah.

Blaaah blaaah blaaah.

> +            });
>
> -        if available.len() < target {
> -            log::warn!( # -2567
> +        // Peers we are going to attempt connections to.
> +        let connect = available.take(wanted).collect::<Vec<_>>();

Woof woof.
Woof.
Woof.

Woof.

"#;

        let commit = Oid::from_str("a32c4b93e2573fd83b15ac1ad6bf1317dc8fd760").unwrap();
        let path = PathBuf::from_str("main.rs").unwrap();
        let expected = &[
            (ReviewComment {
                location: CodeLocation {
                    commit,
                    path: path.clone(),
                    old: Some(CodeRange::Lines { range: 2559..2565 }),
                    new: Some(CodeRange::Lines { range: 2560..2563 }),
                },
                body: r#"
Blah blah blah blah blah blah blah.
Blah blah blah.

Blaah blaah blaah blaah blaah blaah blaah.
blaah blaah blaah.

Blaaah blaaah blaaah.
"#
                .trim()
                .to_owned(),
            }),
            (ReviewComment {
                location: CodeLocation {
                    commit,
                    path: path.clone(),
                    old: Some(CodeRange::Lines { range: 2565..2568 }),
                    new: Some(CodeRange::Lines { range: 2563..2567 }),
                },
                body: r#"
Woof woof.
Woof.
Woof.

Woof.
"#
                .trim()
                .to_owned(),
            }),
        ];

        let mut builder = CommentBuilder::new(commit, path.clone());
        builder.add_hunk(
            HunkHeader {
                old_line_no: 2559,
                old_size: 9,
                new_line_no: 2560,
                new_size: 7,
                text: vec![],
            },
            input,
        );
        let actual = builder.comments();

        assert_eq!(actual.len(), expected.len(), "{actual:#?}");

        for (left, right) in actual.iter().zip(expected) {
            assert_eq!(left, right);
        }
    }

    #[test]
    fn test_review_comments_before() {
        let input = r#"
This is a top-level comment.

> @@ -2559,9 +2560,7 @@ where
>                  // Only consider onion addresses if configured.
>                  AddressType::Onion => self.config.onion.is_some(),
>                  AddressType::Dns | AddressType::Ipv4 | AddressType::Ipv6 => true,
> -            })
> -            .take(wanted)
> -            .collect::<Vec<_>>(); // # -2564
> +            });
>
> -        if available.len() < target {
> -            log::warn!( # -2567
> +        // Peers we are going to attempt connections to.
> +        let connect = available.take(wanted).collect::<Vec<_>>();
"#;

        let commit = Oid::from_str("a32c4b93e2573fd83b15ac1ad6bf1317dc8fd760").unwrap();
        let path = PathBuf::from_str("main.rs").unwrap();
        let expected = &[(ReviewComment {
            location: CodeLocation {
                commit,
                path: path.clone(),
                old: Some(CodeRange::Lines { range: 2559..2569 }),
                new: Some(CodeRange::Lines { range: 2560..2568 }),
            },
            body: "This is a top-level comment.".to_owned(),
        })];

        let mut builder = CommentBuilder::new(commit, path.clone());
        builder.add_hunk(
            HunkHeader {
                old_line_no: 2559,
                old_size: 9,
                new_line_no: 2560,
                new_size: 7,
                text: vec![],
            },
            input,
        );
        let actual = builder.comments();

        assert_eq!(actual.len(), expected.len(), "{actual:#?}");

        for (left, right) in actual.iter().zip(expected) {
            assert_eq!(left, right);
        }
    }

    #[test]
    fn test_review_comments_split_hunk() {
        let input = r#"
> @@ -2559,6 +2560,4 @@ where
>                  // Only consider onion addresses if configured.
>                  AddressType::Onion => self.config.onion.is_some(),
>                  AddressType::Dns | AddressType::Ipv4 | AddressType::Ipv6 => true,
> -            })
> -            .take(wanted)

> -            .collect::<Vec<_>>();
> +            });

Comment on a split hunk.
"#;

        let commit = Oid::from_str("a32c4b93e2573fd83b15ac1ad6bf1317dc8fd760").unwrap();
        let path = PathBuf::from_str("main.rs").unwrap();
        let expected = &[(ReviewComment {
            location: CodeLocation {
                commit,
                path: path.clone(),
                old: Some(CodeRange::Lines { range: 2564..2565 }),
                new: Some(CodeRange::Lines { range: 2563..2564 }),
            },
            body: "Comment on a split hunk.".to_owned(),
        })];

        let mut builder = CommentBuilder::new(commit, path.clone());
        builder.add_hunk(
            HunkHeader {
                old_line_no: 2559,
                old_size: 6,
                new_line_no: 2560,
                new_size: 4,
                text: vec![],
            },
            input,
        );
        let actual = builder.comments();

        assert_eq!(actual.len(), expected.len(), "{actual:#?}");

        for (left, right) in actual.iter().zip(expected) {
            assert_eq!(left, right);
        }
    }
}

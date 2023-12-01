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
use std::io::IsTerminal as _;
use std::str::FromStr;
use std::{fmt, io};

use radicle::cob::patch::{PatchId, Revision, Verdict};
use radicle::git;
use radicle::prelude::*;
use radicle::storage::git::Repository;
use radicle_surf::diff::*;

use crate::git::unified_diff;
use crate::terminal as term;

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
pub struct ReviewItem<'a> {
    file: &'a FileDiff,
    hunk: Option<&'a Hunk<Modification>>,
}

/// Queue of items (usually hunks) left to review.
#[derive(Default)]
pub struct ReviewQueue<'a> {
    queue: VecDeque<(usize, ReviewItem<'a>)>,
}

impl<'a> ReviewQueue<'a> {
    /// Push an item to the queue.
    fn push(&mut self, file: &'a FileDiff, hunks: Option<&'a Hunks<Modification>>) {
        let mut queue_item = |hunk| {
            self.queue
                .push_back((self.queue.len(), ReviewItem { file, hunk }))
        };

        if let Some(hunks) = hunks {
            for hunk in hunks.iter() {
                queue_item(Some(hunk));
            }
        } else {
            queue_item(None);
        }
    }
}

impl<'a> std::ops::Deref for ReviewQueue<'a> {
    type Target = VecDeque<(usize, ReviewItem<'a>)>;

    fn deref(&self) -> &Self::Target {
        &self.queue
    }
}

impl<'a> std::ops::DerefMut for ReviewQueue<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.queue
    }
}

impl<'a> Iterator for ReviewQueue<'a> {
    type Item = (usize, ReviewItem<'a>);

    fn next(&mut self) -> Option<Self::Item> {
        self.queue.pop_front()
    }
}

/// Builds a patch review interactively.
pub struct ReviewBuilder<'a> {
    /// Patch being reviewed.
    patch_id: PatchId,
    /// Where the review draft is being stored.
    refname: git::Namespaced<'a>,
    /// Stored copy of repository.
    repo: &'a Repository,
    /// Single hunk review.
    hunk: Option<usize>,
    /// Verdict for review items.
    verdict: Option<Verdict>,
}

impl<'a> ReviewBuilder<'a> {
    /// Create a new review builder.
    pub fn new(patch_id: PatchId, nid: NodeId, repo: &'a Repository) -> Self {
        Self {
            patch_id,
            refname: git::refs::storage::draft::review(&nid, &patch_id),
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
    pub fn run(self, revision: &Revision, opts: &mut git::raw::DiffOptions) -> anyhow::Result<()> {
        let repo = self.repo.raw();
        let base = repo.find_commit((*revision.base()).into())?;
        let author = repo.signature()?;
        let patch_id = self.patch_id;
        let review = if let Ok(c) = self.current() {
            term::success!(
                "Loaded existing review {} for patch {}",
                term::format::secondary(term::format::parens(term::format::oid(c.id()))),
                term::format::tertiary(&patch_id)
            );
            c
        } else {
            let oid = repo.commit(
                Some(self.refname.as_str()),
                &author,
                &author,
                &format!("Review {patch_id}"),
                &base.tree()?,
                &[&base],
            )?;
            repo.find_commit(oid)?
        };

        let mut writer = unified_diff::Writer::new(io::stdout()).styled(true);
        let mut queue = ReviewQueue::default(); // Queue of hunks to review.
        let mut current = None; // File of the current hunk.
        let mut stdin = io::stdin().lock();
        let mut stderr = io::stderr().lock();

        let commit = repo.find_commit(revision.head().into())?;
        let tree = commit.tree()?;
        let brain = review.tree()?;

        let mut find_opts = git::raw::DiffFindOptions::new();
        find_opts.exact_match_only(true);
        find_opts.all(true);
        find_opts.copies(false); // We don't support finding copies at the moment.

        let mut diff = repo.diff_tree_to_tree(Some(&brain), Some(&tree), Some(opts))?;
        diff.find_similar(Some(&mut find_opts))?;

        if diff.deltas().next().is_none() {
            term::success!("All hunks have been reviewed");
            return Ok(());
        }
        let diff = Diff::try_from(diff)?;

        for file in diff.files() {
            match file {
                FileDiff::Modified(f) => match &f.diff {
                    DiffContent::Plain { hunks, .. } => queue.push(file, Some(hunks)),
                    DiffContent::Binary => queue.push(file, None),
                    DiffContent::Empty => {}
                },
                FileDiff::Added(f) => match &f.diff {
                    DiffContent::Plain { hunks, .. } => queue.push(file, Some(hunks)),
                    DiffContent::Binary => queue.push(file, None),
                    DiffContent::Empty => {}
                },
                FileDiff::Deleted(f) => match &f.diff {
                    DiffContent::Plain { hunks, .. } => queue.push(file, Some(hunks)),
                    DiffContent::Binary => queue.push(file, None),
                    DiffContent::Empty => {}
                },
                FileDiff::Moved(_) => queue.push(file, None),
                FileDiff::Copied(_) => {
                    // Copies are not supported and should never be generated due to the diff
                    // options we pass.
                    panic!("ReviewBuilder::by_hunk: copy diffs are not supported");
                }
            }
        }
        let total = queue.len();

        while let Some((ix, item)) = queue.next() {
            if let Some(hunk) = self.hunk {
                if hunk != ix + 1 {
                    continue;
                }
            }
            let progress = term::format::secondary(format!("({}/{total})", ix + 1));
            let ReviewItem { file, hunk } = item;

            if current.map_or(true, |c| c != file) {
                writer.encode(&unified_diff::FileHeader::from(file))?;
                current = Some(file);
            }
            if let Some(h) = hunk {
                writer.encode(h)?;
            }

            match self.prompt(&mut stdin, &mut stderr, progress) {
                Some(ReviewAction::Accept) => {
                    let mut buf = Vec::new();
                    {
                        let mut writer = unified_diff::Writer::new(&mut buf);

                        writer.encode(&unified_diff::FileHeader::from(file))?;
                        if let Some(h) = hunk {
                            writer.encode(h)?;
                        }
                    }
                    let diff = git::raw::Diff::from_buffer(&buf)?;

                    let mut index = repo.apply_to_tree(&brain, &diff, None)?;
                    let brain = index.write_tree_to(repo)?;
                    let brain = repo.find_tree(brain)?;

                    let _oid =
                        review.amend(Some(&self.refname), None, None, None, None, Some(&brain))?;
                }
                Some(ReviewAction::Ignore) => {
                    // Do nothing. Hunk will be reviewable again next time.
                }
                Some(ReviewAction::Comment) => {
                    eprintln!(
                        "{}",
                        term::format::tertiary("Commenting is not yet implemented").bold()
                    );
                    queue.push_front((ix, item));
                }
                Some(ReviewAction::Split) => {
                    eprintln!(
                        "{}",
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
                    eprintln!("{}", term::format::tertiary(HELP).bold());
                    queue.push_front((ix, item));
                }
                None => {
                    eprintln!(
                        "{}",
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

    fn prompt(
        &self,
        mut input: impl io::BufRead,
        mut output: &mut io::StderrLock,
        progress: impl fmt::Display,
    ) -> Option<ReviewAction> {
        if let Some(v) = self.verdict {
            match v {
                Verdict::Accept => Some(ReviewAction::Accept),
                Verdict::Reject => Some(ReviewAction::Ignore),
            }
        } else if output.is_terminal() {
            let prompt = term::format::secondary("Accept this hunk? [y,n,c,j,k,?]").bold();

            ReviewAction::prompt(&mut input, &mut output, format!("{progress} {prompt}"))
                .unwrap_or(Some(ReviewAction::Help))
        } else {
            Some(ReviewAction::Ignore)
        }
    }

    fn current(&self) -> Result<git::raw::Commit, git::raw::Error> {
        self.repo
            .raw()
            .find_reference(&self.refname)?
            .peel_to_commit()
    }
}

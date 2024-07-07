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
use std::io::IsTerminal as _;
use std::ops::{Not, Range};
use std::path::PathBuf;
use std::str::FromStr;
use std::{fmt, io};

use radicle::cob::patch::{PatchId, Revision, Verdict};
use radicle::cob::{CodeLocation, CodeRange};
use radicle::git;
use radicle::prelude::*;
use radicle::storage::git::Repository;
use radicle_git_ext::Oid;
use radicle_surf::diff::*;

use crate::git::unified_diff;
use crate::git::unified_diff::{Encode, HunkHeader};
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
            // TODO: Validate this leads to correct UX for potentially abandoned drafts on
            // past revisions.
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
        let tree = {
            let commit = repo.find_commit(revision.head().into())?;
            commit.tree()?
        };

        let mut stdin = io::stdin().lock();
        let mut stderr = io::stderr().lock();
        let mut review = if let Ok(c) = self.current() {
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
                // TODO: Verify this is necessary, shouldn't matter.
                &[&base],
            )?;
            repo.find_commit(oid)?
        };
        let mut brain = review.tree()?;
        let mut writer = unified_diff::Writer::new(io::stdout()).styled(true);
        let mut queue = ReviewQueue::default(); // Queue of hunks to review.
        let mut current = None; // File of the current hunk.

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
        let mut delta: i32 = 0;

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
                delta = 0;
            }

            let header = hunk
                .map(|h| {
                    let header = unified_diff::HunkHeader::try_from(h)?;
                    writer.encode(h)?;
                    Ok::<_, anyhow::Error>(header)
                })
                .transpose()?;

            match self.prompt(&mut stdin, &mut stderr, progress) {
                Some(ReviewAction::Accept) => {
                    let mut buf = Vec::new();
                    {
                        let mut writer = unified_diff::Writer::new(&mut buf);
                        writer.encode(&unified_diff::FileHeader::from(file))?;

                        if let (Some(h), Some(mut header)) = (hunk, header) {
                            header.old_line_no -= delta as u32;
                            header.new_line_no -= delta as u32;

                            let h = Hunk {
                                header: header.to_unified_string()?.as_bytes().to_owned().into(),
                                lines: h.lines.clone(),
                                old: h.old.clone(),
                                new: h.new.clone(),
                            };
                            writer.encode(&h)?;
                        }
                    }
                    let diff = git::raw::Diff::from_buffer(&buf)?;

                    let mut index = repo.apply_to_tree(&brain, &diff, None)?;
                    let brain_oid = index.write_tree_to(repo)?;
                    brain = repo.find_tree(brain_oid)?;

                    let oid =
                        review.amend(Some(&self.refname), None, None, None, None, Some(&brain))?;
                    review = repo.find_commit(oid)?;
                }
                Some(ReviewAction::Ignore) => {
                    // Do nothing. Hunk will be reviewable again next time.
                    if let Some(h) = header {
                        delta += h.new_size as i32 - h.old_size as i32;
                    }
                }
                Some(ReviewAction::Comment) => {
                    if let Some(hunk) = hunk {
                        let mut builder =
                            CommentBuilder::new(revision.head(), item.file.path().to_path_buf());
                        builder.edit(hunk)?;

                        let _comments = builder.comments();

                        queue.push_front((ix, item));
                    } else {
                        eprintln!(
                            "{}",
                            term::format::tertiary(
                                "Commenting on binary blobs is not yet implemented"
                            )
                            .bold()
                        );
                        queue.push_front((ix, item));
                    }
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
            let prompt = term::format::secondary("Accept this hunk? [y,n,c,j,k,q,?]").bold();

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
    Io(#[from] io::Error),
    #[error(transparent)]
    Format(#[from] std::fmt::Error),
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

    fn edit(&mut self, hunk: &Hunk<Modification>) -> Result<&mut Self, Error> {
        let mut input = String::new();
        for line in hunk.to_unified_string()?.lines() {
            writeln!(&mut input, "> {line}")?;
        }
        let output = term::Editor::new().extension("diff").edit(input)?;

        if let Some(output) = output {
            let header = HunkHeader::try_from(hunk)?;
            self.add_hunk(header, &output);
        }
        Ok(self)
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

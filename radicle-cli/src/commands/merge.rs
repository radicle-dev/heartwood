use std::ffi::OsString;
use std::fmt;
use std::fmt::Write;

use anyhow::{anyhow, Context};

use crate::git::Rev;
use crate::terminal as term;
use crate::terminal::args::{string, Args, Error, Help};
use radicle::cob::patch::{Patch, PatchId, Patches};
use radicle::git;
use radicle::prelude::*;
use radicle::rad;

pub const HELP: Help = Help {
    name: "merge",
    description: "Merge a patch",
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad merge [<revision-id>] [<option>...]

    To specify a patch revision to merge, use the fully qualified revision id.

Options

    -f, --force               Force merging an older patch revision
    -i, --interactive         Ask for confirmations
        --help                Print help
"#,
};

/// Merge commit help message.
const MERGE_HELP_MSG: &str = r#"
# Check the commit message for this merge and make sure everything looks good,
# or make any necessary change.
#
# Lines starting with '#' will be ignored, and an empty message aborts the commit.
#
# vim: ft=gitcommit
#
"#;

/// A patch merge style.
#[derive(Debug, PartialEq, Eq)]
pub enum MergeStyle {
    /// A merge commit is created.
    Commit,
    /// The branch is fast-forwarded to the patch's commit.
    FastForward,
}

impl fmt::Display for MergeStyle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Commit => {
                write!(f, "merge-commit")
            }
            Self::FastForward => {
                write!(f, "fast-forward")
            }
        }
    }
}

#[derive(PartialEq, Eq)]
pub enum State {
    Open,
    Merged,
}

#[derive(Debug)]
pub struct Options {
    pub revision_id: Rev,
    pub force: bool,
    pub interactive: bool,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut force = false;
        let mut revision_id = None;
        let mut interactive = false;

        while let Some(arg) = parser.next()? {
            match arg {
                Long("help") => {
                    return Err(Error::Help.into());
                }
                Long("force") | Short('f') => {
                    force = true;
                }
                Long("interactive") | Short('i') => {
                    interactive = true;
                }
                Value(val) => {
                    let val = string(&val);
                    revision_id = Some(Rev::from(val));
                }
                _ => return Err(anyhow::anyhow!(arg.unexpected())),
            }
        }

        let revision_id =
            revision_id.ok_or_else(|| anyhow!("a revision id to merge must be provided"))?;

        Ok((
            Options {
                revision_id,
                force,
                interactive,
            },
            vec![],
        ))
    }
}

pub fn run(options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    //
    // Setup
    //
    let (repo, id) =
        rad::cwd().map_err(|_| anyhow!("this command must be run in the context of a project"))?;
    let profile = ctx.profile()?;
    let signer = term::signer(&profile)?;
    let repository = profile.storage.repository(id)?;
    let _project = repository
        .identity_doc_of(profile.id())
        .context(format!("couldn't load project {id} from local state"))?;
    let repository = profile.storage.repository(id)?;
    let mut patches = Patches::open(&repository)?;

    if repo.head_detached()? {
        anyhow::bail!("HEAD is in a detached state; can't merge");
    }

    //
    // Get patch information
    //
    let revision_id = options.revision_id.resolve(&repository.backend)?;
    let (patch_id, patch, revision) = patches.find_by_revision(&revision_id)?.ok_or(anyhow!(
        "no open patch with revision `{}` could be found",
        &revision_id
    ))?;
    if !patch.is_open() {
        anyhow::bail!(
            "revision's patch must be open for merging, but it is `{}`",
            patch.state()
        );
    }
    let (last_revision_id, _) = patch
        .latest()
        .ok_or(anyhow!("patch must have atleast one unredacted revision"))?;
    if !options.force && revision_id != *last_revision_id {
        anyhow::bail!("refusing to merge old patch revision");
    }

    let mut patch = patches
        .get_mut(&patch_id)
        .map_err(|e| anyhow!("couldn't find patch {} locally: {e}", &id))?;

    let head = repo.head()?;
    let branch = head
        .shorthand()
        .ok_or_else(|| anyhow!("invalid head branch"))?;
    let head_oid = head
        .target()
        .ok_or_else(|| anyhow!("cannot merge into detatched head; aborting"))?;

    //
    // Analyze merge
    //
    let patch_commit = repo
        .find_annotated_commit(revision.head().into())
        .context("patch head not found in local repository")?;
    let (merge, _merge_pref) = repo.merge_analysis(&[&patch_commit])?;

    let merge_style = if merge.is_fast_forward() {
        // The given merge input is a fast-forward from HEAD and no merge needs to be performed.
        // Instead, the client can apply the input commits to its HEAD.
        MergeStyle::FastForward
    } else if merge.is_normal() {
        // A “normal” merge; both HEAD and the given merge input have diverged from their common
        // ancestor. The divergent commits must be merged.
        //
        // Let's check if there are potential merge conflicts.
        let our_commit = head.peel_to_commit()?;
        let their_commit = repo.find_commit(revision.head().into())?;

        let index = repo
            .merge_commits(&our_commit, &their_commit, None)
            .context("failed to perform merge analysis")?;

        if index.has_conflicts() {
            return Err(Error::WithHint {
                err: anyhow!("patch conflicts with {}", branch),
                hint: "Patch must be rebased before it can be merged.",
            }
            .into());
        }
        MergeStyle::Commit
    } else if merge.is_up_to_date() {
        term::success!(
            "Patch {} is already part of current branch {}",
            term::format::tertiary(patch_id),
            term::format::parens(term::format::yellow(branch))
        );
        return Ok(());
    } else if merge.is_unborn() {
        anyhow::bail!("HEAD does not point to a valid commit");
    } else {
        anyhow::bail!(
            "no merge is possible between {} and {}",
            head_oid,
            revision.head()
        );
    };

    let merge_style_pretty = match merge_style {
        MergeStyle::FastForward => term::format::style(merge_style.to_string())
            .dim()
            .italic()
            .to_string(),
        MergeStyle::Commit => term::format::yellow(merge_style.to_string())
            .italic()
            .to_string(),
    };

    // SAFETY: The patch has already been fetched by its revision_id.
    let revision_ix = patch
        .revisions()
        .position(|(id_, _)| id_ == &revision_id)
        .unwrap();
    term::info!(
        "{} {} {} {} by {} into {} {} via {}...",
        term::format::bold("Merging"),
        term::format::tertiary(term::format::cob(&patch_id)),
        term::format::dim(format!("R{revision_ix}")),
        term::format::parens(term::format::secondary(term::format::oid(revision.head()))),
        term::format::tertiary(term::format::node(&patch.author().id)),
        term::format::highlight(branch),
        term::format::parens(term::format::secondary(term::format::oid(head_oid))),
        merge_style_pretty
    );

    if options.interactive && !term::confirm("Confirm?") {
        anyhow::bail!("merge aborted by user");
    }

    //
    // Perform merge
    //
    match merge_style {
        MergeStyle::Commit => {
            merge_commit(&repo, patch_id, &patch_commit, &patch, signer.public_key())?;
        }
        MergeStyle::FastForward => {
            fast_forward(&repo, &revision.head())?;
        }
    }

    term::success!(
        "Updated {} {} -> {} via {}",
        term::format::highlight(branch),
        term::format::secondary(term::format::oid(head_oid)),
        term::format::secondary(term::format::oid(revision.head())),
        merge_style_pretty
    );

    //
    // Update patch COB
    //
    // TODO: Don't allow merging the same revision twice?
    patch.merge(revision_id, head_oid.into(), &signer)?;

    term::success!(
        "Patch state updated, use {} to publish",
        term::format::secondary("`rad push`")
    );

    Ok(())
}

// Perform git merge.
//
// This does not touch the COB state.
//
// Nb. Merge can fail even though conflicts were not detected if there are some
// files in the repo that are not checked in. This is because the merge conflict
// simulation only operates on the commits, not the checkout.
fn merge_commit(
    repo: &git::raw::Repository,
    patch_id: PatchId,
    patch_commit: &git::raw::AnnotatedCommit,
    patch: &Patch,
    whoami: &PublicKey,
) -> anyhow::Result<()> {
    let description = patch.description().trim();
    let mut merge_opts = git::raw::MergeOptions::new();
    let mut merge_msg = format!(
        "Merge patch '{}' from {}",
        term::format::cob(&patch_id),
        patch.author().id()
    );
    write!(&mut merge_msg, "\n\n")?;

    if !description.is_empty() {
        write!(&mut merge_msg, "{description}")?;
        write!(&mut merge_msg, "\n\n")?;
    }
    writeln!(&mut merge_msg, "Rad-Patch: {patch_id}")?;
    writeln!(&mut merge_msg, "Rad-Author: {}", patch.author().id())?;
    writeln!(&mut merge_msg, "Rad-Committer: {whoami}")?;
    writeln!(&mut merge_msg)?;
    writeln!(&mut merge_msg, "{}", MERGE_HELP_MSG.trim())?;

    // Offer user the chance to edit the message before committing.
    let merge_msg = match term::Editor::new().extension("git-commit").edit(merge_msg) {
        Ok(Some(s)) => s
            .lines()
            .filter(|l| !l.starts_with('#'))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => anyhow::bail!("user aborted merge"),
    };

    // Empty message aborts merge.
    if merge_msg.trim().is_empty() {
        anyhow::bail!("user aborted merge");
    }

    // Perform merge (nb. this does not commit).
    repo.merge(&[patch_commit], Some(merge_opts.patience(true)), None)
        .context("merge failed")?;

    // Commit staged changes.
    let commit = repo.find_commit(patch_commit.id())?;
    let author = commit.author();
    let committer = repo
        .signature()
        .context("git user name or email not configured")?;

    let tree = repo.index()?.write_tree()?;
    let tree = repo.find_tree(tree)?;
    let parents = &[&repo.head()?.peel_to_commit()?, &commit];

    repo.commit(
        Some("HEAD"),
        &author,
        &committer,
        &merge_msg,
        &tree,
        parents,
    )
    .context("merge commit failed")?;

    // Cleanup merge state.
    repo.cleanup_state().context("merge state cleanup failed")?;

    Ok(())
}

/// Perform fast-forward merge of patch.
fn fast_forward(repo: &git::raw::Repository, patch_oid: &git::Oid) -> anyhow::Result<()> {
    let oid = patch_oid.to_string();
    let args = ["merge", "--ff-only", &oid];

    term::subcommand(format!("git {}", args.join(" ")));
    let output = git::run::<_, _, &str, &str>(
        repo.workdir()
            .ok_or_else(|| anyhow!("cannot fast-forward in bare repo"))?,
        args,
        [],
    )
    .context("fast-forward failed")?;

    term::blob(output);

    Ok(())
}

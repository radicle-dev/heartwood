use std::ffi::OsString;
use std::fmt;
use std::fmt::Write;
use std::str::FromStr;

use anyhow::{anyhow, Context};

use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};
use radicle::cob::patch::RevisionIx;
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

    rad merge [<id>] [<option>...]

    To specify a patch to merge, use the fully qualified patch id.

Options

    -i, --interactive         Ask for confirmations
    -r, --revision <number>   Revision number to merge, defaults to the latest
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
    pub id: PatchId,
    pub interactive: bool,
    pub revision: Option<RevisionIx>,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut id: Option<PatchId> = None;
        let mut revision: Option<RevisionIx> = None;
        let mut interactive = false;

        while let Some(arg) = parser.next()? {
            match arg {
                Long("help") => {
                    return Err(Error::Help.into());
                }
                Long("interactive") | Short('i') => {
                    interactive = true;
                }
                Long("revision") | Short('r') => {
                    let value = parser.value()?;
                    let id =
                        RevisionIx::from_str(value.to_str().unwrap_or_default()).map_err(|_| {
                            anyhow!("invalid revision number `{}`", value.to_string_lossy())
                        })?;
                    revision = Some(id);
                }
                Value(val) => {
                    let val = val
                        .to_str()
                        .ok_or_else(|| anyhow!("patch id specified is not UTF-8"))?;

                    id = Some(
                        PatchId::from_str(val)
                            .map_err(|_| anyhow!("invalid patch id '{}'", val))?,
                    );
                }
                _ => return Err(anyhow::anyhow!(arg.unexpected())),
            }
        }

        Ok((
            Options {
                id: id.ok_or_else(|| anyhow!("a patch id to merge must be provided"))?,
                interactive,
                revision,
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
        .identity_of(profile.id())
        .context(format!("couldn't load project {id} from local state"))?;
    let repository = profile.storage.repository(id)?;
    let mut patches = Patches::open(*profile.id(), &repository)?;

    if repo.head_detached()? {
        anyhow::bail!("HEAD is in a detached state; can't merge");
    }

    //
    // Get patch information
    //
    let patch_id = options.id;
    let mut patch = patches
        .get_mut(&patch_id)
        .map_err(|e| anyhow!("couldn't find patch {} locally: {e}", &options.id))?;

    let head = repo.head()?;
    let branch = head
        .shorthand()
        .ok_or_else(|| anyhow!("invalid head branch"))?;
    let head_oid = head
        .target()
        .ok_or_else(|| anyhow!("cannot merge into detatched head; aborting"))?;
    let revision_ix = options.revision.unwrap_or_else(|| patch.version());
    let (revision_id, revision) = patch
        .revisions()
        .nth(revision_ix)
        .ok_or_else(|| anyhow!("revision R{} does not exist", revision_ix))?;

    //
    // Analyze merge
    //
    let patch_commit = repo
        .find_annotated_commit(revision.oid.into())
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
        let their_commit = repo.find_commit(revision.oid.into())?;

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
        term::info!(
            "✓ Patch {} is already part of {}",
            term::format::tertiary(patch_id),
            term::format::highlight(branch)
        );

        return Ok(());
    } else if merge.is_unborn() {
        anyhow::bail!("HEAD does not point to a valid commit");
    } else {
        anyhow::bail!(
            "no merge is possible between {} and {}",
            head_oid,
            revision.oid
        );
    };

    let merge_style_pretty = match merge_style {
        MergeStyle::FastForward => term::format::style(merge_style.to_string())
            .dim()
            .italic()
            .to_string(),
        MergeStyle::Commit => term::format::style(merge_style.to_string())
            .yellow()
            .italic()
            .to_string(),
    };

    term::info!(
        "{} {} {} ({}) by {} into {} ({}) via {}...",
        term::format::bold("Merging"),
        term::format::tertiary(term::format::cob(&patch_id)),
        term::format::dim(format!("R{revision_ix}")),
        term::format::secondary(term::format::oid(revision.oid)),
        term::format::tertiary(patch.author().id),
        term::format::highlight(branch),
        term::format::secondary(term::format::oid(head_oid)),
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
            fast_forward(&repo, &revision.oid)?;
        }
    }

    term::success!(
        "Updated {} {} -> {} via {}",
        term::format::highlight(branch),
        term::format::secondary(term::format::oid(head_oid)),
        term::format::secondary(term::format::oid(revision.oid)),
        merge_style_pretty
    );

    //
    // Update patch COB
    //
    // TODO: Don't allow merging the same revision twice?
    patch.merge(*revision_id, head_oid.into(), &signer)?;

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
    let description = patch.description().unwrap_or_default().trim();
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
    let merge_msg = match term::Editor::new()
        .require_save(true)
        .trim_newlines(true)
        .extension(".git-commit")
        .edit(&merge_msg)
        .unwrap()
    {
        Some(s) => s
            .lines()
            .filter(|l| !l.starts_with('#'))
            .collect::<Vec<_>>()
            .join("\n"),
        None => anyhow::bail!("user aborted merge"),
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

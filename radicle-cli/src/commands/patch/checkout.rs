use anyhow::anyhow;

use git_ref_format::Qualified;
use radicle::cob::patch;
use radicle::cob::patch::RevisionId;
use radicle::git::RefString;
use radicle::patch::cache::Patches as _;
use radicle::patch::PatchId;
use radicle::storage::git::Repository;
use radicle::{git, rad, Profile};

use crate::terminal as term;

#[derive(Debug, Default)]
pub struct Options {
    pub name: Option<RefString>,
    pub remote: Option<RefString>,
    pub force: bool,
}

impl Options {
    fn branch(&self, id: &PatchId) -> anyhow::Result<RefString> {
        match &self.name {
            Some(refname) => Ok(Qualified::from_refstr(refname)
                .map_or_else(|| refname.clone(), |q| q.to_ref_string())),
            // SAFETY: Patch IDs are valid refstrings.
            None => Ok(git::refname!("patch")
                .join(RefString::try_from(term::format::cob(id).item).unwrap())),
        }
    }
}

pub fn run(
    patch_id: &PatchId,
    revision_id: Option<RevisionId>,
    stored: &Repository,
    working: &git::raw::Repository,
    profile: &Profile,
    opts: Options,
) -> anyhow::Result<()> {
    let patches = term::cob::patches(profile, stored)?;
    let patch = patches
        .get(patch_id)?
        .ok_or_else(|| anyhow!("Patch `{patch_id}` not found"))?;

    let (revision_id, revision) = match revision_id {
        Some(id) => (
            id,
            patch
                .revision(&id)
                .ok_or_else(|| anyhow!("Patch revision `{id}` not found"))?,
        ),
        None => patch.latest(),
    };

    let mut spinner = term::spinner("Performing checkout...");
    let patch_branch = opts.branch(patch_id)?;

    let commit =
        match working.find_branch(patch_branch.as_str(), radicle::git::raw::BranchType::Local) {
            Ok(branch) if opts.force => {
                let commit = find_patch_commit(revision, stored, working)?;
                let mut r = branch.into_reference();
                r.set_target(commit.id(), &format!("force update '{patch_branch}'"))?;
                commit
            }
            Ok(branch) => {
                let head = branch.get().peel_to_commit()?;
                if head.id() != *revision.head() {
                    anyhow::bail!(
                        "branch '{patch_branch}' already exists (use `--force` to overwrite)"
                    );
                }
                head
            }
            Err(e) if radicle::git::is_not_found_err(&e) => {
                let commit = find_patch_commit(revision, stored, working)?;
                // Create patch branch and switch to it.
                working.branch(patch_branch.as_str(), &commit, true)?;
                commit
            }
            Err(e) => return Err(e.into()),
        };

    if opts.force {
        let mut builder = radicle::git::raw::build::CheckoutBuilder::new();
        builder.force();
        working.checkout_tree(commit.as_object(), Some(&mut builder))?;
    } else {
        working.checkout_tree(commit.as_object(), None)?;
    }
    working.set_head(&git::refs::workdir::branch(&patch_branch))?;

    spinner.message(format!(
        "Switched to branch {} at revision {}",
        term::format::highlight(&patch_branch),
        term::format::dim(term::format::oid(revision_id)),
    ));
    spinner.finish();

    if let Some(branch) = rad::setup_patch_upstream(
        patch_id,
        revision.head(),
        working,
        opts.remote.as_ref().unwrap_or(&radicle::rad::REMOTE_NAME),
        false,
    )? {
        let tracking = branch
            .name()?
            .ok_or_else(|| anyhow!("failed to create tracking branch: invalid name"))?;
        term::success!(
            "Branch {} setup to track {}",
            term::format::highlight(patch_branch),
            term::format::tertiary(tracking)
        );
    }
    Ok(())
}

/// Try to find the patch head in our working copy, and if we don't find it,
/// fetch it from storage first.
fn find_patch_commit<'a>(
    revision: &patch::Revision,
    stored: &Repository,
    working: &'a git::raw::Repository,
) -> anyhow::Result<git::raw::Commit<'a>> {
    let head = *revision.head();
    let workdir = working
        .workdir()
        .ok_or(anyhow::anyhow!("repository is a bare git repository "))?;

    match working.find_commit(head) {
        Ok(commit) => Ok(commit),
        Err(e) if git::ext::is_not_found_err(&e) => {
            git::process::fetch_local(workdir, stored, [head.into()])?;
            working.find_commit(head).map_err(|e| e.into())
        }
        Err(e) => Err(e.into()),
    }
}

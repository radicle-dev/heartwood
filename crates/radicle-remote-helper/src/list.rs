use radicle::patch::cache::Patches as _;
use radicle::profile;
use thiserror::Error;

use radicle::Profile;
use radicle::cob;
use radicle::git;
use radicle::prelude::NodeId;
use radicle::storage::ReadRepository;
use radicle::storage::git::transport::local::Url;

#[derive(Debug, Error)]
pub(super) enum Error {
    /// Storage error.
    #[error(transparent)]
    Storage(#[from] radicle::storage::Error),
    /// Identity error.
    #[error(transparent)]
    Identity(#[from] radicle::identity::DocError),
    /// Git error.
    #[error(transparent)]
    Git(#[from] radicle::git::raw::Error),
    /// Profile error.
    #[error(transparent)]
    Profile(#[from] profile::Error),
    /// COB store error.
    #[error(transparent)]
    CobStore(#[from] cob::store::Error),
    /// Patch COB cache error.
    #[error(transparent)]
    PatchCache(#[from] radicle::patch::cache::Error),
    /// General repository error.
    #[error(transparent)]
    Repository(#[from] radicle::storage::RepositoryError),
}

/// List refs for fetching (`git fetch` and `git ls-remote`).
pub(super) fn for_fetch<R: ReadRepository + cob::Store<Namespace = NodeId> + 'static>(
    url: &Url,
    profile: &Profile,
    stored: &R,
) -> Result<Vec<String>, Error> {
    let mut lines = Vec::new();

    if let Some(namespace) = url.namespace {
        // Listing namespaced refs.
        for (name, oid) in stored.references_of(&namespace)? {
            lines.push(format!("{oid} {name}"));
        }
    } else {
        // List the symbolic reference `HEAD`, which is interpreted by
        // Git clients to determine the default branch.
        match stored.head() {
            Ok((target, _)) => lines.push(format!("@{target} HEAD")),
            Err(err) => eprintln!("remote: error resolving HEAD: {err}"),
        }

        // List canonical references.
        // Skip over `refs/rad/*`, since those are not meant to be fetched into a working copy.
        for glob in [
            git::fmt::pattern!("refs/heads/*"),
            git::fmt::pattern!("refs/tags/*"),
        ] {
            for (name, oid) in stored.references_glob(&glob)? {
                lines.push(format!("{oid} {name}"));
            }
        }

        // List the patch refs, but do not abort if there is an error,
        // as this would break all fetch behavior.
        // Instead, just output an error to the user.
        match patch_refs(profile, stored) {
            Ok(mut refs) => lines.append(&mut refs),
            Err(e) => eprintln!("remote: error listing patch refs: {e}"),
        }
    }

    Ok(lines)
}

/// List refs for pushing (`git push`).
pub(super) fn for_push<R: ReadRepository>(
    profile: &Profile,
    stored: &R,
) -> Result<Vec<String>, Error> {
    let mut lines = Vec::new();

    // Only our own refs can be pushed to.
    for (name, oid) in stored.references_of(profile.id())? {
        // Only branches and tags can be pushed to.
        if name.starts_with(git::fmt::refname!("refs/heads").as_str())
            || name.starts_with(git::fmt::refname!("refs/tags").as_str())
        {
            lines.push(format!("{oid} {name}"));
        }
    }

    Ok(lines)
}

/// List canonical patch references. These are magic refs that can be used to pull patch updates.
fn patch_refs<R: ReadRepository + cob::Store<Namespace = NodeId> + 'static>(
    profile: &Profile,
    stored: &R,
) -> Result<Vec<String>, Error> {
    let mut lines = Vec::new();
    let patches = crate::patches(profile, stored)?;
    for patch in patches.list()? {
        let Ok((id, patch)) = patch else {
            // Ignore patches that fail to decode.
            continue;
        };
        let head = patch.head();

        if patch.is_open() && stored.commit(*head).is_ok() {
            lines.push(format!("{} {}", patch.head(), git::refs::patch(&id)));
        }
    }
    Ok(lines)
}

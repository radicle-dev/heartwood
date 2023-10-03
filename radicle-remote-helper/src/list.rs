use thiserror::Error;

use radicle::cob;
use radicle::git;
use radicle::storage::git::transport::local::Url;
use radicle::storage::ReadRepository;
use radicle::Profile;

#[derive(Debug, Error)]
pub enum Error {
    /// Storage error.
    #[error(transparent)]
    Storage(#[from] radicle::storage::Error),
    /// Identity error.
    #[error(transparent)]
    Identity(#[from] radicle::identity::DocError),
    /// Git error.
    #[error(transparent)]
    Git(#[from] radicle::git::ext::Error),
    /// COB store error.
    #[error(transparent)]
    CobStore(#[from] cob::store::Error),
    /// General repository error.
    #[error(transparent)]
    Repository(#[from] radicle::storage::RepositoryError),
}

/// List refs for fetching (`git fetch` and `git ls-remote`).
pub fn for_fetch<R: ReadRepository + cob::Store + 'static>(
    url: &Url,
    stored: &R,
) -> Result<(), Error> {
    if let Some(namespace) = url.namespace {
        // Listing namespaced refs.
        for (name, oid) in stored.references_of(&namespace)? {
            println!("{oid} {name}");
        }
    } else {
        // Listing canonical refs.
        // We skip over `refs/rad/*`, since those are not meant to be fetched into a working copy.
        for glob in [
            git::refspec::pattern!("refs/heads/*"),
            git::refspec::pattern!("refs/tags/*"),
        ] {
            for (name, oid) in stored.references_glob(&glob)? {
                println!("{oid} {name}");
            }
        }
        // List the patch refs, but don't abort if there's an error, as this would break
        // all fetch behavior. Instead, just output an error to the user.
        if let Err(e) = patch_refs(stored) {
            eprintln!("remote: error listing patch refs: {e}");
        }
    }
    println!();

    Ok(())
}

/// List refs for pushing (`git push`).
pub fn for_push<R: ReadRepository>(profile: &Profile, stored: &R) -> Result<(), Error> {
    // Only our own refs can be pushed to.
    for (name, oid) in stored.references_of(profile.id())? {
        // Only branches and tags can be pushed to.
        if name.starts_with(git::refname!("refs/heads").as_str())
            || name.starts_with(git::refname!("refs/tags").as_str())
        {
            println!("{oid} {name}");
        }
    }
    println!();

    Ok(())
}

/// List canonical patch references. These are magic refs that can be used to pull patch updates.
fn patch_refs<R: ReadRepository + cob::Store + 'static>(stored: &R) -> Result<(), Error> {
    let patches = radicle::cob::patch::Patches::open(stored)?;
    for patch in patches.all()? {
        let Ok((id, patch)) = patch else {
            // Ignore patches that fail to decode.
            continue;
        };
        let head = patch.head();

        if patch.is_open() && stored.commit(*head).is_ok() {
            println!("{} {}", patch.head(), git::refs::storage::patch(&id));
        }
    }
    Ok(())
}

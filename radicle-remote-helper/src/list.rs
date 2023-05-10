use thiserror::Error;

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
    Identity(#[from] radicle::identity::IdentityError),
    /// Git error.
    #[error(transparent)]
    Git(#[from] radicle::git::ext::Error),
}

/// List refs for fetching (`git fetch` and `git ls-remote`).
pub fn for_fetch<R: ReadRepository>(url: &Url, stored: &R) -> Result<(), Error> {
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
    }
    println!();

    Ok(())
}

/// List refs for pushing (`git push`).
pub fn for_push<R: ReadRepository>(profile: &Profile, stored: &R) -> Result<(), Error> {
    // Only our own refs can be pushed to.
    for (name, oid) in stored.references_of(profile.id())? {
        println!("{oid} {name}");
    }
    println!();

    Ok(())
}

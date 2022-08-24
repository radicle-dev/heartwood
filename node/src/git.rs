use crate::identity::ProjId;
use crate::storage::{Error, WriteStorage};

/// Default port of the `git` transport protocol.
pub const PROTOCOL_PORT: u16 = 9418;

/// Fetch all remotes of a project from the given URL.
pub fn fetch<S: WriteStorage>(proj: &ProjId, url: &str, mut storage: S) -> Result<(), Error> {
    // TODO: Use `Url` type?
    // TODO: Have function to fetch specific remotes.
    // TODO: Return meaningful info on success.
    //
    // Repository layout should look like this:
    //
    //      /refs/namespaces/<project>
    //              /refs/namespaces/<remote>
    //                    /heads
    //                      /master
    //                    /tags
    //                    ...
    //
    let repo = storage.repository();
    let refs: &[&str] = &[&format!(
        "refs/namespaces/{}/refs/*:refs/namespaces/{}/refs/*",
        proj, proj
    )];
    let mut remote = repo.remote_anonymous(url)?;
    let mut opts = git2::FetchOptions::default();

    remote.fetch(refs, Some(&mut opts), None)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash::Digest;
    use crate::identity::ProjId;
    use crate::storage::Storage;

    /// Create an initial empty commit.
    fn initial_commit(repo: &git2::Repository) -> Result<git2::Oid, Error> {
        // First use the config to initialize a commit signature for the user.
        let sig = git2::Signature::now("cloudhead", "cloudhead@radicle.xyz")?;
        // Now let's create an empty tree for this commit.
        let tree_id = repo.index()?.write_tree()?;
        let tree = repo.find_tree(tree_id)?;
        let oid = repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])?;

        Ok(oid)
    }

    #[test]
    fn test_fetch() {
        let path = tempfile::tempdir().unwrap().into_path();
        let alice = git2::Repository::init_bare(path.join("alice")).unwrap();
        let bob = git2::Repository::init_bare(path.join("bob")).unwrap();
        let mut bob_storage = Storage::from(bob);
        let proj = ProjId::from(Digest::new(&[42]));
        let master = format!("refs/namespaces/{}/refs/heads/master", proj);
        let alice_oid = initial_commit(&alice).unwrap();

        alice
            .reference(&master, alice_oid, false, "Create master branch")
            .unwrap();

        // Have Bob fetch Alice's refs.
        fetch(
            &proj,
            &format!("file://{}/alice", path.display()),
            &mut bob_storage,
        )
        .unwrap();

        let bob_oid = bob_storage
            .repository()
            .find_reference(&master)
            .unwrap()
            .target()
            .unwrap();

        assert_eq!(alice_oid, bob_oid);
    }
}

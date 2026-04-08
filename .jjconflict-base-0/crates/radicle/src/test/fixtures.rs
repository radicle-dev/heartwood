use std::path::Path;
use std::str::FromStr;

use crate::crypto::PublicKey;
use crate::git;
use crate::identity::RepoId;
use crate::identity::doc::Visibility;
use crate::node::Alias;
use crate::node::device::Device;
use crate::rad;
use crate::storage::git::Storage;
use crate::storage::git::transport;
use crate::storage::refs::SignedRefs;

/// The birth of the radicle project, January 1st, 2018.
pub const RADICLE_EPOCH: i64 = 1514817556;

const USER_NAME: &str = "anonymous";

// TODO: Next time we do something that changes all hashes,
// also change this to "anonymous@radicle.example.com".
const USER_EMAIL: &str = "anonymous@radicle.xyz";

/// Create a new user info object.
pub fn user() -> git::UserInfo {
    git::UserInfo {
        alias: Alias::new("Radcliff"),
        key: PublicKey::from_str("z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi").unwrap(),
    }
}

/// Create a new storage with a project.
pub fn storage<P, G>(path: P, signer: &Device<G>) -> Result<Storage, rad::InitError>
where
    P: AsRef<Path>,
    G: crypto::signature::Signer<crypto::Signature>,
{
    let path = path.as_ref();
    let storage = Storage::open(
        path.join("storage"),
        git::UserInfo {
            alias: Alias::new("Radcliff"),
            key: *signer.public_key(),
        },
    )?;

    transport::local::register(storage.clone());
    transport::remote::mock::register(signer.public_key(), storage.path());

    for (name, desc) in [
        ("acme", "Acme's repository"),
        ("vim", "A text editor"),
        ("rx", "A pixel editor"),
    ] {
        let (repo, _) = repository(path.join("workdir").join(name));
        rad::init(
            &repo,
            name.try_into().unwrap(),
            desc,
            git::fmt::refname!("master"),
            Visibility::default(),
            signer,
            &storage,
        )?;
    }

    Ok(storage)
}

/// Create a new repository at the given path, and initialize it into a project.
pub fn project<P, G>(
    path: P,
    storage: &Storage,
    signer: &Device<G>,
) -> Result<(RepoId, SignedRefs, git::raw::Repository, git::raw::Oid), rad::InitError>
where
    P: AsRef<Path>,
    G: crypto::signature::Signer<crypto::Signature>,
{
    transport::local::register(storage.clone());

    let (working, head) = repository(path);
    let (id, _, refs) = rad::init(
        &working,
        "acme".try_into().unwrap(),
        "Acme's repository",
        git::fmt::refname!("master"),
        Visibility::default(),
        signer,
        storage,
    )?;

    Ok((id, refs, working, head))
}

/// Creates a regular repository at the given path with a couple of commits.
pub fn repository<P: AsRef<Path>>(path: P) -> (git::raw::Repository, git::raw::Oid) {
    let (repo, oid) = repository_with(
        path,
        git::raw::RepositoryInitOptions::new().external_template(false),
    );
    repo.checkout_head(None).unwrap();
    (repo, oid)
}

pub fn bare_repository<P: AsRef<Path>>(path: P) -> (git::raw::Repository, git::raw::Oid) {
    repository_with(
        path,
        git::raw::RepositoryInitOptions::new()
            .external_template(false)
            .bare(true),
    )
}

fn repository_with<P: AsRef<Path>>(
    path: P,
    opts: &mut git::raw::RepositoryInitOptions,
) -> (git::raw::Repository, git::raw::Oid) {
    let repo = git::raw::Repository::init_opts(path, opts).unwrap();

    {
        let mut config = repo.config().unwrap();
        config.set_str("user.name", USER_NAME).unwrap();
        config.set_str("user.email", USER_EMAIL).unwrap();
    }

    let sig = git::raw::Signature::new(
        USER_NAME,
        USER_EMAIL,
        &git::raw::Time::new(RADICLE_EPOCH, 0),
    )
    .unwrap();

    let head = git::initial_commit(&repo, &sig).unwrap();
    let tree = git::write_tree(Path::new("README"), "Hello World!\n".as_bytes(), &repo).unwrap();
    let oid = {
        let commit = git::commit(
            &repo,
            &head,
            git::fmt::refname!("refs/heads/master").as_refstr(),
            "Second commit",
            &sig,
            &tree,
        )
        .unwrap();

        commit.id()
    };
    repo.set_head("refs/heads/master").unwrap();

    drop(tree);
    drop(head);

    (repo, oid)
}

/// Create an empty commit on the current branch.
pub fn commit(msg: &str, parents: &[git::raw::Oid], repo: &git::raw::Repository) -> git::Oid {
    let head = repo.head().unwrap();
    let sig = git::raw::Signature::new(
        USER_NAME,
        USER_EMAIL,
        &git::raw::Time::new(RADICLE_EPOCH, 0),
    )
    .unwrap();
    let tree = head.peel_to_commit().unwrap().tree().unwrap();
    let parents = parents
        .iter()
        .map(|p| repo.find_commit(*p).unwrap())
        .collect::<Vec<_>>();
    let parents = parents.iter().collect::<Vec<_>>(); // Get references.

    repo.commit(None, &sig, &sig, msg, &tree, &parents)
        .unwrap()
        .into()
}

/// Create an (annotated) tag of the given commit.
pub fn tag(
    name: &str,
    message: &str,
    commit: git::raw::Oid,
    repo: &git::raw::Repository,
) -> git::Oid {
    let target = repo
        .find_object(commit, Some(git::raw::ObjectType::Commit))
        .unwrap();
    let tagger = git::raw::Signature::new(
        USER_NAME,
        USER_EMAIL,
        &git::raw::Time::new(RADICLE_EPOCH, 0),
    )
    .unwrap();
    repo.tag(name, &target, &tagger, message, false)
        .unwrap()
        .into()
}

/// Populate a repository with commits, branches and blobs.
pub fn populate(repo: &git::raw::Repository, scale: usize) -> Vec<git::fmt::Qualified<'_>> {
    assert!(
        scale <= 8,
        "Scale parameter must be less than or equal to 8"
    );
    if scale == 0 {
        return vec![];
    }
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    let mut rng = fastrand::Rng::with_seed(42);
    let mut buffer = vec![0; 1024 * 1024 * scale];
    let mut refs = Vec::new();

    for _ in 0..scale {
        let random = std::iter::repeat_with(|| rng.alphanumeric())
            .take(7)
            .collect::<String>()
            .to_lowercase();
        let name = git::fmt::refname!("feature")
            .join(git::fmt::RefString::try_from(random.as_str()).unwrap());
        let signature = git::raw::Signature::now("Radicle", "radicle@radicle.xyz").unwrap();

        rng.fill(&mut buffer);

        let blob = repo.blob(&buffer).unwrap();
        let mut builder = repo.treebuilder(None).unwrap();
        builder.insert("random.txt", blob, 0o100_644).unwrap();
        let tree_oid = builder.write().unwrap();
        let tree = repo.find_tree(tree_oid).unwrap();
        let refstr = git::refs::workdir::branch(&name);

        repo.commit(
            Some(&refstr),
            &signature,
            &signature,
            &format!("Initialize new branch {name}"),
            &tree,
            &[&head],
        )
        .unwrap();

        refs.push(git::fmt::Qualified::from_refstr(refstr).unwrap());
    }
    refs
}

/// Generate random fixtures.
pub mod r#gen {
    use super::*;

    /// Generate a random string of the given length.
    pub fn string(length: usize) -> String {
        std::iter::repeat_with(fastrand::alphabetic)
            .take(length)
            .collect::<String>()
    }

    /// Generate a random email.
    pub fn email() -> String {
        format!("{}@{}.xyz", string(6), string(6))
    }

    /// Creates a regular repository at the given path with a couple of commits.
    pub fn repository<P: AsRef<Path>>(path: P) -> (git::raw::Repository, git::raw::Oid) {
        let repo = git::raw::Repository::init_opts(
            path,
            git::raw::RepositoryInitOptions::new().external_template(false),
        )
        .unwrap();
        let sig = git::raw::Signature::now(string(6).as_str(), email().as_str()).unwrap();
        let head = git::initial_commit(&repo, &sig).unwrap();
        let tree =
            git::write_tree(Path::new("README"), "Hello World!\n".as_bytes(), &repo).unwrap();
        let oid = git::commit(
            &repo,
            &head,
            git::fmt::refname!("refs/heads/master").as_refstr(),
            string(16).as_str(),
            &sig,
            &tree,
        )
        .unwrap()
        .id();

        // Look, I don't really understand why we have to do this, but we do.
        drop(head);
        drop(tree);

        (repo, oid)
    }
}

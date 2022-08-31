use std::path::Path;
use std::{fs, io};

use nonempty::NonEmpty;
use thiserror::Error;

use crate::identity;
use crate::identity::{ProjId, UserId};

#[derive(Error, Debug)]
pub enum InitError {
    #[error("doc: {0}")]
    Doc(#[from] identity::DocError),
    #[error("git: {0}")]
    Git(#[from] git2::Error),
    #[error("i/o: {0}")]
    Io(#[from] io::Error),
    #[error("cannot initialize project inside a bare repository")]
    BareRepo,
    #[error("cannot initialize project from detached head state")]
    DetachedHead,
    #[error("HEAD reference is not valid UTF-8")]
    InvalidHead,
}

/// Initialize a new radicle project from a git repository.
pub fn init(
    repo: &git2::Repository,
    name: &str,
    description: &str,
    delegate: UserId,
) -> Result<ProjId, InitError> {
    let delegate = identity::Delegate {
        // TODO: Use actual user name.
        name: String::from("anonymous"),
        id: identity::Did::from(delegate),
    };

    let head = repo.head()?;
    let default_branch = if head.is_branch() {
        head.shorthand().ok_or(InitError::InvalidHead)?.to_owned()
    } else {
        return Err(InitError::DetachedHead);
    };

    let doc = identity::Doc {
        name: name.to_owned(),
        description: description.to_owned(),
        default_branch,
        version: 1,
        parent: None,
        delegate: NonEmpty::new(delegate),
    };
    let sig = repo
        .signature()
        .or_else(|_| git2::Signature::now("anonymous", "anonymous@anonymous.xyz"))?;

    let base = repo.workdir().ok_or(InitError::BareRepo)?;
    let filename = Path::new("Project.toml");
    let path = base.join(filename);
    let file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&path)?;
    let id = doc.write(file)?;

    let mut index = repo.index()?;
    index.add_path(filename)?;

    let tree_id = index.write_tree()?;
    let tree = repo.find_tree(tree_id)?;
    let _oid = repo.commit(
        Some("refs/heads/rad/id"),
        &sig,
        &sig,
        "Initialize Radicle",
        &tree,
        &[],
    )?;

    // Remove identity document from current branch.
    // FIXME: We shouldn't have to do this, as the user may have an unrelated file
    // called the same name. Ideally we are able to create the file in the id branch.
    fs::remove_file(path)?;

    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::Signer;
    use crate::git;
    use crate::test::crypto;

    #[test]
    fn test_init() {
        let tempdir = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(tempdir.path()).unwrap();
        let sig = git2::Signature::now("anonymous", "anonymous@radicle.xyz").unwrap();
        let head = git::initial_commit(&repo, &sig).unwrap();
        let head = git::commit(&repo, &head, "Second commit", "anonymous").unwrap();

        repo.branch("master", &head, false).unwrap();

        let signer = crypto::MockSigner::new(&mut fastrand::Rng::new());
        let delegate = *signer.public_key();

        init(&repo, "acme", "Acme's repo", delegate).unwrap();
    }
}

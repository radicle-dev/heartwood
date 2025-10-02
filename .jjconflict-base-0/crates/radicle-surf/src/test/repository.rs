use std::{convert::Infallible, path::Path};

use git2::{Oid, RepositoryInitOptions};
use radicle_git_metadata::commit::CommitData;
use radicle_git_ref_format::RefString;

use crate::test::r#gen::commit::{self, TreeData};
pub struct Fixture {
    #[allow(unused)] // Prevent early removal of the temporary directory.
    dir: tempfile::TempDir,

    pub inner: git2::Repository,
    pub head: Option<git2::Oid>,
}

/// Initialise a [`git2::Repository`] in a temporary directory.
///
/// The provided `commits` will be added to the repository, and the
/// head commit will be returned.
pub fn fixture(refname: &RefString, commits: Vec<CommitData<TreeData, Infallible>>) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let repo = git2::Repository::init_opts(
        dir.path(),
        RepositoryInitOptions::new().initial_head(refname),
    )
    .unwrap();
    let commits = commit::write_commits(&repo, commits).unwrap();
    let head = commits.last().copied();

    if let Some(head) = head {
        repo.reference(refname.as_str(), head, false, "Initialise repository")
            .unwrap();
    }

    Fixture {
        dir,
        inner: repo,
        head,
    }
}

pub fn bare_fixture(
    refname: &RefString,
    commits: Vec<CommitData<TreeData, Infallible>>,
) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let repo = git2::Repository::init_opts(
        dir.path(),
        RepositoryInitOptions::new()
            .initial_head(refname)
            .bare(true),
    )
    .unwrap();
    let commits = commit::write_commits(&repo, commits).unwrap();
    let head = commits.last().copied();

    if let Some(head) = head {
        repo.reference(refname.as_str(), head, false, "Initialise repository")
            .unwrap();
    }

    Fixture {
        dir,
        inner: repo,
        head,
    }
}

pub fn submodule<'a>(
    parent: &'a git2::Repository,
    child: &'a git2::Repository,
    refname: &RefString,
    head: Oid,
    author: &git2::Signature,
) -> git2::Submodule<'a> {
    let url = format!("file://{}", child.path().canonicalize().unwrap().display());
    let mut sub = parent
        .submodule(url.as_str(), Path::new("submodule"), true)
        .unwrap();
    let _ = sub.open().unwrap();
    let _ = sub
        .clone(Some(&mut git2::SubmoduleUpdateOptions::default()))
        .unwrap();
    sub.add_to_index(true).unwrap();
    sub.add_finalize().unwrap();
    {
        let mut ix = parent.index().unwrap();
        let tree = ix.write_tree_to(parent).unwrap();
        let tree = parent.find_tree(tree).unwrap();
        let head = parent.find_commit(head).unwrap();
        parent
            .commit(
                Some(refname.as_str()),
                author,
                author,
                "Commit submodule",
                &tree,
                &[&head],
            )
            .unwrap();
    }
    sub
}

use std::{convert::Infallible, path::Path};

use proptest::{collection, proptest};
use radicle_git_metadata::commit::CommitData;
use radicle_git_ref_format::refname;

use crate::tree::EntryKind;
use crate::{Branch, Repository, fs};

use super::r#gen;

proptest! {
    #[test]
    fn test_submodule(
        initial in r#gen::commit::commit(),
        commits in collection::vec(r#gen::commit::commit(), 1..5)
    ) {
        prop::test_submodule(initial, commits)
    }

    #[ignore = "segfault"]
    #[test]
    fn test_submodule_bare(
        initial in r#gen::commit::commit(),
        commits in collection::vec(r#gen::commit::commit(), 1..5)
    ) {
        prop::test_submodule_bare(initial, commits)
    }

}

mod prop {
    use crate::test::r#gen::commit;
    use crate::test::repository;

    use super::*;

    pub fn test_submodule(
        initial: CommitData<commit::TreeData, Infallible>,
        commits: Vec<CommitData<commit::TreeData, Infallible>>,
    ) {
        let refname = refname!("refs/heads/master");
        let author = git2::Signature::try_from(initial.author()).unwrap();

        let submodule = repository::fixture(&refname, commits);
        let repo = repository::fixture(&refname, vec![initial]);

        let head = repo.head.expect("missing initial commit");
        let sub = repository::submodule(&repo.inner, &submodule.inner, &refname, head, &author);

        let repo = Repository::open(repo.inner.path()).unwrap();
        let branch = Branch::local(refname);
        let dir = repo.root_dir(&branch).unwrap();

        let platinum = dir.find_entry(&sub.path(), &repo).unwrap();
        assert!(matches!(&platinum, fs::Entry::Submodule(module) if module.url().is_some()));

        let root = repo.tree(&branch, &Path::new("")).unwrap();
        let kind = EntryKind::from(platinum);
        assert!(root.entries().iter().any(|e| e.entry() == &kind));
    }

    pub fn test_submodule_bare(
        initial: CommitData<commit::TreeData, Infallible>,
        commits: Vec<CommitData<commit::TreeData, Infallible>>,
    ) {
        let refname = refname!("refs/heads/master");
        let author = git2::Signature::try_from(initial.author()).unwrap();

        let submodule = repository::fixture(&refname, commits);
        let repo = repository::bare_fixture(&refname, vec![initial]);

        let head = repo.head.expect("missing initial commit");
        let sub = repository::submodule(&repo.inner, &submodule.inner, &refname, head, &author);

        let repo = Repository::open(repo.inner.path()).unwrap();
        let branch = Branch::local(refname);
        let dir = repo.root_dir(&branch).unwrap();

        let platinum = dir.find_entry(&sub.path(), &repo).unwrap();
        assert!(matches!(&platinum, fs::Entry::Submodule(module) if module.url().is_some()));

        let root = repo.tree(&branch, &Path::new("")).unwrap();
        let kind = EntryKind::from(platinum);
        assert!(root.entries().iter().any(|e| e.entry() == &kind));
    }
}

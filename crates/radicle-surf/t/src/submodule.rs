use std::{convert::Infallible, path::Path};

use proptest::{collection, proptest};
use radicle_git_ext::commit::CommitData;
use radicle_git_ext::ref_format::refname;
use radicle_git_ext_test::gen;
use radicle_surf::tree::EntryKind;
use radicle_surf::{fs, Branch, Repository};

proptest! {
    #[test]
    fn test_submodule(
        initial in gen::commit::commit(),
        commits in collection::vec(gen::commit::commit(), 1..5)
    ) {
        prop::test_submodule(initial, commits)
    }

    #[ignore = "segfault"]
    #[test]
    fn test_submodule_bare(
        initial in gen::commit::commit(),
        commits in collection::vec(gen::commit::commit(), 1..5)
    ) {
        prop::test_submodule_bare(initial, commits)
    }

}

mod prop {
    use radicle_git_ext_test::{gen::commit, repository};

    use super::*;

    pub fn test_submodule(
        initial: CommitData<commit::TreeData, Infallible>,
        commits: Vec<CommitData<commit::TreeData, Infallible>>,
    ) {
        let refname = refname!("refs/heads/master");
        let author = git2::Signature::try_from(initial.author()).unwrap();

        let submodule = repository::fixture(&refname, commits).unwrap();
        let repo = repository::fixture(&refname, vec![initial]).unwrap();

        let head = repo.head.expect("missing initial commit");
        let sub =
            repository::submodule(&repo.inner, &submodule.inner, &refname, head, &author).unwrap();

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

        let submodule = repository::fixture(&refname, commits).unwrap();
        let repo = repository::bare_fixture(&refname, vec![initial]).unwrap();

        let head = repo.head.expect("missing initial commit");
        let sub =
            repository::submodule(&repo.inner, &submodule.inner, &refname, head, &author).unwrap();

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

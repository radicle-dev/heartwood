use std::path::Path;

use radicle_git_ext::ref_format::refname;
use radicle_surf::{
    fs::{self, Directory},
    Branch, Repository,
};

use super::GIT_PLATINUM;

#[test]
fn iterate_root_dir_recursive() {
    let repo = Repository::open(GIT_PLATINUM).unwrap();

    let root_dir = repo.root_dir(Branch::local(refname!("master"))).unwrap();
    let count = println_dir(&root_dir, &repo);

    assert_eq!(count, 36); // Check total file count.

    /// Prints items in `dir` with `indent_level`.
    /// For sub-directories, will do Depth-First-Search and print
    /// recursively.
    /// Returns the number of items visited (i.e. printed)
    fn println_dir(dir: &Directory, repo: &Repository) -> i32 {
        dir.traverse::<fs::error::Directory, _, _>(
            repo,
            (0, 0),
            &mut |(count, indent_level), entry| {
                println!("> {}{}", " ".repeat(indent_level * 4), entry.name());
                match entry {
                    fs::Entry::File(_) => Ok((count + 1, indent_level)),
                    fs::Entry::Directory(_) => Ok((count + 1, indent_level + 1)),
                    fs::Entry::Submodule(_) => Ok((count + 1, indent_level)),
                }
            },
        )
        .unwrap()
        .0
    }
}

#[test]
fn browse_repo_lazily() {
    let repo = Repository::open(GIT_PLATINUM).unwrap();

    let root_dir = repo.root_dir(Branch::local(refname!("master"))).unwrap();
    let count = root_dir.entries(&repo).unwrap().entries().count();
    assert_eq!(count, 8);
    let count = traverse(&root_dir, &repo);
    assert_eq!(count, 36);

    fn traverse(dir: &Directory, repo: &Repository) -> i32 {
        dir.traverse::<fs::error::Directory, _, _>(repo, 0, &mut |count, _| Ok(count + 1))
            .unwrap()
    }
}

#[test]
fn test_file_history() {
    let repo = Repository::open(GIT_PLATINUM).unwrap();
    let history = repo.history(Branch::local(refname!("dev"))).unwrap();
    let path = Path::new("README.md");
    let mut file_history = history.by_path(&path);
    let commit = file_history.next().unwrap().unwrap();
    let file = repo.get_commit_file(&commit.id, &path).unwrap();
    assert_eq!(file.size(), 67);
}

#[test]
fn test_commit_history() {
    let repo = Repository::open(GIT_PLATINUM).unwrap();
    let head = "a0dd9122d33dff2a35f564d564db127152c88e02";

    // verify `&str` works.
    let h1 = repo.history(head).unwrap();

    // verify `&String` works.
    let head_string = head.to_string();
    let h2 = repo.history(&head_string).unwrap();

    assert_eq!(h1.head().id, h2.head().id);
}

#[test]
fn test_commit_signature() {
    let repo = Repository::open(GIT_PLATINUM).unwrap();
    let commit_with_signature = "e24124b7538658220b5aaf3b6ef53758f0a106dc";
    let signature = repo.extract_signature(commit_with_signature, None).unwrap();
    assert!(signature.is_some());

    let commit_without_signature = "80bacafba303bf0cdf6142921f430ff265f25095";
    let signature = repo
        .extract_signature(commit_without_signature, None)
        .unwrap();
    assert!(signature.is_none());

    let commit_nonexist = "8080808080";
    let signature = repo.extract_signature(commit_nonexist, None);
    assert!(signature.is_err());
}

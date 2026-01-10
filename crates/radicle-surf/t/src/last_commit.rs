use std::{path::PathBuf, str::FromStr};

use radicle_git_ext::{ref_format::refname, Oid};
use radicle_surf::{Branch, Repository};

use super::GIT_PLATINUM;

#[test]
fn readme_missing_and_memory() {
    let repo = Repository::open(GIT_PLATINUM)
        .expect("Could not retrieve ./data/git-platinum as git repository");
    let oid =
        Oid::from_str("d3464e33d75c75c99bfb90fa2e9d16efc0b7d0e3").expect("Failed to parse SHA");

    // memory.rs is commited later so it should not exist here.
    let memory_last_commit_oid = repo
        .last_commit(&"src/memory.rs", oid)
        .expect("Failed to get last commit")
        .map(|commit| commit.id);

    assert_eq!(memory_last_commit_oid, None);

    // README.md exists in this commit.
    let readme_last_commit = repo
        .last_commit(&"README.md", oid)
        .expect("Failed to get last commit")
        .map(|commit| commit.id);

    assert_eq!(readme_last_commit, Some(oid));
}

#[test]
fn folder_svelte() {
    let repo = Repository::open(GIT_PLATINUM)
        .expect("Could not retrieve ./data/git-platinum as git repository");
    // Check that last commit is the actual last commit even if head commit differs.
    let oid =
        Oid::from_str("19bec071db6474af89c866a1bd0e4b1ff76e2b97").expect("Could not parse SHA");

    let expected_commit_id = Oid::from_str("f3a089488f4cfd1a240a9c01b3fcc4c34a4e97b2").unwrap();

    let folder_svelte = repo
        .last_commit(&"examples/Folder.svelte", oid)
        .expect("Failed to get last commit")
        .map(|commit| commit.id);

    assert_eq!(folder_svelte, Some(expected_commit_id));
}

#[test]
fn nest_directory() {
    let repo = Repository::open(GIT_PLATINUM)
        .expect("Could not retrieve ./data/git-platinum as git repository");
    // Check that last commit is the actual last commit even if head commit differs.
    let oid =
        Oid::from_str("19bec071db6474af89c866a1bd0e4b1ff76e2b97").expect("Failed to parse SHA");

    let expected_commit_id = Oid::from_str("2429f097664f9af0c5b7b389ab998b2199ffa977").unwrap();

    let nested_directory_tree_commit_id = repo
        .last_commit(&"this/is/a/really/deeply/nested/directory/tree", oid)
        .expect("Failed to get last commit")
        .map(|commit| commit.id);

    assert_eq!(nested_directory_tree_commit_id, Some(expected_commit_id));
}

#[test]
#[cfg(not(windows))]
fn can_get_last_commit_for_special_filenames() {
    let repo = Repository::open(GIT_PLATINUM)
        .expect("Could not retrieve ./data/git-platinum as git repository");

    // Check that last commit is the actual last commit even if head commit differs.
    let oid =
        Oid::from_str("a0dd9122d33dff2a35f564d564db127152c88e02").expect("Failed to parse SHA");

    let expected_commit_id = Oid::from_str("a0dd9122d33dff2a35f564d564db127152c88e02").unwrap();

    let backslash_commit_id = repo
        .last_commit(&"special/faux\\path", oid)
        .expect("Failed to get last commit")
        .map(|commit| commit.id);
    assert_eq!(backslash_commit_id, Some(expected_commit_id));

    let ogre_commit_id = repo
        .last_commit(&"special/👹👹👹", oid)
        .expect("Failed to get last commit")
        .map(|commit| commit.id);
    assert_eq!(ogre_commit_id, Some(expected_commit_id));
}

#[test]
fn root() {
    let repo = Repository::open(GIT_PLATINUM)
        .expect("Could not retrieve ./data/git-platinum as git repository");
    let rev = Branch::local(refname!("master"));
    let root_last_commit_id = repo
        .last_commit(&PathBuf::new(), rev)
        .expect("Failed to get last commit")
        .map(|commit| commit.id);

    let expected_oid = repo
        .history(Branch::local(refname!("master")))
        .unwrap()
        .head()
        .id;
    assert_eq!(root_last_commit_id, Some(expected_oid));
}

#[test]
fn binary_file() {
    let repo = Repository::open(GIT_PLATINUM)
        .expect("Could not retrieve ./data/git-platinum as git repository");
    let history = repo.history(Branch::local(refname!("dev"))).unwrap();
    let file_commit = history.by_path(&"bin/cat").next();
    assert!(file_commit.is_some());
    println!("file commit: {:?}", &file_commit);
}

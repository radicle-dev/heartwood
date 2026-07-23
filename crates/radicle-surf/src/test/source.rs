use std::path::PathBuf;

use radicle_git_ext::ref_format::refname;
use radicle_surf::{Branch, Glob, Repository};
use serde_json::json;

const GIT_PLATINUM: &str = "../data/git-platinum";

#[test]
fn tree_serialization() {
    let repo = Repository::open(GIT_PLATINUM).unwrap();
    let tree = repo.tree(refname!("refs/heads/master"), &"src").unwrap();

    let expected = json!({
      "oid": "ed52e9f8dfe1d8b374b2a118c25235349a743dd2",
      "entries": [
        {
          "name": "Eval.hs",
          "kind": "blob",
          "oid": "7d6240123a8d8ea8a8376610168a0a4bcb96afd0",
          "commit": "src/Eval.hs"
        },
        {
          "name": "memory.rs",
          "kind": "blob",
          "oid": "b84992d24be67536837f5ab45a943f1b3f501878",
          "commit": "src/memory.rs"
        }
      ],
      "commit": {
        "id": "a0dd9122d33dff2a35f564d564db127152c88e02",
        "author": {
          "name": "Rūdolfs Ošiņš",
          "email": "rudolfs@osins.org",
          "time": 1602778504
        },
        "committer": {
          "name": "GitHub",
          "email": "noreply@github.com",
          "time": 1602778504
        },
        "summary": "Add files with special characters in their filenames (#5)",
        "message": "Add files with special characters in their filenames (#5)\n\n",
        "description": "",
        "parents": [
          "223aaf87d6ea62eef0014857640fd7c8dd0f80b5"
        ]
      },
      "root": "src"
    });

    assert_eq!(
        serde_json::to_value(&tree).unwrap(),
        expected,
        "Got:\n{}",
        serde_json::to_string_pretty(&tree).unwrap()
    )
}

#[test]
fn test_tree_last_commit() {
    let repo = Repository::open(GIT_PLATINUM).unwrap();
    let tree = repo.tree(refname!("refs/heads/master"), &"src").unwrap();
    let last_commit = tree.last_commit(&repo).unwrap();
    assert_ne!(*tree.commit(), last_commit);
    assert_eq!(
        last_commit.id.to_string(),
        "a57846bbc8ced6587bf8329fc4bce970eb7b757e"
    )
}

#[test]
fn repo_tree_empty_branch() {
    let repo = Repository::open(GIT_PLATINUM).unwrap();
    let rev = Branch::local(refname!("empty-branch"));
    let tree = repo.tree(rev, &"").unwrap();
    assert_eq!(tree.entries().len(), 0);

    // Verify the last commit is the empty commit.
    assert_eq!(
        tree.commit().id.to_string(),
        "e972683fe8136bf8a5cb2378cf50303554008049"
    );
}

#[test]
fn repo_tree() {
    let repo = Repository::open(GIT_PLATINUM).unwrap();
    let tree = repo
        .tree("27acd68c7504755aa11023300890bb85bbd69d45", &"src")
        .unwrap();
    assert_eq!(tree.entries().len(), 3);

    let commit_header = tree.commit();
    assert_eq!(
        commit_header.id.to_string(),
        "27acd68c7504755aa11023300890bb85bbd69d45"
    );

    let tree_oid = tree.object_id();
    assert_eq!(
        tree_oid.to_string(),
        "dbd5d80c64a00969f521b96401a315e9481e9561"
    );

    let entries = tree.entries();
    assert_eq!(entries.len(), 3);
    let entry = &entries[0];
    assert!(!entry.is_tree());
    assert_eq!(entry.name(), "Eval.hs");
    assert_eq!(
        entry.object_id().to_string(),
        "8c7447d13b907aa994ac3a38317c1e9633bf0732"
    );
    let commit = entry.commit();
    assert_eq!(
        commit.id.to_string(),
        "27acd68c7504755aa11023300890bb85bbd69d45"
    );
    let last_commit = entry.last_commit(&repo).unwrap();
    assert_eq!(
        last_commit.id.to_string(),
        "e24124b7538658220b5aaf3b6ef53758f0a106dc"
    );

    // Verify that an empty path works for getting the root tree.
    let root_tree = repo
        .tree("27acd68c7504755aa11023300890bb85bbd69d45", &"")
        .unwrap();
    assert_eq!(root_tree.entries().len(), 8);
}

#[test]
fn repo_blob() {
    let repo = Repository::open(GIT_PLATINUM).unwrap();
    let blob = repo
        .blob("27acd68c7504755aa11023300890bb85bbd69d45", &"src/memory.rs")
        .unwrap();

    let blob_oid = blob.object_id();
    assert_eq!(
        blob_oid.to_string(),
        "b84992d24be67536837f5ab45a943f1b3f501878"
    );

    let commit_header = blob.commit();
    assert_eq!(
        commit_header.id.to_string(),
        "e24124b7538658220b5aaf3b6ef53758f0a106dc"
    );

    assert!(!blob.is_binary());

    // Verify the blob content size matches with the file size of "memory.rs"
    let content = blob.content();
    assert_eq!(blob.size(), 6253);

    // Verify to_owned().
    let blob_owned = blob.to_owned();
    assert_eq!(blob_owned.size(), 6253);
    assert_eq!(blob.content(), blob_owned.content());

    // Verify JSON output is the same.
    let json_ref = json!({ "content": content }).to_string();
    let json_owned = json!( {
      "content": blob_owned.content()
    })
    .to_string();
    assert_eq!(json_ref, json_owned);
}

#[test]
fn tree_ordering() {
    let repo = Repository::open(GIT_PLATINUM).unwrap();
    let tree = repo
        .tree(refname!("refs/heads/master"), &PathBuf::new())
        .unwrap();
    assert_eq!(
        tree.entries()
            .iter()
            .map(|entry| entry.name().to_string())
            .collect::<Vec<_>>(),
        vec![
            "bin".to_string(),
            "special".to_string(),
            "src".to_string(),
            "text".to_string(),
            "this".to_string(),
            ".i-am-well-hidden".to_string(),
            ".i-too-am-hidden".to_string(),
            "README.md".to_string(),
        ]
    );
}

#[test]
fn commit_branches() {
    let repo = Repository::open(GIT_PLATINUM).unwrap();
    let init_commit = "d3464e33d75c75c99bfb90fa2e9d16efc0b7d0e3";
    let glob = Glob::all_heads().branches().and(Glob::all_remotes());
    let branches = repo.revision_branches(init_commit, glob).unwrap();

    assert_eq!(branches.len(), 11);

    let refnames: Vec<_> = branches.iter().map(|b| b.refname().to_string()).collect();
    assert_eq!(
        refnames,
        vec![
            "refs/heads/dev",
            "refs/heads/diff-test",
            "refs/heads/empty-branch",
            "refs/heads/master",
            "refs/remotes/banana/orange/pineapple",
            "refs/remotes/banana/pineapple",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/dev",
            "refs/remotes/origin/diff-test",
            "refs/remotes/origin/empty-branch",
            "refs/remotes/origin/master"
        ]
    );
}

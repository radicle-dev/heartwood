use crate::{Glob, Repository};
use radicle_git_ref_format::pattern;

use super::platinum;

#[test]
fn test_branches() {
    let repo = Repository::open(platinum::get()).unwrap();
    let heads = Glob::all_heads();
    let branches = repo.branches(heads.clone()).unwrap();
    for b in branches {
        println!("{}", b.unwrap().refname());
    }
    let branches = repo
        .branches(heads.branches().and(Glob::remotes(pattern!("banana/*"))))
        .unwrap();
    for b in branches {
        println!("{}", b.unwrap().refname());
    }
}

#[test]
fn test_tag_snapshot() {
    let repo = Repository::open(platinum::get()).unwrap();
    let tags = repo
        .tags(&Glob::all_tags())
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(tags.len(), 6);
    let root_dir = repo.root_dir(&tags[0]).unwrap();
    assert_eq!(root_dir.entries(&repo).unwrap().entries().count(), 1);
}

#[test]
fn test_namespaces() {
    let repo = Repository::open(platinum::get()).unwrap();

    let namespaces = repo.namespaces(&Glob::all_namespaces()).unwrap();
    assert_eq!(namespaces.count(), 3);
    let namespaces = repo
        .namespaces(&Glob::namespaces(pattern!("golden/*")))
        .unwrap();
    assert_eq!(namespaces.count(), 2);
    let namespaces = repo
        .namespaces(&Glob::namespaces(pattern!("golden/*")).insert(pattern!("me/*")))
        .unwrap();
    assert_eq!(namespaces.count(), 3);
}

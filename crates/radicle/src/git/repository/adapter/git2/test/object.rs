use std::path::Path;

use radicle_git_metadata::author::{Author, Time};
use radicle_git_metadata::commit::CommitData;
use radicle_git_metadata::commit::headers::Headers;
use radicle_git_metadata::commit::trailers::OwnedTrailer;
use radicle_oid::Oid;

use crate::git::raw::fixture;
use crate::git::repository;
use crate::git::repository::object;
use crate::git::repository::types::TreeEntry;

#[test]
fn blob_found() {
    let repo = fixture::Repository::new();
    let blob_oid = repo.blob(b"hello");
    let blob = object::Reader::blob(repo.raw(), blob_oid).unwrap().unwrap();
    assert_eq!(blob.oid, blob_oid);
    assert_eq!(blob.content, b"hello");
}

#[test]
fn blob_not_found() {
    let repo = fixture::Repository::new();
    let missing = Oid::from_sha1([0xff; 20]);
    assert!(object::Reader::blob(repo.raw(), missing).unwrap().is_none());
}

#[test]
fn try_blob_not_found() {
    let repo = fixture::Repository::new();
    let missing = Oid::from_sha1([0xff; 20]);
    let err = object::Reader::try_blob(repo.raw(), missing).unwrap_err();
    assert!(matches!(err, object::error::read::Blob::NotFound { .. }));
}

#[test]
fn blob_at() {
    let repo = fixture::Repository::new();
    let commit = repo.commit(&[], &[("hello.txt", b"content")]);
    let blob = object::Reader::blob_at(repo.raw(), commit, &Path::new("hello.txt")).unwrap();
    assert_eq!(blob.unwrap().content, b"content");
}

#[test]
fn blob_at_nested() {
    let repo = fixture::Repository::new();
    let commit = repo.commit(&[], &[("sub/nested.txt", b"deep")]);
    let blob = object::Reader::blob_at(repo.raw(), commit, &Path::new("sub/nested.txt")).unwrap();
    assert_eq!(blob.unwrap().content, b"deep");
}

#[test]
fn blob_at_missing_path() {
    let repo = fixture::Repository::new();
    let commit = repo.commit(&[], &[("file.txt", b"x")]);
    assert!(
        object::Reader::blob_at(repo.raw(), commit, &Path::new("nope.txt"))
            .unwrap()
            .is_none()
    );
}

#[test]
fn blob_at_missing_commit() {
    let repo = fixture::Repository::new();
    let missing = Oid::from_sha1([0xff; 20]);
    let err = object::Reader::blob_at(repo.raw(), missing, &Path::new("f")).unwrap_err();
    assert!(matches!(
        err,
        object::error::read::BlobAt::CommitNotFound { .. }
    ));
}

#[test]
fn commit() {
    let repo = fixture::Repository::new();
    let parent = repo.commit(&[], &[("f", b"v1")]);
    let child = repo.commit(&[parent], &[("f", b"v2")]);
    let commit = object::Reader::commit(repo.raw(), child).unwrap().unwrap();
    assert_eq!(commit.parents().next(), Some(parent));
}

#[test]
fn commit_not_found() {
    let repo = fixture::Repository::new();
    let missing = Oid::from_sha1([0xff; 20]);
    assert!(
        object::Reader::commit(repo.raw(), missing)
            .unwrap()
            .is_none()
    );
}

#[test]
fn exists_true() {
    let repo = fixture::Repository::new();
    let oid = repo.blob(b"exists");
    assert!(object::Reader::exists(repo.raw(), oid).unwrap());
}

#[test]
fn exists_false() {
    let repo = fixture::Repository::new();
    let missing = Oid::from_sha1([0xff; 20]);
    assert!(!object::Reader::exists(repo.raw(), missing).unwrap());
}

#[test]
fn object_kind_blob() {
    let repo = fixture::Repository::new();
    let oid = repo.blob(b"kind test");
    assert_eq!(
        object::Reader::object_kind(repo.raw(), oid).unwrap(),
        Some(repository::ObjectKind::Blob)
    );
}

#[test]
fn object_kind_commit() {
    let repo = fixture::Repository::new();
    let commit = repo.commit(&[], &[("f", b"x")]);
    assert_eq!(
        object::Reader::object_kind(repo.raw(), commit).unwrap(),
        Some(repository::ObjectKind::Commit)
    );
}

#[test]
fn object_kind_tag() {
    let repo = fixture::Repository::new();
    let commit = repo.commit(&[], &[("f", b"x")]);
    let tag = repo.tag("v1", commit, true);
    assert_eq!(
        object::Reader::object_kind(repo.raw(), tag).unwrap(),
        Some(repository::ObjectKind::Tag)
    );
}

#[test]
fn object_kind_missing() {
    let repo = fixture::Repository::new();
    let missing = Oid::from_sha1([0xff; 20]);
    assert!(
        object::Reader::object_kind(repo.raw(), missing)
            .unwrap()
            .is_none()
    );
}

#[test]
fn write_blob_roundtrip() {
    let repo = fixture::Repository::new();
    let oid = object::Writer::write_blob(repo.raw(), b"test content").unwrap();
    let blob = object::Reader::blob(repo.raw(), oid).unwrap().unwrap();
    assert_eq!(blob.content, b"test content");
}

#[test]
fn write_tree_inline_blob() {
    let repo = fixture::Repository::new();
    let entries = vec![TreeEntry::Blob {
        path: "file.txt".into(),
        content: b"data".to_vec(),
    }];
    let tree_oid = object::Writer::write_tree(repo.raw(), &entries).unwrap();
    assert!(object::Reader::exists(repo.raw(), tree_oid).unwrap());
}

#[test]
fn write_tree_multi_component_path() {
    let repo = fixture::Repository::new();
    let entries = vec![TreeEntry::Blob {
        path: "a/b/c.txt".into(),
        content: b"deep".to_vec(),
    }];
    let tree_oid = object::Writer::write_tree(repo.raw(), &entries).unwrap();

    let author = Author {
        name: "t".into(),
        email: "t@t".into(),
        time: Time::new(0, 0),
    };
    let commit = CommitData::new::<_, _, OwnedTrailer>(
        tree_oid,
        None::<Oid>,
        author.clone(),
        author,
        Headers::new(),
        "t\n".to_string(),
        vec![],
    );
    let commit_oid =
        object::Writer::write_commit(repo.raw(), commit.to_string().as_bytes()).unwrap();
    let blob = object::Reader::blob_at(repo.raw(), commit_oid, &Path::new("a/b/c.txt")).unwrap();
    assert_eq!(blob.unwrap().content, b"deep");
}

#[test]
fn write_tree_blob_ref() {
    let repo = fixture::Repository::new();
    let blob_oid = object::Writer::write_blob(repo.raw(), b"existing").unwrap();
    let entries = vec![TreeEntry::BlobRef {
        path: "ref.txt".into(),
        oid: blob_oid,
    }];
    object::Writer::write_tree(repo.raw(), &entries).unwrap();
}

#[test]
fn write_tree_blob_ref_missing() {
    let repo = fixture::Repository::new();
    let missing = Oid::from_sha1([0xff; 20]);
    let entries = vec![TreeEntry::BlobRef {
        path: "bad.txt".into(),
        oid: missing,
    }];
    let err = object::Writer::write_tree(repo.raw(), &entries).unwrap_err();
    assert!(matches!(
        err,
        object::error::write::Tree::MissingBlob { .. }
    ));
}

use std::path::Path;

use radicle_oid::Oid;

use crate::git;
use crate::git::raw;
use crate::git::repository::object;
use crate::git::repository::object::error::{read, write};
use crate::git::repository::types::{Blob, Commit, ObjectKind, TreeEntry};

use super::NotFound as _;
use super::object_kind;

impl object::Reader for raw::Repository {
    fn blob(&self, oid: Oid) -> Result<Option<Blob>, read::Blob> {
        self.find_blob(oid.into())
            .map(|blob| Blob {
                oid,
                content: blob.content().to_vec(),
            })
            .or_is_not_found()
            .map_err(read::Blob::backend)
    }

    fn blob_at<P: AsRef<Path>>(&self, oid: Oid, path: &P) -> Result<Option<Blob>, read::BlobAt> {
        let path = path.as_ref();
        let commit = find_commit(self, oid, &path)?;
        let tree = commit.tree().map_err(|e| read::BlobAt::Tree {
            commit: oid,
            source: Box::new(e),
        })?;
        let entry = tree
            .get_path(path)
            .or_is_not_found()
            .map_err(|e| read::BlobAt::TreeEntry {
                commit: oid,
                path: path.to_path_buf(),
                source: Box::new(e),
            })?;
        entry
            .map(|entry| try_entry_to_blob(self, oid, path, entry))
            .transpose()
    }

    fn commit(&self, oid: Oid) -> Result<Option<Commit>, read::Commit> {
        let odb = self.odb().map_err(read::Commit::backend)?;
        let commit = odb
            .read(oid.into())
            .or_is_not_found()
            .map_err(read::Commit::backend)?;
        commit
            .map(|obj| {
                Commit::from_bytes(obj.data()).map_err(|e| read::Commit::Parse { oid, source: e })
            })
            .transpose()
    }

    fn exists(&self, oid: Oid) -> Result<bool, read::Exists> {
        self.odb()
            .map(|odb| odb.exists(oid.into()))
            .map_err(read::Exists::backend)
    }

    fn object_kind(&self, oid: Oid) -> Result<Option<ObjectKind>, read::ObjectKind> {
        let odb = self.odb().map_err(read::ObjectKind::backend)?;
        match odb.read(oid.into()) {
            Ok(obj) => Ok(Some(object_kind(obj.kind()))),
            Err(e) if matches!(e.code(), git2::ErrorCode::NotFound) => Ok(None),
            Err(e) => Err(read::ObjectKind::backend(e)),
        }
    }
}

impl object::Writer for raw::Repository {
    fn write_blob(&self, content: &[u8]) -> Result<Oid, write::Blob> {
        self.blob(content)
            .map(Oid::from)
            .map_err(write::Blob::backend)
    }

    fn write_tree(&self, entries: &[TreeEntry]) -> Result<Oid, write::Tree> {
        let empty_tree = empty_tree(self)?;
        let mut builder = raw::build::TreeUpdateBuilder::new();
        let odb = self.odb().map_err(write::Tree::backend)?;
        for entry in entries {
            match entry {
                TreeEntry::Blob { path, content } => {
                    let oid = self.blob(content).map_err(|e| write::Tree::WriteBlob {
                        path: path.to_path_buf(),
                        source: Box::new(e),
                    })?;
                    builder.upsert(path, oid, raw::FileMode::Blob);
                }
                TreeEntry::BlobRef { path, oid } => {
                    if !odb.exists(oid.into()) {
                        return Err(write::Tree::MissingBlob { oid: *oid });
                    }
                    builder.upsert(path, (*oid).into(), raw::FileMode::Blob);
                }
            }
        }

        builder
            .create_updated(self, &empty_tree)
            .map(Oid::from)
            .map_err(write::Tree::backend)
    }

    fn write_commit(&self, bytes: &[u8]) -> Result<Oid, write::Commit> {
        let odb = self.odb().map_err(write::Commit::backend)?;
        odb.write(raw::ObjectType::Commit, bytes)
            .map(Oid::from)
            .map_err(write::Commit::backend)
    }
}

/// Get or create the empty tree for use as a baseline.
fn empty_tree(repo: &raw::Repository) -> Result<git::raw::Tree<'_>, write::Tree> {
    let empty_oid = repo
        .treebuilder(None)
        .map_err(write::Tree::backend)?
        .write()
        .map_err(write::Tree::backend)?;
    repo.find_tree(empty_oid).map_err(write::Tree::backend)
}

fn find_commit<'a, P: AsRef<Path>>(
    repository: &'a git::raw::Repository,
    commit: Oid,
    path: &P,
) -> Result<git::raw::Commit<'a>, read::BlobAt> {
    match repository.find_commit(commit.into()) {
        Ok(c) => Ok(c),
        Err(e) if matches!(e.code(), git::raw::ErrorCode::NotFound) => {
            Err(read::BlobAt::CommitNotFound {
                commit,
                path: path.as_ref().to_path_buf(),
            })
        }
        Err(e) => Err(read::BlobAt::backend(e)),
    }
}

fn try_object_to_blob(obj: git::raw::Object) -> Result<Blob, read::BlobAt> {
    let blob = obj.into_blob().map_err(|obj| {
        let actual = obj
            .kind()
            .map(|k| object_kind(k).to_string())
            .unwrap_or_else(|| "unknown".to_string());
        read::BlobAt::TypeMismatch {
            oid: Oid::from(obj.id()),
            expected: ObjectKind::Blob,
            actual,
        }
    })?;
    Ok(Blob {
        oid: Oid::from(blob.id()),
        content: blob.content().to_vec(),
    })
}

fn try_entry_to_blob(
    repository: &git::raw::Repository,
    oid: Oid,
    path: &Path,
    entry: raw::TreeEntry<'_>,
) -> Result<Blob, read::BlobAt> {
    let obj = entry
        .to_object(repository)
        .map_err(|e| read::BlobAt::Object {
            commit: oid,
            path: path.to_path_buf(),
            source: Box::new(e),
        })?;
    try_object_to_blob(obj)
}

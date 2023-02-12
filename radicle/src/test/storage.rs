use std::collections::HashMap;
use std::path::{Path, PathBuf};

use git_ref_format as fmt;
use radicle_git_ext as git_ext;

use crate::crypto::{Signer, Verified};
use crate::identity::doc::{Doc, Id};

pub use crate::storage::*;

#[derive(Clone, Debug)]
pub struct MockStorage {
    pub path: PathBuf,
    pub inventory: HashMap<Id, Doc<Verified>>,
}

impl MockStorage {
    pub fn new(inventory: Vec<(Id, Doc<Verified>)>) -> Self {
        Self {
            path: PathBuf::default(),
            inventory: inventory.into_iter().collect(),
        }
    }

    pub fn empty() -> Self {
        Self {
            path: PathBuf::default(),
            inventory: HashMap::new(),
        }
    }
}

impl ReadStorage for MockStorage {
    fn path(&self) -> &Path {
        self.path.as_path()
    }

    fn contains(&self, rid: &Id) -> Result<bool, ProjectError> {
        Ok(self.inventory.contains_key(rid))
    }

    fn get(
        &self,
        _remote: &RemoteId,
        proj: Id,
    ) -> Result<Option<Doc<Verified>>, git::ProjectError> {
        Ok(self.inventory.get(&proj).cloned())
    }

    fn inventory(&self) -> Result<Inventory, Error> {
        Ok(self.inventory.keys().cloned().collect::<Vec<_>>())
    }
}

impl WriteStorage for MockStorage {
    type Repository = MockRepository;

    fn repository(&self, _proj: Id) -> Result<Self::Repository, Error> {
        Ok(MockRepository {})
    }
}

pub struct MockRepository {}

impl ReadRepository for MockRepository {
    fn id(&self) -> Id {
        todo!()
    }

    fn is_empty(&self) -> Result<bool, git2::Error> {
        Ok(true)
    }

    fn head(&self) -> Result<(fmt::Qualified, Oid), ProjectError> {
        todo!()
    }

    fn canonical_head(&self) -> Result<(fmt::Qualified, Oid), ProjectError> {
        todo!()
    }

    fn verify(&self) -> Result<(), VerifyError> {
        Ok(())
    }

    fn path(&self) -> &std::path::Path {
        todo!()
    }

    fn remote(&self, _remote: &RemoteId) -> Result<Remote<Verified>, refs::Error> {
        todo!()
    }

    fn remotes(&self) -> Result<Remotes<Verified>, refs::Error> {
        todo!()
    }

    fn commit(&self, _oid: Oid) -> Result<git2::Commit, git_ext::Error> {
        todo!()
    }

    fn revwalk(&self, _head: Oid) -> Result<git2::Revwalk, git2::Error> {
        todo!()
    }

    fn blob_at<'a>(
        &'a self,
        _oid: git_ext::Oid,
        _path: &'a std::path::Path,
    ) -> Result<git2::Blob<'a>, git_ext::Error> {
        todo!()
    }

    fn reference(
        &self,
        _remote: &RemoteId,
        _reference: &git::Qualified,
    ) -> Result<git2::Reference, git_ext::Error> {
        todo!()
    }

    fn reference_oid(
        &self,
        _remote: &RemoteId,
        _reference: &git::Qualified,
    ) -> Result<git_ext::Oid, git_ext::Error> {
        todo!()
    }

    fn references_of(&self, _remote: &RemoteId) -> Result<crate::storage::refs::Refs, Error> {
        todo!()
    }

    fn identity_doc(
        &self,
    ) -> Result<(Oid, crate::identity::Doc<crate::crypto::Unverified>), git::ProjectError> {
        todo!()
    }
}

impl WriteRepository for MockRepository {
    fn raw(&self) -> &git2::Repository {
        todo!()
    }

    fn set_head(&self) -> Result<Oid, ProjectError> {
        todo!()
    }

    fn sign_refs<G: Signer>(
        &self,
        _signer: &G,
    ) -> Result<crate::storage::refs::SignedRefs<Verified>, Error> {
        todo!()
    }
}

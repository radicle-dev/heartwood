use std::collections::HashMap;
use std::path::{Path, PathBuf};

use git_url::Url;
use radicle_git_ext as git_ext;

use crate::crypto::{Signer, Verified};
use crate::identity::project::Doc;
use crate::identity::Id;

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

    fn url(&self, _proj: &Id) -> Url {
        Url {
            scheme: git_url::Scheme::Radicle,
            host: Some("mock".to_string()),
            ..Url::default()
        }
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

    fn sign_refs<G: Signer>(
        &self,
        _repository: &Self::Repository,
        _signer: G,
    ) -> Result<crate::storage::refs::SignedRefs<Verified>, Error> {
        todo!()
    }

    fn fetch(&self, _proj_id: Id, _remote: &Url) -> Result<Vec<RefUpdate>, FetchError> {
        Ok(vec![])
    }
}

pub struct MockRepository {}

impl ReadRepository for MockRepository {
    fn is_empty(&self) -> Result<bool, git2::Error> {
        Ok(true)
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

    fn commit(&self, _oid: Oid) -> Result<Option<git2::Commit>, git2::Error> {
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
        _reference: &git::RefStr,
    ) -> Result<git2::Reference, git_ext::Error> {
        todo!()
    }

    fn reference_oid(
        &self,
        _remote: &RemoteId,
        _reference: &git::RefStr,
    ) -> Result<git_ext::Oid, git_ext::Error> {
        todo!()
    }

    fn references(&self, _remote: &RemoteId) -> Result<crate::storage::refs::Refs, Error> {
        todo!()
    }

    fn project(&self) -> Result<Doc<Verified>, Error> {
        todo!()
    }

    fn project_identity(
        &self,
    ) -> Result<(Oid, crate::identity::Doc<crate::crypto::Unverified>), git::ProjectError> {
        todo!()
    }
}

impl WriteRepository for MockRepository {
    fn fetch(&mut self, _url: &Url) -> Result<Vec<RefUpdate>, FetchError> {
        Ok(vec![])
    }

    fn raw(&self) -> &git2::Repository {
        todo!()
    }
}

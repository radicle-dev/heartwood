use git_url::Url;

use crate::crypto::{PublicKey, Verified};
use crate::git;
use crate::identity::{Id, Project};
use crate::storage::{refs, RefUpdate};
use crate::storage::{
    Error, Inventory, ReadRepository, ReadStorage, Remote, RemoteId, WriteRepository, WriteStorage,
};

#[derive(Clone, Debug)]
pub struct MockStorage {
    pub inventory: Vec<Project>,
}

impl MockStorage {
    pub fn new(inventory: Vec<Project>) -> Self {
        Self { inventory }
    }

    pub fn empty() -> Self {
        Self {
            inventory: Vec::new(),
        }
    }
}

impl ReadStorage for MockStorage {
    fn public_key(&self) -> &PublicKey {
        todo!()
    }

    fn url(&self) -> Url {
        Url {
            scheme: git_url::Scheme::Radicle,
            host: Some("mock".to_string()),
            ..Url::default()
        }
    }

    fn get(&self, proj: &Id) -> Result<Option<Project>, Error> {
        if let Some(proj) = self.inventory.iter().find(|p| p.id == *proj) {
            return Ok(Some(proj.clone()));
        }
        Ok(None)
    }

    fn inventory(&self) -> Result<Inventory, Error> {
        let inventory = self
            .inventory
            .iter()
            .map(|proj| proj.id.clone())
            .collect::<Vec<_>>();

        Ok(inventory)
    }
}

impl WriteStorage<'_> for MockStorage {
    type Repository = MockRepository;

    fn repository(&self, _proj: &Id) -> Result<Self::Repository, Error> {
        Ok(MockRepository {})
    }

    fn sign_refs(
        &self,
        _repository: &Self::Repository,
    ) -> Result<crate::storage::refs::SignedRefs<Verified>, Error> {
        todo!()
    }
}

pub struct MockRepository {}

impl ReadRepository<'_> for MockRepository {
    type Remotes = std::iter::Empty<Result<(RemoteId, Remote<Verified>), refs::Error>>;

    fn is_empty(&self) -> Result<bool, git2::Error> {
        Ok(true)
    }

    fn path(&self) -> &std::path::Path {
        todo!()
    }

    fn remote(&self, _remote: &RemoteId) -> Result<Remote<Verified>, refs::Error> {
        todo!()
    }

    fn remotes(&self) -> Result<Self::Remotes, git2::Error> {
        todo!()
    }

    fn blob_at<'a>(
        &'a self,
        _oid: radicle_git_ext::Oid,
        _path: &'a std::path::Path,
    ) -> Result<git2::Blob<'a>, radicle_git_ext::Error> {
        todo!()
    }

    fn reference(
        &self,
        _remote: &RemoteId,
        _reference: &git::RefStr,
    ) -> Result<Option<git2::Reference>, git2::Error> {
        todo!()
    }

    fn reference_oid(
        &self,
        _remote: &RemoteId,
        _reference: &git::RefStr,
    ) -> Result<Option<radicle_git_ext::Oid>, git2::Error> {
        todo!()
    }

    fn references(&self, _remote: &RemoteId) -> Result<crate::storage::refs::Refs, Error> {
        todo!()
    }
}

impl WriteRepository<'_> for MockRepository {
    fn fetch(&mut self, _url: &Url) -> Result<Vec<RefUpdate>, git2::Error> {
        Ok(vec![])
    }

    fn raw(&self) -> &git2::Repository {
        todo!()
    }
}

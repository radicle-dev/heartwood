use git_url::Url;

use crate::crypto::Verified;
use crate::identity::{ProjId, UserId};
use crate::storage::refs;
use crate::storage::{
    Error, Inventory, ReadRepository, ReadStorage, Remote, Remotes, WriteRepository, WriteStorage,
};

#[derive(Clone, Debug)]
pub struct MockStorage {
    pub inventory: Vec<(ProjId, Remotes<Verified>)>,
}

impl MockStorage {
    pub fn new(inventory: Vec<(ProjId, Remotes<Verified>)>) -> Self {
        Self { inventory }
    }

    pub fn empty() -> Self {
        Self {
            inventory: Vec::new(),
        }
    }
}

impl ReadStorage for MockStorage {
    fn user_id(&self) -> &UserId {
        todo!()
    }

    fn url(&self) -> Url {
        Url {
            scheme: git_url::Scheme::Radicle,
            host: Some("mock".to_string()),
            ..Url::default()
        }
    }

    fn get(&self, proj: &ProjId) -> Result<Option<Remotes<Verified>>, Error> {
        if let Some((_, refs)) = self.inventory.iter().find(|(id, _)| id == proj) {
            return Ok(Some(refs.clone()));
        }
        Ok(None)
    }

    fn inventory(&self) -> Result<Inventory, Error> {
        let inventory = self
            .inventory
            .iter()
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();

        Ok(inventory)
    }
}

impl WriteStorage for MockStorage {
    type Repository = MockRepository;

    fn repository(&self, _proj: &ProjId) -> Result<Self::Repository, Error> {
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

impl ReadRepository for MockRepository {
    fn path(&self) -> &std::path::Path {
        todo!()
    }

    fn remote(&self, _user: &UserId) -> Result<Remote<Verified>, refs::Error> {
        todo!()
    }

    fn remotes(&self) -> Result<Remotes<Verified>, refs::Error> {
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
        _user: &UserId,
        _reference: &str,
    ) -> Result<Option<git2::Reference>, git2::Error> {
        todo!()
    }

    fn reference_oid(
        &self,
        _user: &UserId,
        _reference: &str,
    ) -> Result<Option<radicle_git_ext::Oid>, git2::Error> {
        todo!()
    }

    fn references(&self, _user: &UserId) -> Result<crate::storage::refs::Refs, Error> {
        todo!()
    }
}

impl WriteRepository for MockRepository {
    fn fetch(&mut self, _url: &Url) -> Result<(), git2::Error> {
        Ok(())
    }

    fn raw(&self) -> &git2::Repository {
        todo!()
    }
}

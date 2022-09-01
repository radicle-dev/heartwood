use git_url::Url;

use crate::identity::{ProjId, UserId};
use crate::storage::{
    Error, Inventory, ReadRepository, ReadStorage, Remote, Remotes, Unverified, WriteRepository,
    WriteStorage,
};

#[derive(Clone, Debug)]
pub struct MockStorage {
    pub inventory: Vec<(ProjId, Remotes<Unverified>)>,
}

impl MockStorage {
    pub fn new(inventory: Vec<(ProjId, Remotes<Unverified>)>) -> Self {
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

    fn get(&self, proj: &ProjId) -> Result<Option<Remotes<Unverified>>, Error> {
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
}

pub struct MockRepository {}

impl ReadRepository for MockRepository {
    fn path(&self) -> &std::path::Path {
        todo!()
    }

    fn remote(&self, _user: &UserId) -> Result<Remote<Unverified>, Error> {
        todo!()
    }

    fn remotes(&self) -> Result<Remotes<Unverified>, Error> {
        todo!()
    }
}

impl WriteRepository for MockRepository {
    fn fetch(&mut self, _url: &Url) -> Result<(), git2::Error> {
        Ok(())
    }

    fn namespace(
        &mut self,
        _user: &crate::identity::UserId,
    ) -> Result<&mut git2::Repository, git2::Error> {
        todo!()
    }
}

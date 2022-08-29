use crate::identity::ProjId;
use crate::storage::{
    Error, Inventory, ReadStorage, Remotes, Unverified, WriteRepository, WriteStorage,
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
            .map(|(id, remotes)| (id.clone(), remotes.clone().into()))
            .collect::<Vec<_>>();

        Ok(inventory)
    }
}

impl WriteStorage for MockStorage {
    type Repository = MockRepository;

    fn repository(&self, _proj: &ProjId) -> Result<Self::Repository, Error> {
        todo!()
    }
}

pub struct MockRepository {}

impl WriteRepository for MockRepository {
    fn fetch(&mut self, _url: &str) -> Result<(), git2::Error> {
        todo!()
    }

    fn namespace(
        &mut self,
        _user: &crate::identity::UserId,
    ) -> Result<&mut git2::Repository, git2::Error> {
        todo!()
    }
}

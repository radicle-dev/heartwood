use crate::identity::ProjId;
use crate::storage::{Error, Inventory, ReadStorage, Remotes, Unverified, WriteStorage};

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
    fn repository(&mut self) -> &mut git2::Repository {
        todo!()
    }
}

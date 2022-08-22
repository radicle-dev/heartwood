use std::net;

use crate::identity::ProjId;
use crate::storage::{Error, Inventory, ReadStorage, Refs, WriteStorage};

pub struct MockStorage {
    pub inventory: Inventory,
}

impl MockStorage {
    pub fn new(inventory: Inventory) -> Self {
        Self { inventory }
    }

    pub fn empty() -> Self {
        Self {
            inventory: Vec::new(),
        }
    }
}

impl ReadStorage for MockStorage {
    fn get(&self, proj: &ProjId) -> Result<Option<Refs>, Error> {
        if let Some((_, refs)) = self.inventory.iter().find(|(id, _)| id == proj) {
            return Ok(Some(refs.clone()));
        }
        Ok(None)
    }

    fn inventory(&self) -> Result<Inventory, Error> {
        Ok(self.inventory.clone())
    }
}

impl WriteStorage for MockStorage {
    fn fetch(&mut self, _proj: &ProjId, _remote: &net::SocketAddr) -> Result<(), Error> {
        todo!()
    }
}

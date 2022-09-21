use std::sync::{Arc, Mutex};

use crate::client::handle::traits;
use crate::client::handle::Error;
use crate::identity::Id;
use crate::service;
use crate::service::FetchLookup;

#[derive(Default, Clone)]
pub struct Handle {
    pub updates: Arc<Mutex<Vec<Id>>>,
}

impl traits::Handle for Handle {
    fn fetch(&self, _id: Id) -> Result<FetchLookup, Error> {
        Ok(FetchLookup::NotFound)
    }

    fn track(&self, _id: Id) -> Result<bool, Error> {
        Ok(true)
    }

    fn untrack(&self, _id: Id) -> Result<bool, Error> {
        Ok(true)
    }

    fn updated(&self, id: Id) -> Result<(), Error> {
        self.updates.lock().unwrap().push(id);

        Ok(())
    }

    fn command(&self, _cmd: service::Command) -> Result<(), Error> {
        Ok(())
    }

    fn shutdown(self) -> Result<(), Error> {
        Ok(())
    }
}

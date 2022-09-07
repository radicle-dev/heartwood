use std::sync::{Arc, Mutex};

use crate::client::handle::traits;
use crate::client::handle::Error;
use crate::identity::ProjId;
use crate::protocol;
use crate::protocol::FetchLookup;

#[derive(Default, Clone)]
pub struct Handle {
    pub updates: Arc<Mutex<Vec<ProjId>>>,
}

impl traits::Handle for Handle {
    fn fetch(&self, _id: ProjId) -> Result<FetchLookup, Error> {
        Ok(FetchLookup::NotFound)
    }

    fn track(&self, _id: ProjId) -> Result<bool, Error> {
        Ok(true)
    }

    fn untrack(&self, _id: ProjId) -> Result<bool, Error> {
        Ok(true)
    }

    fn updated(&self, id: ProjId) -> Result<(), Error> {
        self.updates.lock().unwrap().push(id);

        Ok(())
    }

    fn command(&self, _cmd: protocol::Command) -> Result<(), Error> {
        Ok(())
    }

    fn shutdown(self) -> Result<(), Error> {
        Ok(())
    }
}

use std::sync::{Arc, Mutex};

use crate::client::handle;
use crate::client::handle::traits;
use crate::{client, identity, protocol};

#[derive(Default, Clone)]
pub struct Handle {
    pub updates: Arc<Mutex<Vec<identity::ProjId>>>,
}

impl traits::Handle for Handle {
    fn updated(&self, id: identity::ProjId) -> Result<(), handle::Error> {
        self.updates.lock().unwrap().push(id);

        Ok(())
    }

    fn command(&self, _cmd: protocol::Command) -> Result<(), handle::Error> {
        Ok(())
    }

    fn shutdown(self) -> Result<(), client::handle::Error> {
        Ok(())
    }
}

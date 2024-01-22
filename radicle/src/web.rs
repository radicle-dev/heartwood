use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::prelude::Id;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub pinned: Pinned,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pinned {
    pub repositories: HashSet<Id>,
}

impl Pinned {
    pub fn has_rid(&self, id: &Id) -> bool {
        self.repositories.contains(id)
    }
}

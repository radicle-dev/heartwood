use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::prelude::RepoId;

/// Web configuration.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    /// Pinned content.
    pub pinned: Pinned,
}

/// Pinned content. This can be used to pin certain content when
/// listing, e.g. pin repositories on a web client.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pinned {
    /// Pinned repositories.
    pub repositories: HashSet<RepoId>,
}

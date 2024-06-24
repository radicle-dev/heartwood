use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::prelude::RepoId;

/// Web configuration.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    /// Pinned content.
    pub pinned: Pinned,
    /// URL pointing to an image for the node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
    /// Node name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Node description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Pinned content. This can be used to pin certain content when
/// listing, e.g. pin repositories on a web client.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pinned {
    /// Pinned repositories.
    pub repositories: HashSet<RepoId>,
}

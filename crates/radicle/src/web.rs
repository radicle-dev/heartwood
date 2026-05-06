use indexmap::IndexSet;
use serde::{Deserialize, Serialize};

use crate::prelude::RepoId;

/// Web configuration.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(
    feature = "schemars",
    derive(schemars::JsonSchema),
    schemars(rename = "WebConfig")
)]
pub struct Config {
    /// Pinned content.
    #[serde(default, skip_serializing_if = "crate::serde_ext::is_default")]
    pub pinned: Pinned,
    /// URL pointing to an image used in the header of a node page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "schemars", schemars(url))]
    pub banner_url: Option<String>,
    /// URL pointing to an image used as the node avatar.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "schemars", schemars(url))]
    pub avatar_url: Option<String>,
    /// Node description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Pinned content. This can be used to pin certain content when
/// listing, e.g. pin repositories on a web client.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Pinned {
    /// Pinned repositories.
    #[serde(default, skip_serializing_if = "crate::serde_ext::is_default")]
    #[cfg_attr(
        feature = "schemars",
        schemars(with = "std::collections::HashSet<RepoId>")
    )]
    pub repositories: IndexSet<RepoId>,
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn description_only() {
        let value = serde_json::json!({
            "description": "A node description",
        });
        let _: Config = serde_json::from_value(value).unwrap();
    }

    #[test]
    fn pinned_empty() {
        let value = serde_json::json!({
            "pinned": {},
        });
        let _: Config = serde_json::from_value(value).unwrap();
    }
}

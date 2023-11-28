use std::str::FromStr;

use thiserror::Error;

use crate::prelude::Id;
use crate::{cob, git};

#[derive(Debug, Error)]
pub enum ExplorerError {
    #[error("invalid explorer URL {0:?}: unknown protocol")]
    UnknownProtocol(String),
    #[error("invalid explorer URL {0:?}: missing `$host` component")]
    MissingHost(String),
    #[error("invalid explorer URL {0:?}: missing `$rid` component")]
    MissingRid(String),
    #[error("invalid explorer URL {0:?}: missing `$path` component")]
    MissingPath(String),
}

/// A resource such as a branch, patch or commit.
#[derive(Debug, Hash, PartialEq, Eq)]
pub enum ExplorerResource {
    /// Git tree object. Used for the repository root.
    Tree { oid: git::Oid },
    /// A Patch COB.
    Patch { id: cob::ObjectId },
}

impl std::fmt::Display for ExplorerResource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Tree { oid } => {
                write!(f, "/tree/{oid}")
            }
            Self::Patch { id } => {
                write!(f, "/patches/{id}")
            }
        }
    }
}

/// A URL to a specific repository or resource within a repository.
#[derive(Debug, PartialEq, Eq)]
pub struct ExplorerUrl {
    /// URL template.
    pub template: Explorer,
    /// Host serving the repository.
    pub host: String,
    /// Repository.
    pub rid: Id,
    /// Resource under the repository.
    pub resource: Option<ExplorerResource>,
}

impl ExplorerUrl {
    /// Set a resource on for this URL.
    pub fn resource(mut self, resource: ExplorerResource) -> Self {
        self.resource = Some(resource);
        self
    }
}

impl std::fmt::Display for ExplorerUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            self.template
                .0
                .replace("$host", &self.host)
                .replace("$rid", self.rid.urn().as_str())
                .replace(
                    "$path",
                    self.resource
                        .as_ref()
                        .map(|r| r.to_string())
                        .as_deref()
                        .unwrap_or(""),
                )
                .as_str(),
        )
    }
}

/// A public explorer, eg. `https://app.radicle.xyz`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(transparent)]
pub struct Explorer(String);

impl Default for Explorer {
    fn default() -> Self {
        Self(String::from(
            "https://app.radicle.xyz/nodes/$host/$rid$path",
        ))
    }
}

impl Explorer {
    /// Get the explorer URL, filling in the host and RID.
    pub fn url(&self, host: impl ToString, rid: Id) -> ExplorerUrl {
        ExplorerUrl {
            template: self.clone(),
            host: host.to_string(),
            rid,
            resource: None,
        }
    }
}

impl FromStr for Explorer {
    type Err = ExplorerError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let url = s.to_owned();

        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Err(ExplorerError::UnknownProtocol(url));
        }
        if !url.contains("$host") {
            return Err(ExplorerError::MissingHost(url));
        }
        if !url.contains("$rid") {
            return Err(ExplorerError::MissingRid(url));
        }
        if !url.contains("$path") {
            return Err(ExplorerError::MissingPath(url));
        }
        Ok(Explorer(url))
    }
}

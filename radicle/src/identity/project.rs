use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::crypto;
use crate::identity::doc;
use crate::identity::doc::Payload;
use crate::storage::BranchName;

pub use crypto::PublicKey;

/// A project-related error.
#[derive(Debug, Error)]
pub enum ProjectError {
    #[error("invalid name: {0}")]
    Name(&'static str),
    #[error("invalid description: {0}")]
    Description(&'static str),
    #[error("invalid default branch: {0}")]
    DefaultBranch(&'static str),
}

/// A "project" payload in an identity document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    /// Project name.
    pub name: String,
    /// Project description.
    pub description: String,
    /// Project default branch.
    pub default_branch: BranchName,
}

impl Project {
    /// Validate the project data.
    pub fn validate(&self) -> Result<(), ProjectError> {
        if self.name.is_empty() {
            return Err(ProjectError::Name("name cannot be empty"));
        }
        if self.name.len() > doc::MAX_STRING_LENGTH {
            return Err(ProjectError::Name("name cannot exceed 255 bytes"));
        }
        if self.description.len() > doc::MAX_STRING_LENGTH {
            return Err(ProjectError::Description(
                "description cannot exceed 255 bytes",
            ));
        }
        if self.default_branch.is_empty() {
            return Err(ProjectError::DefaultBranch(
                "default branch cannot be empty",
            ));
        }
        if self.default_branch.len() > doc::MAX_STRING_LENGTH {
            return Err(ProjectError::DefaultBranch(
                "default branch cannot exceed 255 bytes",
            ));
        }
        Ok(())
    }
}

impl From<Project> for Payload {
    fn from(proj: Project) -> Self {
        let value = serde_json::to_value(proj)
            .expect("Payload::from: could not convert project into value");

        Self::from(value)
    }
}

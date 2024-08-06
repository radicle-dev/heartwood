use std::{fmt, str::FromStr};

use serde::{
    de::{self, MapAccess, Visitor},
    Deserialize, Serialize,
};
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

/// A valid project name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ProjectName(String);

impl From<ProjectName> for String {
    fn from(value: ProjectName) -> Self {
        value.0
    }
}

impl ProjectName {
    /// List of allowed special characters.
    pub const ALLOWED_CHARS: &'static [char] = &['-', '_', '.'];

    /// Return a string reference to the name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<&str> for ProjectName {
    type Error = ProjectError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        ProjectName::from_str(s)
    }
}

impl TryFrom<String> for ProjectName {
    type Error = ProjectError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        if s.is_empty() {
            return Err(ProjectError::Name("name cannot be empty"));
        } else if s.len() > doc::MAX_STRING_LENGTH {
            return Err(ProjectError::Name("name cannot exceed 255 bytes"));
        }
        // Nb. We avoid characters that need to be quoted by shells, such as `$`,
        // `!` etc., since repository names are used for naming folders during clone.
        if !s
            .chars()
            .all(|c| c.is_alphanumeric() || Self::ALLOWED_CHARS.contains(&c))
        {
            return Err(ProjectError::Name(
                "invalid repository name, only alphanumeric characters, '-', '_' and '.' are allowed",
            ));
        }
        Ok(Self(s))
    }
}

impl FromStr for ProjectName {
    type Err = ProjectError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_from(s.to_owned())
    }
}

/// A "project" payload in an identity document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    /// Project name.
    name: ProjectName,
    /// Project description.
    description: String,
    /// Project default branch.
    default_branch: BranchName,
}

impl<'de> Deserialize<'de> for Project {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "camelCase")]
        enum Field {
            Name,
            Description,
            DefaultBranch,
            /// A catch-all variant to allow for unknown fields
            Unknown(#[allow(dead_code)] String),
        }

        struct ProjectVisitor;

        impl<'de> Visitor<'de> for ProjectVisitor {
            type Value = Project;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("xyz.radicle.project")
            }

            fn visit_map<V>(self, mut map: V) -> Result<Self::Value, V::Error>
            where
                V: MapAccess<'de>,
            {
                let mut name = None;
                let mut description = None;
                let mut default_branch = None;

                while let Some(key) = map.next_key()? {
                    match key {
                        Field::Name => {
                            if name.is_some() {
                                return Err(de::Error::duplicate_field("name"));
                            }
                            name = Some(map.next_value()?);
                        }
                        Field::Description => {
                            if description.is_some() {
                                return Err(de::Error::duplicate_field("description"));
                            }
                            description = Some(map.next_value()?);
                        }
                        Field::DefaultBranch => {
                            if default_branch.is_some() {
                                return Err(de::Error::duplicate_field("defaultBranch"));
                            }
                            default_branch = Some(map.next_value()?);
                        }
                        Field::Unknown(_) => continue,
                    }
                }
                let name = name.ok_or_else(|| de::Error::missing_field("name"))?;
                let description =
                    description.ok_or_else(|| de::Error::missing_field("description"))?;
                let default_branch =
                    default_branch.ok_or_else(|| de::Error::missing_field("defaultBranch"))?;
                Project::new(name, description, default_branch).map_err(|errs| {
                    de::Error::custom(
                        errs.into_iter()
                            .map(|err| err.to_string())
                            .collect::<Vec<_>>()
                            .join(", "),
                    )
                })
            }
        }
        const FIELDS: &[&str] = &["name", "descrption", "defaultBranch"];
        deserializer.deserialize_struct("Project", FIELDS, ProjectVisitor)
    }
}

impl Project {
    /// Create a new `Project` payload with the given values.
    ///
    /// These values are subject to validation and any errors are returned in a vector.
    ///
    /// # Validation Rules
    ///
    ///   * `name`'s length must not be empty and must not exceed 255.
    ///   * `description`'s length must not exceed 255.
    ///   * `default_branch`'s length must not be empty and must not exceed 255.
    pub fn new(
        name: ProjectName,
        description: String,
        default_branch: BranchName,
    ) -> Result<Self, Vec<ProjectError>> {
        let mut errs = Vec::new();

        if description.len() > doc::MAX_STRING_LENGTH {
            errs.push(ProjectError::Description(
                "description cannot exceed 255 bytes",
            ));
        }

        if default_branch.is_empty() {
            errs.push(ProjectError::DefaultBranch(
                "default branch cannot be empty",
            ))
        } else if default_branch.len() > doc::MAX_STRING_LENGTH {
            errs.push(ProjectError::DefaultBranch(
                "default branch cannot exceed 255 bytes",
            ))
        }

        if errs.is_empty() {
            Ok(Self {
                name,
                description,
                default_branch,
            })
        } else {
            Err(errs)
        }
    }

    /// Update the `Project` payload with new values, if provided.
    ///
    /// When any of the values are set to `None` then the original
    /// value will be used, and so the value will pass validation.
    ///
    /// Otherwise, the new value is used and will be subject to the
    /// original validation rules (see [`Project::new`]).
    pub fn update(
        self,
        name: impl Into<Option<ProjectName>>,
        description: impl Into<Option<String>>,
        default_branch: impl Into<Option<BranchName>>,
    ) -> Result<Self, Vec<ProjectError>> {
        let name = name.into().unwrap_or(self.name);
        let description = description.into().unwrap_or(self.description);
        let default_branch = default_branch.into().unwrap_or(self.default_branch);
        Self::new(name, description, default_branch)
    }

    #[inline]
    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    #[inline]
    pub fn description(&self) -> &str {
        &self.description
    }

    #[inline]
    pub fn default_branch(&self) -> &BranchName {
        &self.default_branch
    }
}

impl From<Project> for Payload {
    fn from(proj: Project) -> Self {
        let value = serde_json::to_value(proj)
            .expect("Payload::from: could not convert project into value");

        Self::from(value)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod test {
    use super::*;
    use crate::assert_matches;

    #[test]
    fn test_project_name() {
        assert_matches!(serde_json::from_str::<ProjectName>("\"\""), Err(_));
        assert_matches!(
            serde_json::from_str::<ProjectName>("\"invalid name\""),
            Err(_)
        );
        assert_matches!(
            serde_json::from_str::<ProjectName>("\"invalid%name\""),
            Err(_)
        );
        assert_eq!(
            serde_json::from_str::<ProjectName>("\"valid-name\"").unwrap(),
            ProjectName("valid-name".to_owned())
        );
    }
}

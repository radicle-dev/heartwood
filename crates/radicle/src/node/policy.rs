pub mod config;
pub mod store;

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::prelude::RepoId;

pub use super::{Alias, NodeId};

/// Repository seeding policy.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeedPolicy {
    pub rid: RepoId,
    pub policy: SeedingPolicy,
}

impl std::ops::Deref for SeedPolicy {
    type Target = SeedingPolicy;

    fn deref(&self) -> &Self::Target {
        &self.policy
    }
}

/// Node following policy.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FollowPolicy {
    pub nid: NodeId,
    pub alias: Option<Alias>,
    pub policy: Policy,
}

/// Seeding policy of a node or repo.
#[derive(Debug, Copy, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "policy")]
pub enum SeedingPolicy {
    /// Allow seeding.
    Allow {
        /// Seeding scope.
        #[serde(default)]
        scope: Scope,
    },
    /// Block seeding.
    #[default]
    Block,
}

impl SeedingPolicy {
    /// Is this an "allow" policy.
    pub fn is_allow(&self) -> bool {
        matches!(self, Self::Allow { .. })
    }

    /// Is this a "block" policy.
    pub fn is_block(&self) -> bool {
        !self.is_allow()
    }

    /// Scope, if any.
    pub fn scope(&self) -> Option<Scope> {
        match self {
            Self::Allow { scope } => Some(*scope),
            Self::Block => None,
        }
    }
}

impl From<SeedingPolicy> for Policy {
    fn from(p: SeedingPolicy) -> Self {
        match p {
            SeedingPolicy::Block => Policy::Block,
            SeedingPolicy::Allow { .. } => Policy::Allow,
        }
    }
}

impl std::fmt::Display for SeedingPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} ({})",
            Policy::from(*self),
            self.scope().unwrap_or_default()
        )
    }
}

/// Resource policy.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Policy {
    /// The resource is allowed.
    Allow,
    /// The resource is blocked.
    #[default]
    Block,
}

impl fmt::Display for Policy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Allow => write!(f, "allow"),
            Self::Block => write!(f, "block"),
        }
    }
}

impl FromStr for Policy {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "allow" => Ok(Self::Allow),
            "block" => Ok(Self::Block),
            _ => Err(s.to_owned()),
        }
    }
}

impl sqlite::BindableWithIndex for Policy {
    fn bind<I: sqlite::ParameterIndex>(
        self,
        stmt: &mut sqlite::Statement<'_>,
        i: I,
    ) -> sqlite::Result<()> {
        match self {
            Self::Allow => "allow",
            Self::Block => "block",
        }
        .bind(stmt, i)
    }
}

impl TryFrom<&sqlite::Value> for Policy {
    type Error = sqlite::Error;

    fn try_from(value: &sqlite::Value) -> Result<Self, Self::Error> {
        let message = Some("sql: invalid policy value".to_owned());

        match value {
            sqlite::Value::String(s) if s == "allow" => Ok(Policy::Allow),
            sqlite::Value::String(s) if s == "block" => Ok(Policy::Block),
            sqlite::Value::String(s) => Err(sqlite::Error {
                code: None,
                message: Some(format!("sql: invalid policy '{s}'")),
            }),
            _ => Err(sqlite::Error {
                code: None,
                message,
            }),
        }
    }
}

/// Follow scope of a seeded repository.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum Scope {
    /// Seed remotes that are explicitly followed.
    Followed,
    /// Seed all remotes.
    #[default]
    All,
}

impl fmt::Display for Scope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Scope::Followed => f.write_str("followed"),
            Scope::All => f.write_str("all"),
        }
    }
}

#[derive(Debug, Error)]
#[error("invalid seeding scope: {0:?}")]
pub struct ParseScopeError(String);

impl FromStr for Scope {
    type Err = ParseScopeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "followed" => Ok(Self::Followed),
            "all" => Ok(Self::All),
            _ => Err(ParseScopeError(s.to_string())),
        }
    }
}

impl sqlite::BindableWithIndex for Scope {
    fn bind<I: sqlite::ParameterIndex>(
        self,
        stmt: &mut sqlite::Statement<'_>,
        i: I,
    ) -> sqlite::Result<()> {
        let s = match self {
            Self::Followed => "followed",
            Self::All => "all",
        };
        s.bind(stmt, i)
    }
}

impl TryFrom<&sqlite::Value> for Scope {
    type Error = sqlite::Error;

    fn try_from(value: &sqlite::Value) -> Result<Self, Self::Error> {
        let message = Some("invalid remote scope".to_owned());

        match value {
            sqlite::Value::String(scope) => Scope::from_str(scope).map_err(|_| sqlite::Error {
                code: None,
                message,
            }),
            _ => Err(sqlite::Error {
                code: None,
                message,
            }),
        }
    }
}

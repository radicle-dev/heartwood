pub mod store;

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::prelude::Id;

use super::NodeId;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Repo {
    pub id: Id,
    pub scope: Scope,
    pub policy: Policy,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    pub alias: Option<Alias>,
    pub policy: Policy,
}

/// Node alias.
pub type Alias = String;

/// Tracking policy.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Policy {
    /// The resource is tracked.
    Track,
    /// The resource is blocked.
    #[default]
    Block,
}

impl fmt::Display for Policy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Track => write!(f, "track"),
            Self::Block => write!(f, "block"),
        }
    }
}

impl FromStr for Policy {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "track" => Ok(Self::Track),
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
            Self::Track => "track",
            Self::Block => "block",
        }
        .bind(stmt, i)
    }
}

impl TryFrom<&sqlite::Value> for Policy {
    type Error = sqlite::Error;

    fn try_from(value: &sqlite::Value) -> Result<Self, Self::Error> {
        let message = Some("sql: invalid policy".to_owned());

        match value {
            sqlite::Value::String(s) if s == "track" => Ok(Policy::Track),
            sqlite::Value::String(s) if s == "block" => Ok(Policy::Block),
            _ => Err(sqlite::Error {
                code: None,
                message,
            }),
        }
    }
}

/// Tracking scope of a repository tracking policy.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Scope {
    /// Track remotes of nodes that are already tracked.
    #[default]
    Trusted,
    /// Track all remotes.
    All,
}

impl fmt::Display for Scope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Scope::Trusted => f.write_str("trusted"),
            Scope::All => f.write_str("all"),
        }
    }
}

#[derive(Debug, Error)]
#[error("invalid tracking scope: {0:?}")]
pub struct ParseScopeError(String);

impl FromStr for Scope {
    type Err = ParseScopeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "trusted" => Ok(Self::Trusted),
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
            Self::Trusted => "trusted",
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

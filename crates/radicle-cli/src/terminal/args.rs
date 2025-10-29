use clap::builder::TypedValueParser;
use thiserror::Error;

use radicle::node::policy::Scope;
use radicle::prelude::{Did, NodeId, RepoId};

#[derive(thiserror::Error, Debug)]
pub(crate) enum Error {
    /// An error with a hint.
    #[error("{err}")]
    WithHint {
        err: anyhow::Error,
        hint: &'static str,
    },
}

/// Targets used in the `block` and `unblock` commands
#[derive(Clone, Debug)]
pub(crate) enum BlockTarget {
    Node(NodeId),
    Repo(RepoId),
}

#[derive(Debug, Error)]
#[error("invalid repository or node specified (RID parsing failed with: '{repo}', NID parsing failed with: '{node}'))")]
pub(crate) struct BlockTargetParseError {
    repo: radicle::identity::IdError,
    node: radicle::crypto::PublicKeyError,
}

impl std::str::FromStr for BlockTarget {
    type Err = BlockTargetParseError;

    fn from_str(val: &str) -> Result<Self, Self::Err> {
        val.parse::<RepoId>()
            .map(BlockTarget::Repo)
            .or_else(|repo| {
                val.parse::<NodeId>()
                    .map(BlockTarget::Node)
                    .map_err(|node| BlockTargetParseError { repo, node })
            })
    }
}

impl std::fmt::Display for BlockTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Node(nid) => nid.fmt(f),
            Self::Repo(rid) => rid.fmt(f),
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("invalid Node ID specified (Node ID parsing failed with: '{nid}', DID parsing failed with: '{did}'))")]
pub(crate) struct NodeIdParseError {
    did: radicle::identity::did::DidError,
    nid: radicle::crypto::PublicKeyError,
}

pub(crate) fn parse_nid(value: &str) -> Result<NodeId, NodeIdParseError> {
    value.parse::<Did>().map(NodeId::from).or_else(|did| {
        value
            .parse::<NodeId>()
            .map_err(|nid| NodeIdParseError { nid, did })
    })
}

#[derive(Clone, Debug)]
pub(crate) struct ScopeParser;

impl TypedValueParser for ScopeParser {
    type Value = Scope;

    fn parse_ref(
        &self,
        cmd: &clap::Command,
        arg: Option<&clap::Arg>,
        value: &std::ffi::OsStr,
    ) -> Result<Self::Value, clap::Error> {
        <Scope as std::str::FromStr>::from_str.parse_ref(cmd, arg, value)
    }

    fn possible_values(
        &self,
    ) -> Option<Box<dyn Iterator<Item = clap::builder::PossibleValue> + '_>> {
        use clap::builder::PossibleValue;
        Some(Box::new(
            [PossibleValue::new("all"), PossibleValue::new("followed")].into_iter(),
        ))
    }
}

#[cfg(test)]
mod test {
    use std::str::FromStr;

    use super::BlockTarget;
    use super::BlockTargetParseError;

    #[test]
    fn should_parse_nid() {
        let target = BlockTarget::from_str("z6MkiswaKJ85vafhffCGBu2gdBsYoDAyHVBWRxL3j297fwS9");
        assert!(target.is_ok())
    }

    #[test]
    fn should_parse_rid() {
        let target = BlockTarget::from_str("rad:z3Tr6bC7ctEg2EHmLvknUr29mEDLH");
        assert!(target.is_ok())
    }

    #[test]
    fn should_not_parse() {
        let err = BlockTarget::from_str("bee").unwrap_err();
        assert!(matches!(err, BlockTargetParseError { .. }));
    }
}

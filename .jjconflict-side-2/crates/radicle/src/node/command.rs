//! Commands sent to the node via the control socket, and auxiliary types, as
//! well as their results (responses on the socket).

// There are derives on an enum with a deprecated variant
// in this module, see [`Command::AnnounceRefs`] and also
// <https://github.com/rust-lang/rust/issues/92313>.
#![allow(deprecated)]

use std::collections::HashSet;
use std::io;
use std::time;

use serde::{Deserialize, Serialize};
use serde_json as json;

use crate::crypto::PublicKey;
use crate::identity::RepoId;
use crate::storage::refs;

use super::NodeId;
use super::events::Event;

/// Default timeout when waiting for the node to respond with data.
pub const DEFAULT_TIMEOUT: time::Duration = time::Duration::from_secs(30);

/// Command name.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "command")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum Command {
    /// Announce repository references for given repository to peers.
    #[serde(rename_all = "camelCase")]
    #[deprecated(note = "use `AnnounceRefsFor` instead")]
    AnnounceRefs { rid: RepoId },

    /// Announce repository references for given repository
    /// and namespaces to peers.
    #[serde(rename_all = "camelCase")]
    AnnounceRefsFor {
        /// The ID of the repository for which references should be announced.
        rid: RepoId,

        /// The namespaces for which references should be announced.
        namespaces: HashSet<PublicKey>,
    },

    /// Announce local repositories to peers.
    #[serde(rename_all = "camelCase")]
    AnnounceInventory,

    /// Update node's inventory.
    AddInventory { rid: RepoId },

    /// Get the current node configuration.
    Config,

    /// Get the node's listen addresses.
    ListenAddrs,

    /// Connect to node with the given address.
    #[serde(rename_all = "camelCase")]
    Connect {
        addr: super::config::ConnectAddress,
        opts: ConnectOptions,
    },

    /// Disconnect from a node.
    #[serde(rename_all = "camelCase")]
    Disconnect { nid: NodeId },

    /// Look up seeds for the given repository in the routing table.
    #[serde(rename_all = "camelCase")]
    #[deprecated(note = "use `SeedsFor` instead")]
    Seeds { rid: RepoId },

    /// Look up seeds for the given repository in the routing table and
    /// report sync status for the given namespaces.
    #[serde(rename_all = "camelCase")]
    SeedsFor {
        /// The ID of the repository for which seeds should be looked up
        /// in the routing table.
        rid: RepoId,

        /// The namespaces for which references should be announced.
        namespaces: HashSet<PublicKey>,
    },

    /// Get the current peer sessions.
    Sessions,

    /// Get a specific peer session.
    Session { nid: NodeId },

    /// Fetch the given repository from the network.
    #[serde(rename_all = "camelCase")]
    Fetch {
        rid: RepoId,
        nid: NodeId,
        timeout: time::Duration,
        signed_references_minimum_feature_level: Option<refs::FeatureLevel>,
    },

    /// Seed the given repository.
    #[serde(rename_all = "camelCase")]
    Seed {
        rid: RepoId,
        scope: super::policy::Scope,
    },

    /// Unseed the given repository.
    #[serde(rename_all = "camelCase")]
    Unseed { rid: RepoId },

    /// Follow the given node.
    #[serde(rename_all = "camelCase")]
    Follow {
        nid: NodeId,
        alias: Option<super::Alias>,
    },

    /// Unfollow the given node.
    #[serde(rename_all = "camelCase")]
    Unfollow { nid: NodeId },

    /// Block the given node.
    #[serde(rename_all = "camelCase")]
    Block { nid: NodeId },

    /// Get the node's status.
    Status,

    /// Get node debug information.
    Debug,

    /// Get the node's NID.
    NodeId,

    /// Shutdown the node.
    Shutdown,

    /// Subscribe to events.
    Subscribe,
}

impl Command {
    /// Write this command to a stream, including a terminating LF character.
    pub fn to_writer(&self, mut w: impl io::Write) -> io::Result<()> {
        json::to_writer(&mut w, self).map_err(|_| io::ErrorKind::InvalidInput)?;
        w.write_all(b"\n")
    }
}

/// Options passed to the "connect" node command.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct ConnectOptions {
    /// Establish a persistent connection.
    pub persistent: bool,
    /// How long to wait for the connection to be established.
    pub timeout: time::Duration,
}

impl Default for ConnectOptions {
    fn default() -> Self {
        Self {
            persistent: false,
            timeout: DEFAULT_TIMEOUT,
        }
    }
}

/// Result of a command, on the node control socket.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CommandResult<T> {
    /// Response on node socket indicating that a command was carried out successfully.
    Okay(T),
    /// Response on node socket indicating that an error occurred.
    Error {
        /// The reason for the error.
        #[serde(rename = "error")]
        reason: String,
    },
}

impl<T, E> From<Result<T, E>> for CommandResult<T>
where
    E: std::error::Error,
{
    fn from(result: Result<T, E>) -> Self {
        match result {
            Ok(t) => Self::Okay(t),
            Err(e) => Self::Error {
                reason: e.to_string(),
            },
        }
    }
}

impl From<Event> for CommandResult<Event> {
    fn from(event: Event) -> Self {
        Self::Okay(event)
    }
}

/// A success response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Success {
    /// Whether something was updated.
    #[serde(default, skip_serializing_if = "crate::serde_ext::is_default")]
    pub(super) updated: bool,
}

impl CommandResult<Success> {
    /// Create an "updated" response.
    pub fn updated(updated: bool) -> Self {
        Self::Okay(Success { updated })
    }

    /// Create an "ok" response.
    pub fn ok() -> Self {
        Self::Okay(Success { updated: false })
    }
}

impl CommandResult<()> {
    /// Create an error result.
    pub fn error(err: impl std::error::Error) -> Self {
        Self::Error {
            reason: err.to_string(),
        }
    }
}

impl<T: Serialize> CommandResult<T> {
    /// Write this command result to a stream, including a terminating LF character.
    pub fn to_writer(&self, mut w: impl io::Write) -> io::Result<()> {
        json::to_writer(&mut w, self).map_err(|_| io::ErrorKind::InvalidInput)?;
        w.write_all(b"\n")
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod test {
    use super::*;
    use std::collections::VecDeque;

    use localtime::LocalTime;

    use crate::assert_matches;
    use crate::node::{Seeds, State};

    #[test]
    fn command_result() {
        #[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
        struct Test {
            value: u32,
        }

        assert_eq!(json::to_string(&CommandResult::Okay(true)).unwrap(), "true");
        assert_eq!(
            json::to_string(&CommandResult::Okay(Test { value: 42 })).unwrap(),
            "{\"value\":42}"
        );
        assert_eq!(
            json::from_str::<CommandResult<Test>>("{\"value\":42}").unwrap(),
            CommandResult::Okay(Test { value: 42 })
        );
        assert_eq!(json::to_string(&CommandResult::ok()).unwrap(), "{}");
        assert_eq!(
            json::to_string(&CommandResult::updated(true)).unwrap(),
            "{\"updated\":true}"
        );
        assert_eq!(
            json::to_string(&CommandResult::error(io::Error::from(
                io::ErrorKind::NotFound
            )))
            .unwrap(),
            "{\"error\":\"entity not found\"}"
        );

        json::from_str::<CommandResult<State>>(
            &serde_json::to_string(&CommandResult::Okay(State::Connected {
                since: LocalTime::now(),
                ping: Default::default(),
                latencies: VecDeque::default(),
                stable: false,
            }))
            .unwrap(),
        )
        .unwrap();

        assert_matches!(
            json::from_str::<CommandResult<State>>(
                r#"{"connected":{"since":1699636852107,"fetching":[]}}"#
            ),
            Ok(CommandResult::Okay(_))
        );
        assert_matches!(
            json::from_str::<CommandResult<Seeds>>(
                r#"[{"nid":"z6MksmpU5b1dS7oaqF2bHXhQi1DWy2hB7Mh9CuN7y1DN6QSz","addrs":[{"addr":"seed.radicle.example.com:8776","source":"peer","lastSuccess":1699983994234,"lastAttempt":1699983994000,"banned":false}],"state":{"connected":{"since":1699983994}}}]"#
            ),
            Ok(CommandResult::Okay(_))
        );
    }
}

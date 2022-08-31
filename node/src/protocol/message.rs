use std::net;

use git_url::Url;
use serde::{Deserialize, Serialize};

use crate::crypto;
use crate::identity::ProjId;
use crate::protocol::{Context, NodeId, Timestamp, NETWORK_MAGIC, PROTOCOL_VERSION};
use crate::storage;

/// Message envelope. All messages sent over the network are wrapped in this type.
#[derive(Debug, Serialize, Deserialize)]
pub struct Envelope {
    /// Network magic constant. Used to differentiate networks.
    pub magic: u32,
    /// The message payload.
    pub msg: Message,
}

/// Advertized node feature. Signals what services the node supports.
pub type NodeFeatures = [u8; 32];

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
// TODO: We should check the length and charset when deserializing.
pub struct Hostname(String);

/// Peer public protocol address.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum Address {
    /// Tor V3 onion address.
    Onion {
        key: crypto::PublicKey,
        port: u16,
        checksum: u16,
        version: u8,
    },
    Ip {
        ip: net::IpAddr,
        port: u16,
    },
    Hostname {
        host: Hostname,
        port: u16,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct NodeAnnouncement {
    /// Node identifier.
    id: NodeId,
    /// Advertized features.
    features: NodeFeatures,
    /// Monotonic timestamp.
    timestamp: Timestamp,
    /// Non-unique alias. Must be valid UTF-8.
    alias: [u8; 32],
    /// Announced addresses.
    addresses: Vec<Address>,
}

/// Message payload.
/// These are the messages peers send to each other.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum Message {
    /// Say hello to a peer. This is the first message sent to a peer after connection.
    Hello {
        // TODO: This is currently untrusted.
        id: NodeId,
        timestamp: Timestamp,
        version: u32,
        addrs: Vec<Address>,
        git: Url,
    },
    Node {
        /// Signature over the announcement, by the node being announced.
        signature: crypto::Signature,
        /// Unsigned node announcement.
        announcement: NodeAnnouncement,
    },
    /// Get a peer's inventory.
    GetInventory { ids: Vec<ProjId> },
    /// Send our inventory to a peer. Sent in response to [`Message::GetInventory`].
    /// Nb. This should be the whole inventory, not a partial update.
    Inventory {
        inv: Vec<ProjId>,
        timestamp: Timestamp,
        /// Original peer this inventory came from. We don't set this when we
        /// are the originator, only when relaying.
        origin: Option<NodeId>,
    },
}

impl From<Message> for Envelope {
    fn from(msg: Message) -> Self {
        Self {
            magic: NETWORK_MAGIC,
            msg,
        }
    }
}

impl Message {
    pub fn hello(id: NodeId, timestamp: Timestamp, addrs: Vec<Address>, git: Url) -> Self {
        Self::Hello {
            id,
            timestamp,
            version: PROTOCOL_VERSION,
            addrs,
            git,
        }
    }

    pub fn inventory<S, T, G>(ctx: &mut Context<S, T, G>) -> Result<Self, storage::Error>
    where
        T: storage::ReadStorage,
    {
        let timestamp = ctx.timestamp();
        let inv = ctx.storage.inventory()?;

        Ok(Self::Inventory {
            timestamp,
            inv,
            origin: None,
        })
    }

    pub fn get_inventory(ids: impl Into<Vec<ProjId>>) -> Self {
        Self::GetInventory { ids: ids.into() }
    }
}

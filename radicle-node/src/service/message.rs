use std::{fmt, io, mem};

use crate::crypto;
use crate::git;
use crate::identity::Id;
use crate::node;
use crate::node::Address;
use crate::prelude::BoundedVec;
use crate::service::filter::Filter;
use crate::service::{NodeId, Timestamp};
use crate::wire;

/// Maximum number of addresses which can be announced to other nodes.
pub const ADDRESS_LIMIT: usize = 16;
/// Maximum number of project git references.
pub const REF_LIMIT: usize = 235;
/// Maximum number of inventory which can be announced to other nodes.
pub const INVENTORY_LIMIT: usize = 2973;

#[derive(Debug, Clone, PartialEq, Eq)]
// TODO: We should check the length and charset when deserializing.
pub struct Hostname(String);

impl fmt::Display for Hostname {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subscribe {
    /// Subscribe to events matching this filter.
    pub filter: Filter,
    /// Request messages since this time.
    pub since: Timestamp,
    /// Request messages until this time.
    pub until: Timestamp,
}

impl Subscribe {
    pub fn all() -> Self {
        Self {
            filter: Filter::default(),
            since: Timestamp::MIN,
            until: Timestamp::MAX,
        }
    }
}

/// Node announcing itself to the network.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeAnnouncement {
    /// Advertized features.
    pub features: node::Features,
    /// Monotonic timestamp.
    pub timestamp: Timestamp,
    /// Non-unique alias. Must be valid UTF-8.
    pub alias: [u8; 32],
    /// Announced addresses.
    pub addresses: BoundedVec<Address, ADDRESS_LIMIT>,
    /// Nonce used for announcement proof-of-work.
    pub nonce: u64,
}

impl NodeAnnouncement {
    /// Validate a node announcement message.
    ///
    /// Checks that the proof-of-work is valid, by generating a single byte that
    /// must be zero.
    ///
    /// `scrypt(encode(announcement)) == 0`
    ///
    pub fn validate(&self) -> bool {
        let (n, r, p) = Announcement::POW_PARAMS;
        let params = scrypt::Params::new(n, r, p).expect("proof-of-work parameters are valid");
        let mut output = [0; 1];

        scrypt::scrypt(
            wire::serialize(self).as_ref(),
            Announcement::POW_SALT,
            &params,
            &mut output,
        )
        .expect("proof-of-work output vector is a valid length");

        output == [0]
    }

    /// Solve the proof-of-work of a node announcement by iterating through different nonces.
    pub fn solve(mut self) -> Self {
        loop {
            if let Some(nonce) = self.nonce.checked_add(1) {
                self.nonce = nonce;

                if self.validate() {
                    break;
                }
            } else {
                // If a very high difficulty is chosen, it's possible to iterate through all
                // possible values of the nonce without solving the puzzle. However, with "normal"
                // values, this is virtually impossible.
                panic!("could not solve proof-of-work!");
            }
        }
        self
    }
}

impl wire::Encode for NodeAnnouncement {
    fn encode<W: io::Write + ?Sized>(&self, writer: &mut W) -> Result<usize, io::Error> {
        let mut n = 0;

        n += self.features.encode(writer)?;
        n += self.timestamp.encode(writer)?;
        n += self.alias.encode(writer)?;
        n += self.addresses.encode(writer)?;
        n += self.nonce.encode(writer)?;

        Ok(n)
    }
}

impl wire::Decode for NodeAnnouncement {
    fn decode<R: std::io::Read + ?Sized>(reader: &mut R) -> Result<Self, wire::Error> {
        let features = node::Features::decode(reader)?;
        let timestamp = Timestamp::decode(reader)?;
        let alias = wire::Decode::decode(reader)?;
        let addresses = BoundedVec::<Address, ADDRESS_LIMIT>::decode(reader)?;
        let nonce = u64::decode(reader)?;

        Ok(Self {
            features,
            timestamp,
            alias,
            addresses,
            nonce,
        })
    }
}

/// Node announcing project refs being created or updated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefsAnnouncement {
    /// Repository identifier.
    pub id: Id,
    /// Updated refs.
    pub refs: BoundedVec<(git::RefString, git::Oid), REF_LIMIT>,
    /// Time of announcement.
    pub timestamp: Timestamp,
}

/// Node announcing its inventory to the network.
/// This should be the whole inventory every time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoryAnnouncement {
    /// Node inventory.
    pub inventory: BoundedVec<Id, INVENTORY_LIMIT>,
    /// Time of announcement.
    pub timestamp: Timestamp,
}

/// Announcement messages are messages that are relayed between peers.
#[derive(Clone, PartialEq, Eq)]
pub enum AnnouncementMessage {
    /// Inventory announcement.
    Inventory(InventoryAnnouncement),
    /// Node announcement.
    Node(NodeAnnouncement),
    /// Refs announcement.
    Refs(RefsAnnouncement),
}

impl AnnouncementMessage {
    /// Sign this announcement message.
    pub fn signed<G: crypto::Signer>(self, signer: &G) -> Announcement {
        let msg = wire::serialize(&self);
        let signature = signer.sign(&msg);

        Announcement {
            node: *signer.public_key(),
            message: self,
            signature,
        }
    }

    pub fn timestamp(&self) -> Timestamp {
        match self {
            Self::Inventory(InventoryAnnouncement { timestamp, .. }) => *timestamp,
            Self::Refs(RefsAnnouncement { timestamp, .. }) => *timestamp,
            Self::Node(NodeAnnouncement { timestamp, .. }) => *timestamp,
        }
    }
}

impl From<NodeAnnouncement> for AnnouncementMessage {
    fn from(ann: NodeAnnouncement) -> Self {
        Self::Node(ann)
    }
}

impl From<InventoryAnnouncement> for AnnouncementMessage {
    fn from(ann: InventoryAnnouncement) -> Self {
        Self::Inventory(ann)
    }
}

impl From<RefsAnnouncement> for AnnouncementMessage {
    fn from(ann: RefsAnnouncement) -> Self {
        Self::Refs(ann)
    }
}

impl fmt::Debug for AnnouncementMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Node { .. } => write!(f, "Node(..)"),
            Self::Inventory(message) => {
                write!(
                    f,
                    "Inventory([{}], {})",
                    message
                        .inventory
                        .iter()
                        .map(|i| i.to_string())
                        .collect::<Vec<String>>()
                        .join(", "),
                    message.timestamp
                )
            }
            Self::Refs(message) => {
                write!(f, "Refs({}, {:?})", message.id, message.refs)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Announcement {
    /// Node identifier.
    pub node: NodeId,
    /// Unsigned node announcement.
    pub message: AnnouncementMessage,
    /// Signature over the announcement.
    pub signature: crypto::Signature,
}

impl Announcement {
    /// Proof-of-work parameters for announcements.
    ///
    /// These parameters are fed into `scrypt`.
    /// They represent the `log2(N)`, `r`, `p` parameters, respectively.
    ///
    /// * log2(N) – iterations count (affects memory and CPU usage), e.g. 15
    /// * r – block size (affects memory and CPU usage), e.g. 8
    /// * p – parallelism factor (threads to run in parallel - affects the memory, CPU usage), usually 1
    ///
    /// `15, 8, 1` are usually the recommended parameters.
    ///
    #[cfg(test)]
    pub const POW_PARAMS: (u8, u32, u32) = (1, 1, 1);
    #[cfg(not(test))]
    pub const POW_PARAMS: (u8, u32, u32) = (15, 8, 1);
    /// Salt used for generating PoW.
    pub const POW_SALT: &[u8] = &[b'r', b'a', b'd'];

    /// Verify this announcement's signature.
    pub fn verify(&self) -> bool {
        let msg = wire::serialize(&self.message);
        self.node.verify(msg, &self.signature).is_ok()
    }

    pub fn matches(&self, filter: &Filter) -> bool {
        match &self.message {
            AnnouncementMessage::Inventory(_) => true,
            AnnouncementMessage::Node(_) => true,
            AnnouncementMessage::Refs(RefsAnnouncement { id, .. }) => filter.contains(id),
        }
    }
}

/// Message payload.
/// These are the messages peers send to each other.
#[derive(Clone, PartialEq, Eq)]
pub enum Message {
    /// The first message sent to a peer after connection.
    Initialize {},

    /// Subscribe to gossip messages matching the filter and time range.
    Subscribe(Subscribe),

    /// Gossip announcement. These messages are relayed to peers, and filtered
    /// using [`Message::Subscribe`].
    Announcement(Announcement),

    /// Ask a connected peer for a Pong.
    ///
    /// Used to check if the remote peer is responsive, or a side-effect free way to keep a
    /// connection alive.
    Ping(Ping),

    /// Response to `Ping` message.
    Pong {
        /// The pong payload.
        zeroes: ZeroBytes,
    },

    /// Request a session upgrade to the Git protocol and fetch the given repository.
    Fetch { rid: Id },

    /// Accept a fetch request.
    FetchOk { rid: Id },
}

impl Message {
    pub fn init() -> Self {
        Self::Initialize {}
    }

    pub fn announcement(
        node: NodeId,
        message: impl Into<AnnouncementMessage>,
        signature: crypto::Signature,
    ) -> Self {
        Announcement {
            node,
            signature,
            message: message.into(),
        }
        .into()
    }

    pub fn node<G: crypto::Signer>(message: NodeAnnouncement, signer: &G) -> Self {
        AnnouncementMessage::from(message).signed(signer).into()
    }

    pub fn inventory<G: crypto::Signer>(message: InventoryAnnouncement, signer: &G) -> Self {
        AnnouncementMessage::from(message).signed(signer).into()
    }

    pub fn subscribe(filter: Filter, since: Timestamp, until: Timestamp) -> Self {
        Self::Subscribe(Subscribe {
            filter,
            since,
            until,
        })
    }
}

/// A ping message.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Ping {
    /// The requested length of the pong message.
    pub ponglen: wire::Size,
    /// Zero bytes (ignored).
    pub zeroes: ZeroBytes,
}

impl Ping {
    /// Maximum number of zero bytes in a ping message.
    pub const MAX_PING_ZEROES: wire::Size = Message::MAX_SIZE // Message size without the type.
        - mem::size_of::<wire::Size>() as wire::Size // Account for pong length.
        - mem::size_of::<wire::Size>() as wire::Size; // Account for zeroes length prefix.

    /// Maximum number of zero bytes in a pong message.
    pub const MAX_PONG_ZEROES: wire::Size =
        Message::MAX_SIZE - mem::size_of::<wire::Size>() as wire::Size; // Account for zeroes length
                                                                        // prefix.

    pub fn new(rng: &mut fastrand::Rng) -> Self {
        let ponglen = rng.u16(0..Self::MAX_PONG_ZEROES);

        Ping {
            ponglen,
            zeroes: ZeroBytes::new(rng.u16(0..Self::MAX_PING_ZEROES)),
        }
    }
}

impl From<Announcement> for Message {
    fn from(ann: Announcement) -> Self {
        Self::Announcement(ann)
    }
}

impl fmt::Debug for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Initialize { .. } => write!(f, "Initialize(..)"),
            Self::Subscribe(Subscribe { since, until, .. }) => {
                write!(f, "Subscribe({since}..{until})")
            }
            Self::Announcement(Announcement { node, message, .. }) => {
                write!(f, "Announcement({node}, {message:?})")
            }
            Self::Ping(Ping { ponglen, zeroes }) => write!(f, "Ping({ponglen}, {zeroes:?})"),
            Self::Pong { zeroes } => write!(f, "Pong({zeroes:?})"),
            Self::Fetch { rid } => write!(f, "Fetch({rid})"),
            Self::FetchOk { rid } => write!(f, "FetchOk({rid})"),
        }
    }
}

/// Represents a vector of zeroes of a certain length.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ZeroBytes(wire::Size);

impl ZeroBytes {
    pub fn new(size: wire::Size) -> Self {
        ZeroBytes(size)
    }

    pub fn is_empty(&self) -> bool {
        self.0 == 0
    }

    pub fn len(&self) -> usize {
        self.0.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::*;
    use crate::wire::Encode;

    use crate::crypto::test::signer::MockSigner;
    use crate::test::arbitrary;
    use fastrand;
    use qcheck_macros::quickcheck;

    #[test]
    fn test_ref_limit() {
        let mut refs = Refs::default();
        while refs.len() < REF_LIMIT {
            refs.insert(arbitrary::refstring(u8::MAX as usize), arbitrary::oid());
        }

        let bounded_refs = BoundedVec::collect_from(&mut refs.iter().map(|(a, b)| (a.clone(), *b)));
        let msg: Message = AnnouncementMessage::from(RefsAnnouncement {
            id: arbitrary::gen(1),
            refs: bounded_refs,
            timestamp: LocalTime::now().as_secs(),
        })
        .signed(&MockSigner::default())
        .into();

        let mut buf: Vec<u8> = Vec::new();
        assert!(
            msg.encode(&mut buf).is_ok(),
            "REF_LIMIT is too big to support message encoding",
        );

        let decoded = wire::deserialize(buf.as_slice());
        assert!(
            decoded.is_ok(),
            "REF_LIMIT is too big to support message decoding"
        );
        assert_eq!(
            msg,
            decoded.unwrap(),
            "encoding and decoding should be safe for message at REF_LIMIT",
        );
    }

    #[test]
    fn test_inventory_limit() {
        let msg = Message::inventory(
            InventoryAnnouncement {
                inventory: arbitrary::vec(INVENTORY_LIMIT)
                    .try_into()
                    .expect("size within bounds limit"),
                timestamp: LocalTime::now().as_secs(),
            },
            &MockSigner::default(),
        );
        let mut buf: Vec<u8> = Vec::new();
        assert!(
            msg.encode(&mut buf).is_ok(),
            "INVENTORY_LIMIT is a valid limit for encoding",
        );

        let decoded = wire::deserialize(buf.as_slice());
        assert!(
            decoded.is_ok(),
            "INVENTORY_LIMIT is a valid limit for decoding"
        );
        assert_eq!(
            msg,
            decoded.unwrap(),
            "encoding and decoding should be safe for message at INVENTORY_LIMIT",
        );
    }

    #[quickcheck]
    fn prop_refs_announcement_signing(id: Id, refs: Refs) {
        let signer = MockSigner::new(&mut fastrand::Rng::new());
        let timestamp = 0;

        let message = AnnouncementMessage::Refs(RefsAnnouncement {
            id,
            refs: BoundedVec::collect_from(&mut refs.iter().map(|(k, v)| (k.clone(), *v))),
            timestamp,
        });
        let ann = message.signed(&signer);

        assert!(ann.verify());
    }

    #[test]
    fn test_node_announcement_validate() {
        let ann = NodeAnnouncement {
            features: node::Features::SEED,
            timestamp: 42491841,
            alias: [0; 32],
            addresses: BoundedVec::new(),
            nonce: 0,
        };

        assert!(!ann.validate());
        assert!(ann.solve().validate());
    }
}

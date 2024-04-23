use std::{fmt, io, mem};

use nonempty::NonEmpty;
use radicle::git;
use radicle::storage::refs::RefsAt;

use crate::crypto;
use crate::identity::RepoId;
use crate::node;
use crate::node::{Address, Alias};
use crate::prelude::BoundedVec;
use crate::service::filter::Filter;
use crate::service::{Link, NodeId, Timestamp};
use crate::storage;
use crate::wire;

/// Maximum number of addresses which can be announced to other nodes.
pub const ADDRESS_LIMIT: usize = 16;
/// Maximum number of repository remotes that can be included in a [`RefsAnnouncement`] message.
pub const REF_REMOTE_LIMIT: usize = 1024;
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
    /// Non-unique alias.
    pub alias: Alias,
    /// Announced addresses.
    pub addresses: BoundedVec<Address, ADDRESS_LIMIT>,
    /// Nonce used for announcement proof-of-work.
    pub nonce: u64,
}

impl NodeAnnouncement {
    /// Calculate the amount of work that went into creating this announcement.
    ///
    /// Proof-of-work uses the [`scrypt`] algorithm with the parameters in
    /// [`Announcement::POW_PARAMS`]. The "work" is calculated by counting the number of leading
    /// zero bits after running `scrypt` on a serialized [`NodeAnnouncement`] using
    /// [`wire::serialize`].
    ///
    /// In other words, `work = leading-zeros(scrypt(serialize(announcement)))`.
    ///
    /// Higher numbers mean higher difficulty. For each increase in work, difficulty is doubled.
    /// For instance, an output of `7` is *four* times more work than an output of `5`.
    ///
    pub fn work(&self) -> u32 {
        let (n, r, p) = Announcement::POW_PARAMS;
        let params = scrypt::Params::new(n, r, p, 32).expect("proof-of-work parameters are valid");
        let mut output = [0u8; 32];

        scrypt::scrypt(
            wire::serialize(self).as_ref(),
            Announcement::POW_SALT,
            &params,
            &mut output,
        )
        .expect("proof-of-work output vector is a valid length");

        // Calculate the number of leading zero bits in the output vector.
        if let Some((zero_bytes, non_zero)) = output.iter().enumerate().find(|(_, &x)| x != 0) {
            zero_bytes as u32 * 8 + non_zero.leading_zeros()
        } else {
            output.len() as u32 * 8
        }
    }

    /// Solve the proof-of-work of a node announcement for the given target, by iterating through
    /// different nonces.
    ///
    /// If the given difficulty target is too high, there may not be a result. In that case, `None`
    /// is returned.
    pub fn solve(mut self, target: u32) -> Option<Self> {
        loop {
            if let Some(nonce) = self.nonce.checked_add(1) {
                self.nonce = nonce;

                if self.work() >= target {
                    break;
                }
            } else {
                return None;
            }
        }
        Some(self)
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
    pub rid: RepoId,
    /// Updated `rad/sigrefs`.
    pub refs: BoundedVec<RefsAt, REF_REMOTE_LIMIT>,
    /// Time of announcement.
    pub timestamp: Timestamp,
}

/// Track the status of `RefsAt` within a given repository.
#[derive(Default)]
pub struct RefsStatus {
    /// The `rad/sigrefs` was missing or it's ahead of the local
    /// `rad/sigrefs`. We want it.
    pub want: Vec<RefsAt>,
    /// The `rad/sigrefs` has been seen before. We already have it.
    pub have: Vec<RefsAt>,
}

impl RefsStatus {
    /// Get the set of `want` and `have` `RefsAt`'s for the given
    /// announcement.
    ///
    /// Nb. We use the refs database as a cache for quick lookups. This does *not* check
    /// for ancestry matches, since we don't cache the whole history (only the tips).
    /// This, however, is not a problem because the signed refs branch is fast-forward only,
    /// and old refs announcements will be discarded due to their lower timestamps.
    pub fn new<D: node::refs::Store>(
        rid: RepoId,
        refs: NonEmpty<RefsAt>,
        db: &D,
    ) -> Result<RefsStatus, storage::Error> {
        let mut status = RefsStatus::default();
        for theirs in refs.iter() {
            status.insert(&rid, *theirs, db)?;
        }
        Ok(status)
    }

    fn insert<D: node::refs::Store>(
        &mut self,
        repo: &RepoId,
        theirs: RefsAt,
        db: &D,
    ) -> Result<(), storage::Error> {
        match db.get(repo, &theirs.remote, &storage::refs::SIGREFS_BRANCH) {
            Ok(Some((ours, _))) => {
                if theirs.at != ours {
                    self.want.push(theirs);
                } else {
                    self.have.push(theirs);
                }
            }
            Ok(None) => {
                self.want.push(theirs);
            }
            Err(e) => {
                log::warn!(
                    target: "service",
                    "Error getting cached ref of {repo} for refs status: {e}"
                );
            }
        }
        Ok(())
    }
}

/// Node announcing its inventory to the network.
/// This should be the whole inventory every time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoryAnnouncement {
    /// Node inventory.
    pub inventory: BoundedVec<RepoId, INVENTORY_LIMIT>,
    /// Time of announcement.
    pub timestamp: Timestamp,
}

/// Node announcing information to a connected peer.
///
/// This should not be relayed and should be used to send an
/// informational message a peer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Info {
    /// Tell a node that sent a refs announcement that it was already synced at the given `Oid`,
    /// for this particular `rid`.
    RefsAlreadySynced { rid: RepoId, at: git::Oid },
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
            Self::Node(message) => write!(f, "Node({})", message.timestamp),
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
                write!(
                    f,
                    "Refs({}, {}, {:?})",
                    message.rid, message.timestamp, message.refs
                )
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
    #[cfg(debug_assertions)]
    pub const POW_PARAMS: (u8, u32, u32) = (1, 1, 1);
    #[cfg(not(debug_assertions))]
    pub const POW_PARAMS: (u8, u32, u32) = (15, 8, 1);
    /// Salt used for generating PoW.
    pub const POW_SALT: &'static [u8] = &[b'r', b'a', b'd'];

    /// Verify this announcement's signature.
    pub fn verify(&self) -> bool {
        let msg = wire::serialize(&self.message);
        self.node.verify(msg, &self.signature).is_ok()
    }

    pub fn matches(&self, filter: &Filter) -> bool {
        match &self.message {
            AnnouncementMessage::Inventory(_) => true,
            AnnouncementMessage::Node(_) => true,
            AnnouncementMessage::Refs(RefsAnnouncement { rid, .. }) => filter.contains(rid),
        }
    }

    /// Check whether this announcement is of the same variant as another.
    pub fn variant_eq(&self, other: &Self) -> bool {
        std::mem::discriminant(&self.message) == std::mem::discriminant(&other.message)
    }

    /// Get the announcement timestamp.
    pub fn timestamp(&self) -> Timestamp {
        self.message.timestamp()
    }
}

/// Message payload.
/// These are the messages peers send to each other.
#[derive(Clone, PartialEq, Eq)]
pub enum Message {
    /// Subscribe to gossip messages matching the filter and time range.
    Subscribe(Subscribe),

    /// Gossip announcement. These messages are relayed to peers, and filtered
    /// using [`Message::Subscribe`].
    Announcement(Announcement),

    /// Informational message. These messages are sent between peers for information
    /// and do not need to be acted upon. They can be safely ignored, though handling
    /// them can be useful for the user.
    Info(Info),

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
}

impl PartialOrd for Message {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Message {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let this = wire::serialize(self);
        let other = wire::serialize(other);

        this.cmp(&other)
    }
}

impl Message {
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

    pub fn log(&self, level: log::Level, remote: &NodeId, link: Link) {
        if !log::log_enabled!(level) {
            return;
        }
        let (verb, prep) = if link.is_inbound() {
            ("Received", "from")
        } else {
            ("Sending", "to")
        };
        let msg = match self {
            Self::Announcement(Announcement { node, message, .. }) => match message {
                AnnouncementMessage::Node(NodeAnnouncement { addresses, timestamp, .. }) => format!(
                    "{verb} node announcement of {node} with {} address(es) {prep} {remote} (t={timestamp})",
                    addresses.len()
                ),
                AnnouncementMessage::Refs(RefsAnnouncement { rid, refs, timestamp }) => format!(
                    "{verb} refs announcement of {node} for {rid} with {} remote(s) {prep} {remote} (t={timestamp})",
                    refs.len()
                ),
                AnnouncementMessage::Inventory(InventoryAnnouncement { inventory, timestamp }) => {
                    format!(
                        "{verb} inventory announcement of {node} with {} item(s) {prep} {remote} (t={timestamp})",
                        inventory.len()
                    )
                }
            },
            Self::Info(Info::RefsAlreadySynced { rid,  .. }) => {
                format!(
                    "{verb} `refs-already-synced` info {prep} {remote} for {rid}"
                )
            },
            Self::Ping { .. } => format!("{verb} ping {prep} {remote}"),
            Self::Pong { .. } => format!("{verb} pong {prep} {remote}"),
            Self::Subscribe(Subscribe { .. }) => {
                format!("{verb} subscription filter {prep} {remote}")
            }
        };
        log::log!(target: "service", level, "{msg}");
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

impl From<Info> for Message {
    fn from(info: Info) -> Self {
        Self::Info(info)
    }
}

impl fmt::Debug for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Subscribe(Subscribe { since, until, .. }) => {
                write!(f, "Subscribe({since}..{until})")
            }
            Self::Announcement(Announcement { node, message, .. }) => {
                write!(f, "Announcement({node}, {message:?})")
            }
            Self::Info(info) => {
                write!(f, "Info({info:?})")
            }
            Self::Ping(Ping { ponglen, zeroes }) => write!(f, "Ping({ponglen}, {zeroes:?})"),
            Self::Pong { zeroes } => write!(f, "Pong({zeroes:?})"),
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
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::prelude::*;
    use crate::wire::Encode;

    use crate::crypto::test::signer::MockSigner;
    use crate::test::arbitrary;
    use fastrand;
    use qcheck_macros::quickcheck;
    use radicle::git::raw;

    #[test]
    fn test_ref_remote_limit() {
        let mut refs = BoundedVec::<_, REF_REMOTE_LIMIT>::new();
        let signer = MockSigner::default();
        let at = raw::Oid::zero().into();

        assert_eq!(refs.capacity(), REF_REMOTE_LIMIT);

        for _ in 0..refs.capacity() {
            refs.push(RefsAt {
                remote: *signer.public_key(),
                at,
            })
            .unwrap();
        }

        let msg: Message = AnnouncementMessage::from(RefsAnnouncement {
            rid: arbitrary::gen(1),
            refs,
            timestamp: LocalTime::now().into(),
        })
        .signed(&MockSigner::default())
        .into();

        let mut buf: Vec<u8> = Vec::new();
        assert!(msg.encode(&mut buf).is_ok());

        let decoded = wire::deserialize(buf.as_slice());
        assert!(decoded.is_ok());
        assert_eq!(msg, decoded.unwrap());
    }

    #[test]
    fn test_inventory_limit() {
        let msg = Message::inventory(
            InventoryAnnouncement {
                inventory: arbitrary::vec(INVENTORY_LIMIT)
                    .try_into()
                    .expect("size within bounds limit"),
                timestamp: LocalTime::now().into(),
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
    fn prop_refs_announcement_signing(rid: RepoId) {
        let signer = MockSigner::new(&mut fastrand::Rng::new());
        let timestamp = Timestamp::EPOCH;
        let at = raw::Oid::zero().into();
        let refs = BoundedVec::collect_from(
            &mut [RefsAt {
                remote: *signer.public_key(),
                at,
            }]
            .into_iter(),
        );
        let message = AnnouncementMessage::Refs(RefsAnnouncement {
            rid,
            refs,
            timestamp,
        });
        let ann = message.signed(&signer);

        assert!(ann.verify());
    }

    #[test]
    fn test_node_announcement_validate() {
        let ann = NodeAnnouncement {
            features: node::Features::SEED,
            timestamp: Timestamp::from(42491841),
            alias: Alias::new("alice"),
            addresses: BoundedVec::new(),
            nonce: 0,
        };

        assert_eq!(ann.work(), 0);
        assert_eq!(ann.clone().solve(1).unwrap().work(), 4);
        assert_eq!(ann.clone().solve(8).unwrap().work(), 9);
        assert_eq!(ann.solve(14).unwrap().work(), 14);
    }
}

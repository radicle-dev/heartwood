use std::collections::{HashSet, VecDeque};
use std::{fmt, time};

use crossbeam_channel as chan;

use crate::node::config::Limits;
use crate::node::{FetchResult, Severity};
use crate::service::message;
use crate::service::message::Message;
use crate::service::{Address, LocalDuration, LocalTime, NodeId, Outbox, RepoId, Rng};
use crate::storage::refs::RefsAt;
use crate::{Link, Timestamp};

pub use crate::node::{PingState, State};

/// Time after which a connection is considered stable.
pub const CONNECTION_STABLE_THRESHOLD: LocalDuration = LocalDuration::from_mins(1);
/// Maximum items in the fetch queue.
pub const MAX_FETCH_QUEUE_SIZE: usize = 128;

#[derive(thiserror::Error, Debug, Clone, Copy)]
pub enum Error {
    /// The remote peer sent an invalid announcement timestamp,
    /// for eg. a timestamp far in the future.
    #[error("invalid announcement timestamp: {0}")]
    InvalidTimestamp(Timestamp),
    /// The remote peer sent git protocol messages while we were expecting
    /// gossip messages. Or vice-versa.
    #[error("protocol mismatch")]
    ProtocolMismatch,
    /// The remote peer did something that violates the protocol rules.
    #[error("peer misbehaved")]
    Misbehavior,
    /// The remote peer timed out.
    #[error("peer timed out")]
    Timeout,
}

impl Error {
    /// Return the severity for this error.
    pub fn severity(&self) -> Severity {
        match self {
            Self::InvalidTimestamp(_) => Severity::High,
            Self::ProtocolMismatch => Severity::High,
            Self::Misbehavior => Severity::High,
            Self::Timeout => Severity::Low,
        }
    }
}

/// Error when trying to queue a fetch.
#[derive(thiserror::Error, Debug, Clone)]
pub enum QueueError {
    /// The item already exists in the queue.
    #[error("item is already queued")]
    Duplicate(QueuedFetch),
    /// The queue is at capacity.
    #[error("queue capacity reached")]
    CapacityReached(QueuedFetch),
}

impl QueueError {
    /// Get the inner [`QueuedFetch`].
    pub fn inner(&self) -> &QueuedFetch {
        match self {
            Self::Duplicate(f) => f,
            Self::CapacityReached(f) => f,
        }
    }
}

/// Fetch waiting to be processed, in the fetch queue.
#[derive(Debug, Clone)]
pub struct QueuedFetch {
    /// Repo being fetched.
    pub rid: RepoId,
    /// Peer being fetched from.
    pub from: NodeId,
    /// Refs being fetched.
    pub refs_at: Vec<RefsAt>,
    /// The timeout given for the fetch request.
    pub timeout: time::Duration,
    /// Result channel.
    pub channel: Option<chan::Sender<FetchResult>>,
}

impl PartialEq for QueuedFetch {
    fn eq(&self, other: &Self) -> bool {
        self.rid == other.rid
            && self.from == other.from
            && self.refs_at == other.refs_at
            && self.channel.is_none()
            && other.channel.is_none()
    }
}

/// A peer session. Each connected peer will have one session.
#[derive(Debug, Clone)]
pub struct Session {
    /// Peer id.
    pub id: NodeId,
    /// Peer address.
    pub addr: Address,
    /// Connection direction.
    pub link: Link,
    /// Whether we should attempt to re-connect
    /// to this peer upon disconnection.
    pub persistent: bool,
    /// Peer connection state.
    pub state: State,
    /// Peer subscription.
    pub subscribe: Option<message::Subscribe>,
    /// Last time a message was received from the peer.
    pub last_active: LocalTime,
    /// Fetch queue.
    pub queue: VecDeque<QueuedFetch>,

    /// Connection attempts. For persistent peers, Tracks
    /// how many times we've attempted to connect. We reset this to zero
    /// upon successful connection, once the connection is stable.
    attempts: usize,
    /// Source of entropy.
    rng: Rng,
    /// Protocol limits.
    limits: Limits,
}

impl fmt::Display for Session {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut attrs = Vec::new();
        let state = self.state.to_string();

        if self.link.is_inbound() {
            attrs.push("inbound");
        } else {
            attrs.push("outbound");
        }
        if self.persistent {
            attrs.push("persistent");
        }
        attrs.push(state.as_str());

        write!(f, "{} [{}]", self.id, attrs.join(" "))
    }
}

impl From<&Session> for radicle::node::Session {
    fn from(s: &Session) -> Self {
        Self {
            nid: s.id,
            link: if s.link.is_inbound() {
                radicle::node::Link::Inbound
            } else {
                radicle::node::Link::Outbound
            },
            addr: s.addr.clone(),
            state: s.state.clone(),
        }
    }
}

impl Session {
    pub fn outbound(id: NodeId, addr: Address, persistent: bool, rng: Rng, limits: Limits) -> Self {
        Self {
            id,
            addr,
            state: State::Initial,
            link: Link::Outbound,
            subscribe: None,
            persistent,
            last_active: LocalTime::default(),
            queue: VecDeque::with_capacity(MAX_FETCH_QUEUE_SIZE),
            attempts: 1,
            rng,
            limits,
        }
    }

    pub fn inbound(
        id: NodeId,
        addr: Address,
        persistent: bool,
        rng: Rng,
        time: LocalTime,
        limits: Limits,
    ) -> Self {
        Self {
            id,
            addr,
            state: State::Connected {
                since: time,
                ping: PingState::default(),
                fetching: HashSet::default(),
                latencies: VecDeque::default(),
                stable: false,
            },
            link: Link::Inbound,
            subscribe: None,
            persistent,
            last_active: time,
            queue: VecDeque::new(),
            attempts: 0,
            rng,
            limits,
        }
    }

    pub fn is_connecting(&self) -> bool {
        matches!(self.state, State::Attempted { .. })
    }

    pub fn is_stable(&self) -> bool {
        matches!(self.state, State::Connected { stable: true, .. })
    }

    pub fn is_connected(&self) -> bool {
        self.state.is_connected()
    }

    pub fn is_disconnected(&self) -> bool {
        matches!(self.state, State::Disconnected { .. })
    }

    pub fn is_initial(&self) -> bool {
        matches!(self.state, State::Initial)
    }

    pub fn is_at_capacity(&self) -> bool {
        if let State::Connected { fetching, .. } = &self.state {
            if fetching.len() >= self.limits.fetch_concurrency {
                return true;
            }
        }
        false
    }

    pub fn is_fetching(&self, rid: &RepoId) -> bool {
        if let State::Connected { fetching, .. } = &self.state {
            return fetching.contains(rid);
        }
        false
    }

    /// Queue a fetch. Returns `true` if it was added to the queue, and `false` if
    /// it already was present in the queue.
    pub fn queue_fetch(&mut self, fetch: QueuedFetch) -> Result<(), QueueError> {
        assert_eq!(fetch.from, self.id);

        if self.queue.len() >= MAX_FETCH_QUEUE_SIZE {
            return Err(QueueError::CapacityReached(fetch));
        } else if self.queue.contains(&fetch) {
            return Err(QueueError::Duplicate(fetch));
        }
        self.queue.push_back(fetch);

        Ok(())
    }

    pub fn dequeue_fetch(&mut self) -> Option<QueuedFetch> {
        self.queue.pop_front()
    }

    pub fn attempts(&self) -> usize {
        self.attempts
    }

    /// Run 'idle' task for session.
    pub fn idle(&mut self, now: LocalTime) {
        if let State::Connected {
            since,
            ref mut stable,
            ..
        } = self.state
        {
            if now >= since && now.duration_since(since) >= CONNECTION_STABLE_THRESHOLD {
                *stable = true;
                // Reset number of attempts for stable connections.
                self.attempts = 0;
            }
        }
    }

    /// Mark this session as fetching the given RID.
    ///
    /// # Panics
    ///
    /// If it is already fetching that RID, or the session is disconnected.
    pub fn fetching(&mut self, rid: RepoId) {
        if let State::Connected { fetching, .. } = &mut self.state {
            assert!(
                fetching.insert(rid),
                "Session must not already be fetching {rid}"
            );
        } else {
            panic!(
                "Attempting to fetch {rid} from disconnected session {}",
                self.id
            );
        }
    }

    pub fn fetched(&mut self, rid: RepoId) {
        if let State::Connected { fetching, .. } = &mut self.state {
            if !fetching.remove(&rid) {
                log::warn!(target: "service", "Fetched unknown repository {rid}");
            }
        }
    }

    pub fn to_attempted(&mut self) {
        assert!(
            self.is_initial(),
            "Can only transition to 'attempted' state from 'initial' state"
        );
        self.state = State::Attempted;
        self.attempts += 1;
    }

    pub fn to_connected(&mut self, since: LocalTime) {
        self.last_active = since;

        if let State::Connected { .. } = &self.state {
            log::error!(target: "service", "Session {} is already in 'connected' state, resetting..", self.id);
        };
        self.state = State::Connected {
            since,
            ping: PingState::default(),
            fetching: HashSet::default(),
            latencies: VecDeque::default(),
            stable: false,
        };
    }

    /// Move the session state to "disconnected". Returns any pending RID
    /// that was requested.
    pub fn to_disconnected(&mut self, since: LocalTime, retry_at: LocalTime) {
        self.state = State::Disconnected { since, retry_at };
    }

    /// Return to initial state from disconnected state. This state transition
    /// happens when we attempt to re-connect to a disconnected peer.
    pub fn to_initial(&mut self) {
        assert!(
            self.is_disconnected(),
            "Can only transition to 'initial' state from 'disconnected' state"
        );
        self.state = State::Initial;
    }

    pub fn ping(&mut self, since: LocalTime, reactor: &mut Outbox) -> Result<(), Error> {
        if let State::Connected { ping, .. } = &mut self.state {
            let msg = message::Ping::new(&mut self.rng);
            *ping = PingState::AwaitingResponse {
                len: msg.ponglen,
                since,
            };
            reactor.write(self, Message::Ping(msg));
        }
        Ok(())
    }
}

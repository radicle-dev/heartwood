use std::collections::{HashSet, VecDeque};
use std::fmt;

use crate::node::config::Limits;
use crate::service::message;
use crate::service::message::Message;
use crate::service::{Address, Id, LocalTime, NodeId, Outbox, Rng};
use crate::Link;

pub use crate::node::{PingState, State};

/// Return value of [`Session::fetch`].
#[derive(Debug)]
pub enum FetchResult {
    /// Maximum concurrent fetches reached.
    Queued,
    /// We are already fetching the given repo from this peer.
    AlreadyFetching,
    /// Ok, ready to fetch.
    Ready,
    /// This peer is not ready to fetch.
    NotConnected,
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    /// The remote peer sent an invalid announcement timestamp,
    /// for eg. a timestamp far in the future.
    #[error("invalid announcement timestamp: {0}")]
    InvalidTimestamp(u64),
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
    /// Check whether this error is transient.
    pub fn is_transient(&self) -> bool {
        match self {
            Self::InvalidTimestamp(_) => false,
            Self::ProtocolMismatch => true,
            Self::Misbehavior => false,
            Self::Timeout => true,
        }
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
    pub queue: VecDeque<Id>,

    /// Connection attempts. For persistent peers, Tracks
    /// how many times we've attempted to connect. We reset this to zero
    /// upon successful connection.
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
            queue: VecDeque::default(),
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
            },
            link: Link::Inbound,
            subscribe: None,
            persistent,
            last_active: LocalTime::default(),
            queue: VecDeque::default(),
            attempts: 0,
            rng,
            limits,
        }
    }

    pub fn is_connecting(&self) -> bool {
        matches!(self.state, State::Attempted { .. })
    }

    pub fn is_connected(&self) -> bool {
        matches!(self.state, State::Connected { .. })
    }

    pub fn is_disconnected(&self) -> bool {
        matches!(self.state, State::Disconnected { .. })
    }

    pub fn is_initial(&self) -> bool {
        matches!(self.state, State::Initial)
    }

    pub fn attempts(&self) -> usize {
        self.attempts
    }

    pub fn fetch(&mut self, rid: Id) -> FetchResult {
        if let State::Connected { fetching, .. } = &mut self.state {
            if fetching.contains(&rid) || self.queue.contains(&rid) {
                return FetchResult::AlreadyFetching;
            }
            if fetching.len() >= self.limits.fetch_concurrency {
                self.queue.push_back(rid);
                return FetchResult::Queued;
            }
            fetching.insert(rid);

            FetchResult::Ready
        } else {
            FetchResult::NotConnected
        }
    }

    pub fn fetched(&mut self, rid: Id) -> Option<Id> {
        if let State::Connected { fetching, .. } = &mut self.state {
            if !fetching.remove(&rid) {
                log::error!(target: "service", "Fetched unknown repository {rid}");
            }
            // Dequeue the next fetch, if any.
            if let Some(rid) = self.queue.pop_front() {
                return Some(rid);
            }
        }
        None
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
        self.attempts = 0;

        let State::Attempted = &self.state else {
            panic!("Session::to_connected: can only transition to 'connected' state from 'attempted' state");
        };
        self.state = State::Connected {
            since,
            ping: PingState::default(),
            fetching: HashSet::default(),
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

    pub fn fetching(&self) -> HashSet<Id> {
        if let State::Connected { fetching, .. } = &self.state {
            fetching.clone()
        } else {
            HashSet::default()
        }
    }

    pub fn ping(&mut self, reactor: &mut Outbox) -> Result<(), Error> {
        if let State::Connected { ping, .. } = &mut self.state {
            let msg = message::Ping::new(&mut self.rng);
            *ping = PingState::AwaitingResponse(msg.ponglen);

            reactor.write(self, Message::Ping(msg));
        }
        Ok(())
    }
}

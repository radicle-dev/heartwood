use std::collections::{HashSet, VecDeque};
use std::fmt;

use crate::node::config::Limits;
use crate::node::Severity;
use crate::service::message;
use crate::service::message::Message;
use crate::service::{Address, LocalTime, NodeId, Outbox, RepoId, Rng};
use crate::{Link, Timestamp};

pub use crate::node::{PingState, State};

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
            },
            link: Link::Inbound,
            subscribe: None,
            persistent,
            last_active: time,
            attempts: 0,
            rng,
            limits,
        }
    }

    pub fn is_connecting(&self) -> bool {
        matches!(self.state, State::Attempted { .. })
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

    pub fn attempts(&self) -> usize {
        self.attempts
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
        self.attempts = 0;
        self.last_active = since;

        let State::Attempted = &self.state else {
            panic!("Session::to_connected: can only transition to 'connected' state from 'attempted' state");
        };
        self.state = State::Connected {
            since,
            ping: PingState::default(),
            fetching: HashSet::default(),
            latencies: VecDeque::default(),
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

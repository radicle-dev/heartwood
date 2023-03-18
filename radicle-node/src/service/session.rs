use std::collections::VecDeque;
use std::fmt;

use radicle::storage::Namespaces;

use crate::service::message;
use crate::service::message::Message;
use crate::service::storage;
use crate::service::{Id, LocalTime, NodeId, Reactor, Rng};
use crate::Link;

#[derive(Debug, Copy, Clone, Default, PartialEq, Eq)]
pub enum PingState {
    #[default]
    /// The peer has not been sent a ping.
    None,
    /// A ping has been sent and is waiting on the peer's response.
    AwaitingResponse(u16),
    /// The peer was successfully pinged.
    Ok,
}

/// Sub-state of the gossip protocol.
#[derive(Debug, Default, PartialEq, Eq, Clone)]
pub enum GossipState {
    /// Regular gossip, no pending fetch requests.
    #[default]
    Idle,
    /// Requesting a fetch for the given RID. Waiting for a [`Message::FetchOk`].
    Requesting { rid: Id, namespaces: Namespaces },
}

/// Session protocol.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Protocol {
    /// The default message-based gossip protocol.
    Gossip { state: GossipState },
    /// Git smart protocol. Used for fetching repository data.
    /// This protocol is used after a connection upgrade via the
    /// [`Message::Fetch`] message.
    Fetch { rid: Id },
}

impl Default for Protocol {
    fn default() -> Self {
        Self::Gossip {
            state: GossipState::default(),
        }
    }
}

#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum State {
    /// Initial state for outgoing connections.
    Initial,
    /// Connection attempted successfully.
    Attempted,
    /// Initial state after handshake protocol hand-off.
    Connected {
        /// Connected since this time.
        since: LocalTime,
        /// Ping state.
        ping: PingState,
        /// Session protocol.
        protocol: Protocol,
    },
    /// When a peer is disconnected.
    Disconnected {
        /// Since when has this peer been disconnected.
        since: LocalTime,
        /// When to retry the connection.
        retry_at: LocalTime,
    },
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Initial => {
                write!(f, "initial")
            }
            Self::Attempted => {
                write!(f, "attempted")
            }
            Self::Connected { protocol, .. } => match protocol {
                Protocol::Gossip {
                    state: GossipState::Idle,
                    ..
                } => {
                    write!(f, "connected <gossip>")
                }
                Protocol::Gossip {
                    state: GossipState::Requesting { rid, .. },
                    ..
                } => {
                    write!(f, "connected <gossip> requested={rid}")
                }
                Protocol::Fetch { .. } => {
                    write!(f, "connected <fetch>")
                }
            },
            Self::Disconnected { .. } => {
                write!(f, "disconnected")
            }
        }
    }
}

/// Return value of [`Session::fetch`].
#[derive(Debug)]
pub enum FetchResult {
    /// We are already fetching from this peer.
    AlreadyFetching(Id),
    /// Ok, ready to fetch.
    Ready(Message),
    /// This peer is not ready to fetch.
    NotConnected,
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("wrong protocol version in message: {0}")]
    WrongVersion(u32),
    #[error("invalid announcement timestamp: {0}")]
    InvalidTimestamp(u64),
    #[error("session not found for node `{0}`")]
    NotFound(NodeId),
    #[error("verification failed on fetch: {0}")]
    VerificationFailed(#[from] storage::VerifyError),
    #[error("peer misbehaved")]
    Misbehavior,
    #[error("peer timed out")]
    Timeout,
    #[error("handshake error")]
    Handshake(String),
    #[error("failed to inspect remotes for fetch: {0}")]
    Remotes(#[from] storage::refs::Error),
}

/// A peer session. Each connected peer will have one session.
#[derive(Debug, Clone)]
pub struct Session {
    /// Peer id.
    pub id: NodeId,
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

    /// Fetches queued due to another ongoing fetch.
    pending_fetches: VecDeque<Id>,

    /// Connection attempts. For persistent peers, Tracks
    /// how many times we've attempted to connect. We reset this to zero
    /// upon successful connection.
    attempts: usize,

    /// Source of entropy.
    rng: Rng,
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
    pub fn outbound(id: NodeId, persistent: bool, rng: Rng) -> Self {
        Self {
            id,
            state: State::Initial,
            link: Link::Outbound,
            subscribe: None,
            persistent,
            last_active: LocalTime::default(),
            attempts: 1,
            pending_fetches: VecDeque::new(),
            rng,
        }
    }

    pub fn inbound(id: NodeId, persistent: bool, rng: Rng, time: LocalTime) -> Self {
        Self {
            id,
            state: State::Connected {
                since: time,
                ping: PingState::default(),
                protocol: Protocol::default(),
            },
            link: Link::Inbound,
            subscribe: None,
            persistent,
            last_active: LocalTime::default(),
            attempts: 0,
            pending_fetches: VecDeque::new(),
            rng,
        }
    }

    pub fn is_connecting(&self) -> bool {
        matches!(self.state, State::Attempted { .. })
    }

    pub fn is_connected(&self) -> bool {
        matches!(self.state, State::Connected { .. })
    }

    pub fn is_fetching(&self) -> bool {
        matches!(
            self.state,
            State::Connected {
                protocol: Protocol::Fetch { .. },
                ..
            }
        )
    }

    pub fn is_disconnected(&self) -> bool {
        matches!(self.state, State::Disconnected { .. })
    }

    pub fn is_initial(&self) -> bool {
        matches!(self.state, State::Initial)
    }

    pub fn is_requesting(&self) -> bool {
        matches!(
            self.state,
            State::Connected {
                protocol: Protocol::Gossip {
                    state: GossipState::Requesting { .. }
                },
                ..
            }
        )
    }

    pub fn attempts(&self) -> usize {
        self.attempts
    }

    pub fn fetch(&self, rid: Id) -> FetchResult {
        if let State::Connected { protocol, .. } = &self.state {
            match protocol {
                Protocol::Gossip { state } => {
                    if let GossipState::Requesting { rid, .. } = state {
                        FetchResult::AlreadyFetching(*rid)
                    } else {
                        FetchResult::Ready(Message::Fetch { rid })
                    }
                }
                Protocol::Fetch { rid } => FetchResult::AlreadyFetching(*rid),
            }
        } else {
            FetchResult::NotConnected
        }
    }

    pub fn to_requesting(&mut self, rid: Id, namespaces: Namespaces) {
        let State::Connected { protocol, .. } = &mut self.state else {
            panic!("Session::to_requesting: cannot transition to 'requesting': session is not connected");
        };
        *protocol = Protocol::Gossip {
            state: GossipState::Requesting { rid, namespaces },
        };
    }

    pub fn to_fetching(&mut self, rid: Id) {
        let State::Connected { protocol, .. } = &mut self.state else {
            panic!("Session::to_fetching: cannot transition to 'fetching': session is not connected");
        };
        *protocol = Protocol::Fetch { rid };
    }

    pub fn to_gossip(&mut self) {
        if let State::Connected { protocol, .. } = &mut self.state {
            if let Protocol::Fetch { .. } = protocol {
                *protocol = Protocol::default();
            } else {
                panic!(
                    "Unexpected session state for {}: expected 'fetch' protocol, got 'gossip'",
                    self.id
                );
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
        assert!(
            self.is_connecting(),
            "Can only transition to 'connected' state from 'connecting' state"
        );
        self.attempts = 0;
        self.state = State::Connected {
            since,
            ping: PingState::default(),
            protocol: Protocol::default(),
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

    pub fn requesting(&self) -> Option<(&Id, &Namespaces)> {
        if let State::Connected {
            protocol:
                Protocol::Gossip {
                    state: GossipState::Requesting { rid, namespaces },
                },
            ..
        } = &self.state
        {
            Some((rid, namespaces))
        } else {
            None
        }
    }

    pub fn ping(&mut self, reactor: &mut Reactor) -> Result<(), Error> {
        if let State::Connected { ping, .. } = &mut self.state {
            let msg = message::Ping::new(&mut self.rng);
            *ping = PingState::AwaitingResponse(msg.ponglen);

            reactor.write(self, Message::Ping(msg));
        }
        Ok(())
    }

    pub(crate) fn queue_fetch(&mut self, rid: Id) {
        self.pending_fetches.push_back(rid);
    }

    pub(crate) fn dequeue_fetch(&mut self) -> Option<Id> {
        self.pending_fetches.pop_front()
    }
}

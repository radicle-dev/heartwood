use crate::service::chan;
use crate::service::message;
use crate::service::message::Message;
use crate::service::{storage, FetchResult};
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

/// Session protocol.
#[derive(Debug, Default, Clone)]
pub enum Protocol {
    /// The default message-based gossip protocol.
    #[default]
    Gossip,
    /// Git smart protocol. Used for fetching repository data.
    /// This protocol is used after a connection upgrade via the
    /// [`Message::Fetch`] message.
    Fetch {
        /// Channel to send fetch results on. Set to `Some` when the fetch
        /// is locally initiated. Otherwise, no results need to be communicated
        /// back.
        results: Option<chan::Sender<FetchResult>>,
    },
}

#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum State {
    /// Initial state for outgoing connections.
    Connecting,
    /// Initial state after handshake protocol hand-off.
    Connected {
        /// Whether this session was initialized with a [`Message::Initialize`].
        initialized: bool,
        /// Connected since this time.
        since: LocalTime,
        /// Ping state.
        ping: PingState,
        /// Session protocol.
        protocol: Protocol,
    },
    /// When a peer is disconnected.
    Disconnected { since: LocalTime },
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

    /// Connection attempts. For persistent peers, Tracks
    /// how many times we've attempted to connect. We reset this to zero
    /// upon successful connection.
    attempts: usize,

    /// Source of entropy.
    rng: Rng,
}

impl Session {
    pub fn connecting(id: NodeId, persistent: bool, rng: Rng) -> Self {
        Self {
            id,
            state: State::Connecting,
            link: Link::Outbound,
            subscribe: None,
            persistent,
            last_active: LocalTime::default(),
            attempts: 0,
            rng,
        }
    }

    pub fn connected(id: NodeId, link: Link, persistent: bool, rng: Rng, time: LocalTime) -> Self {
        Self {
            id,
            state: State::Connected {
                initialized: false,
                since: time,
                ping: PingState::default(),
                protocol: Protocol::default(),
            },
            link,
            subscribe: None,
            persistent,
            last_active: LocalTime::default(),
            attempts: 0,
            rng,
        }
    }

    pub fn is_connecting(&self) -> bool {
        matches!(self.state, State::Connecting { .. })
    }

    pub fn is_connected(&self) -> bool {
        matches!(self.state, State::Connected { .. })
    }

    pub fn attempts(&self) -> usize {
        self.attempts
    }

    pub fn attempted(&mut self) {
        self.attempts += 1;
    }

    pub fn fetch(&mut self, repo: Id, results: chan::Sender<FetchResult>) -> Option<Message> {
        if let State::Connected { protocol, .. } = &mut self.state {
            if let Protocol::Gossip = protocol {
                *protocol = Protocol::Fetch {
                    results: Some(results),
                };
                return Some(Message::Fetch { repo });
            } else {
                log::error!(
                    "Attempted to upgrade protocol for {} which was already upgraded",
                    self.id
                );
            }
        }
        None
    }

    pub fn to_connected(&mut self, since: LocalTime) {
        assert!(
            self.is_connecting(),
            "Can only transition to 'connected' state from 'connecting' state"
        );
        self.attempts = 0;
        self.state = State::Connected {
            initialized: false,
            since,
            ping: PingState::default(),
            protocol: Protocol::default(),
        };
    }

    pub fn to_disconnected(&mut self, since: LocalTime) {
        self.state = State::Disconnected { since };
    }

    pub fn ping(&mut self, reactor: &mut Reactor) -> Result<(), Error> {
        if let State::Connected { ping, .. } = &mut self.state {
            let msg = message::Ping::new(&mut self.rng);
            *ping = PingState::AwaitingResponse(msg.ponglen);

            reactor.write(self.id, Message::Ping(msg));
        }
        Ok(())
    }
}

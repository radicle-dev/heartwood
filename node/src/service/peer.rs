use crate::service::message::*;
use crate::service::*;

#[derive(Debug, Default)]
#[allow(clippy::large_enum_variant)]
pub enum PeerState {
    /// Initial peer state. For outgoing peers this
    /// means we've attempted a connection. For incoming
    /// peers, this means they've successfully connected
    /// to us.
    #[default]
    Initial,
    /// State after successful handshake.
    Negotiated {
        /// The peer's unique identifier.
        id: NodeId,
        since: LocalTime,
        /// Addresses this peer is reachable on.
        addrs: Vec<Address>,
        git: Url,
    },
    /// When a peer is disconnected.
    Disconnected { since: LocalTime },
}

#[derive(thiserror::Error, Debug, Clone)]
pub enum PeerError {
    #[error("wrong network constant in message: {0}")]
    WrongMagic(u32),
    #[error("wrong protocol version in message: {0}")]
    WrongVersion(u32),
    #[error("invalid inventory timestamp: {0}")]
    InvalidTimestamp(u64),
    #[error("peer misbehaved")]
    Misbehavior,
}

#[derive(Debug)]
pub struct Peer {
    /// Peer address.
    pub addr: net::SocketAddr,
    /// Connection direction.
    pub link: Link,
    /// Whether we should attempt to re-connect
    /// to this peer upon disconnection.
    pub persistent: bool,
    /// Peer connection state.
    pub state: PeerState,
    /// Last known peer time.
    pub timestamp: Timestamp,
    /// Peer subscription.
    pub subscribe: Option<Subscribe>,

    /// Connection attempts. For persistent peers, Tracks
    /// how many times we've attempted to connect. We reset this to zero
    /// upon successful connection.
    attempts: usize,
}

impl Peer {
    pub fn new(addr: net::SocketAddr, link: Link, persistent: bool) -> Self {
        Self {
            addr,
            state: PeerState::default(),
            link,
            timestamp: Timestamp::default(),
            subscribe: None,
            persistent,
            attempts: 0,
        }
    }

    pub fn ip(&self) -> IpAddr {
        self.addr.ip()
    }

    pub fn is_negotiated(&self) -> bool {
        matches!(self.state, PeerState::Negotiated { .. })
    }

    pub fn attempts(&self) -> usize {
        self.attempts
    }

    pub fn attempted(&mut self) {
        self.attempts += 1;
    }

    pub fn connected(&mut self) {
        self.attempts = 0;
    }

    pub fn received<'r, S, T, G>(
        &mut self,
        envelope: Envelope,
        ctx: &mut Context<S, T, G>,
    ) -> Result<Option<Message>, PeerError>
    where
        T: storage::WriteStorage<'r>,
        G: crypto::Signer,
    {
        if envelope.magic != ctx.config.network.magic() {
            return Err(PeerError::WrongMagic(envelope.magic));
        }
        debug!("Received {:?} from {}", &envelope.msg, self.ip());

        match (&self.state, envelope.msg) {
            (
                PeerState::Initial,
                Message::Initialize {
                    id,
                    timestamp,
                    version,
                    addrs,
                    git,
                },
            ) => {
                let now = ctx.timestamp();

                if timestamp.abs_diff(now) > MAX_TIME_DELTA.as_secs() {
                    return Err(PeerError::InvalidTimestamp(timestamp));
                }
                if version != PROTOCOL_VERSION {
                    return Err(PeerError::WrongVersion(version));
                }
                // Nb. This is a very primitive handshake. Eventually we should have anyhow
                // extra "acknowledgment" message sent when the `Initialize` is well received.
                if self.link.is_inbound() {
                    ctx.write_all(self.addr, ctx.handshake_messages());
                }
                // Nb. we don't set the peer timestamp here, since it is going to be
                // set after the first message is received only. Setting it here would
                // mean that messages received right after the handshake could be ignored.
                self.state = PeerState::Negotiated {
                    id,
                    since: ctx.clock.local_time(),
                    addrs,
                    git,
                };
            }
            (PeerState::Initial, _) => {
                debug!(
                    "Disconnecting peer {} for sending us a message before handshake",
                    self.ip()
                );
                return Err(PeerError::Misbehavior);
            }
            (
                PeerState::Negotiated { git, .. },
                Message::InventoryAnnouncement {
                    node,
                    message,
                    signature,
                },
            ) => {
                let now = ctx.clock.local_time();
                let last = self.timestamp;

                // Don't allow messages from too far in the past or future.
                if message.timestamp.abs_diff(now.as_secs()) > MAX_TIME_DELTA.as_secs() {
                    return Err(PeerError::InvalidTimestamp(message.timestamp));
                }
                // Discard inventory messages we've already seen, otherwise update
                // out last seen time.
                if message.timestamp > last {
                    self.timestamp = message.timestamp;
                } else {
                    return Ok(None);
                }
                ctx.process_inventory(&message.inventory, node, git);

                if ctx.config.relay {
                    return Ok(Some(Message::InventoryAnnouncement {
                        node,
                        message,
                        signature,
                    }));
                }
            }
            // Process a peer inventory update announcement by (maybe) fetching.
            (
                PeerState::Negotiated { git, .. },
                Message::RefsAnnouncement {
                    node,
                    message,
                    signature,
                },
            ) => {
                if message.verify(&node, &signature) {
                    // TODO: Buffer/throttle fetches.
                    // TODO: Check that we're tracking this user as well.
                    if ctx.config.is_tracking(&message.id) {
                        // TODO: Check refs to see if we should try to fetch or not.
                        let updated_refs = ctx.fetch(&message.id, git);
                        let is_updated = !updated_refs.is_empty();

                        ctx.io.push_back(Io::Event(Event::RefsFetched {
                            from: git.clone(),
                            project: message.id.clone(),
                            updated: updated_refs,
                        }));

                        if is_updated {
                            return Ok(Some(Message::RefsAnnouncement {
                                node,
                                message,
                                signature,
                            }));
                        }
                    }
                } else {
                    return Err(PeerError::Misbehavior);
                }
            }
            (
                PeerState::Negotiated { .. },
                Message::NodeAnnouncement {
                    node,
                    message,
                    signature,
                },
            ) => {
                if !message.verify(&node, &signature) {
                    return Err(PeerError::Misbehavior);
                }
                log::warn!("Node announcement handling is not implemented");
            }
            (PeerState::Negotiated { .. }, Message::Subscribe(subscribe)) => {
                self.subscribe = Some(subscribe);
            }
            (PeerState::Negotiated { .. }, Message::Initialize { .. }) => {
                debug!(
                    "Disconnecting peer {} for sending us a redundant handshake message",
                    self.ip()
                );
                return Err(PeerError::Misbehavior);
            }
            (PeerState::Disconnected { .. }, msg) => {
                debug!("Ignoring {:?} from disconnected peer {}", msg, self.ip());
            }
        }
        Ok(None)
    }
}

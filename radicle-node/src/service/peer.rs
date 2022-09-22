use crate::service::message::*;
use crate::service::*;

#[derive(Debug, Default)]
#[allow(clippy::large_enum_variant)]
pub enum SessionState {
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
pub enum SessionError {
    #[error("wrong network constant in message: {0}")]
    WrongMagic(u32),
    #[error("wrong protocol version in message: {0}")]
    WrongVersion(u32),
    #[error("invalid announcement timestamp: {0}")]
    InvalidTimestamp(u64),
    #[error("peer misbehaved")]
    Misbehavior,
}

/// A peer session. Each connected peer will have one session.
#[derive(Debug)]
pub struct Session {
    /// Peer address.
    pub addr: net::SocketAddr,
    /// Connection direction.
    pub link: Link,
    /// Whether we should attempt to re-connect
    /// to this peer upon disconnection.
    pub persistent: bool,
    /// Peer connection state.
    pub state: SessionState,
    /// Peer subscription.
    pub subscribe: Option<Subscribe>,

    /// Connection attempts. For persistent peers, Tracks
    /// how many times we've attempted to connect. We reset this to zero
    /// upon successful connection.
    attempts: usize,
}

impl Session {
    pub fn new(addr: net::SocketAddr, link: Link, persistent: bool) -> Self {
        Self {
            addr,
            state: SessionState::default(),
            link,
            subscribe: None,
            persistent,
            attempts: 0,
        }
    }

    pub fn ip(&self) -> IpAddr {
        self.addr.ip()
    }

    pub fn is_negotiated(&self) -> bool {
        matches!(self.state, SessionState::Negotiated { .. })
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
    ) -> Result<Option<Message>, SessionError>
    where
        T: storage::WriteStorage<'r>,
        G: crypto::Signer,
    {
        if envelope.magic != ctx.config.network.magic() {
            return Err(SessionError::WrongMagic(envelope.magic));
        }
        debug!("Received {:?} from {}", &envelope.msg, self.ip());

        match (&self.state, envelope.msg) {
            (
                SessionState::Initial,
                Message::Initialize {
                    id,
                    version,
                    addrs,
                    git,
                },
            ) => {
                if version != PROTOCOL_VERSION {
                    return Err(SessionError::WrongVersion(version));
                }
                // Nb. This is a very primitive handshake. Eventually we should have anyhow
                // extra "acknowledgment" message sent when the `Initialize` is well received.
                if self.link.is_inbound() {
                    ctx.write_all(self.addr, ctx.handshake_messages());
                }
                // Nb. we don't set the peer timestamp here, since it is going to be
                // set after the first message is received only. Setting it here would
                // mean that messages received right after the handshake could be ignored.
                self.state = SessionState::Negotiated {
                    id,
                    since: ctx.clock.local_time(),
                    addrs,
                    git,
                };
            }
            (SessionState::Initial, _) => {
                debug!(
                    "Disconnecting peer {} for sending us a message before handshake",
                    self.ip()
                );
                return Err(SessionError::Misbehavior);
            }
            (
                SessionState::Negotiated { git, .. },
                Message::InventoryAnnouncement {
                    node,
                    message,
                    signature,
                },
            ) => {
                let now = ctx.clock.local_time();
                let peer = ctx.peers.entry(node).or_insert_with(Peer::default);

                // Don't allow messages from too far in the future.
                if message.timestamp.saturating_sub(now.as_secs()) > MAX_TIME_DELTA.as_secs() {
                    return Err(SessionError::InvalidTimestamp(message.timestamp));
                }
                // Discard inventory messages we've already seen, otherwise update
                // out last seen time.
                if message.timestamp > peer.last_message {
                    peer.last_message = message.timestamp;
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
                SessionState::Negotiated { git, .. },
                Message::RefsAnnouncement {
                    node,
                    message,
                    signature,
                },
            ) => {
                // FIXME: Check message timestamp.

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
                    return Err(SessionError::Misbehavior);
                }
            }
            (
                SessionState::Negotiated { .. },
                Message::NodeAnnouncement {
                    node,
                    message,
                    signature,
                },
            ) => {
                // FIXME: Check message timestamp.

                if !message.verify(&node, &signature) {
                    return Err(SessionError::Misbehavior);
                }
                log::warn!("Node announcement handling is not implemented");
            }
            (SessionState::Negotiated { .. }, Message::Subscribe(subscribe)) => {
                self.subscribe = Some(subscribe);
            }
            (SessionState::Negotiated { .. }, Message::Initialize { .. }) => {
                debug!(
                    "Disconnecting peer {} for sending us a redundant handshake message",
                    self.ip()
                );
                return Err(SessionError::Misbehavior);
            }
            (SessionState::Disconnected { .. }, msg) => {
                debug!("Ignoring {:?} from disconnected peer {}", msg, self.ip());
            }
        }
        Ok(None)
    }
}

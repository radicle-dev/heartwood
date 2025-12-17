pub mod command;
pub mod event;

use std::net::IpAddr;

use localtime::{LocalDuration, LocalTime};
use radicle::node::config::RateLimit;
use radicle::node::{address, Severity};
use radicle::node::{HostName, Link, NodeId};
use radicle::prelude::RepoId;

use crate::connections::session;
use crate::connections::session::Sessions;
use crate::connections::Config;
use crate::service::limiter::RateLimiter;
use crate::service::{message, DisconnectReason};

use super::Attempts;

/// Manage the state of node connections for a running node.
///
/// Note the following terminology:
///
/// - Outbound connection is one that is originating from this node to another node.
/// - Inbound connection is one that is coming from another node to this node.
///
/// These [`Sessions`] are categorized into one of the four following states.
///
/// # Initial
///
/// - [`Connections::connect`]
/// - [`Connections::reconnect`]
///
/// A connection is in the initial state when the running node has attempted to
/// make an outbound connection to another node.
///
/// It can also be in an initial state when a disconnected node is being
/// reconnected, and thus goes back to the initial state.
///
/// # Attempted
///
/// - [`Connections::attempted`]
///
/// A connection is in the attempted state when it was previously in the initial
/// state, and an attempt to make a connection was made.
///
/// # Connected
///
/// - [`Connections::connected`]
///
/// A connection is considered connected in one of two cases.
///
/// The attempted outbound connection was established. In this case, there must
/// have been a session to transition to being connected.
///
/// If the connection is inbound then the connection is simply marked as
/// connected, regardless of the state of a previous connection.
///
/// # Disconnected
///
/// - [`Connections::disconnected`]
///
/// A connection is marked as disconnected only if it is considered a persisted
/// peer (see [`ConnectionType`]). If this is the case, then a reconnection
/// attempt should be made after an appropriate delay.
///
/// If the connection is not considered for persistence, then it will be removed
/// from the [`Sessions`], and may be penalized for the severity of its
/// disconnection reason.
///
/// [`ConnectionType`]: session::ConnectionType
#[derive(Debug)]
pub struct Connections {
    /// [`NodeId`] of the running node.
    local: NodeId,
    /// The state of the connection lifecycle for each node in the network.
    sessions: Sessions,
    /// Rate limiter of IP hosts.
    limiter: RateLimiter,
    /// Configuration for managing connections.
    config: Config,
}

impl Connections {
    /// Construct a new [`Connections`] with the provided [`Config`] and [`RateLimiter`].
    ///
    /// The state will start with no [`Sessions`], to begin.
    pub fn new(local: NodeId, config: Config, limiter: RateLimiter) -> Self {
        Self {
            local,
            sessions: Sessions::default(),
            limiter,
            config,
        }
    }

    /// Return the [`Config`] the [`Connections`] were initialized with.
    pub fn config(&self) -> &Config {
        &self.config
    }
}

impl Connections {
    // TODO(finto): we could enforce that only an accepted `IpAddr` is allowed
    // for calling to `connected` – which also helps reinforce that they are
    // interconnected.
    /// Perform checks on whether an incoming IP address should be accepted for
    /// connecting to.
    ///
    /// The caller can decide based on the resulting [`event::Accept`] whether
    /// to accept the connection. However, the following events are recommended
    /// to result in a rejected address:
    /// - [`event::Accept::LimitExceeded`]
    /// - [`event::Accept::HostLimited`]
    ///
    /// # State Transition
    ///
    /// This does not transition any session states, and simply inspects the
    /// rate limiter and [`IpAddr`] properties.
    pub fn accept(
        &mut self,
        command::Accept { ip }: command::Accept,
        now: LocalTime,
    ) -> event::Accept {
        // Always accept localhost connections, even if we already reached
        // our inbound connection limit.
        if ip.is_loopback() || ip.is_unspecified() {
            return event::Accept::LocalHost { ip };
        }

        if self.has_reached_inbound_limit() {
            return event::Accept::LimitExceeded {
                ip,
                current_inbound: self.sessions.connected_inbound(),
            };
        }

        if self.has_reached_ip_limit(&ip, now) {
            return event::Accept::HostLimited { ip };
        }

        event::Accept::Accepted { ip }
    }

    /// Mark a connection, with the given node, as attempted.
    ///
    /// # State Transition
    ///
    /// Transitions the state of the existing session to `Attempted`.
    pub fn attempted(&mut self, command::Attempt { node }: command::Attempt) -> event::Attempted {
        if let Some(event) =
            self.guard_self_session(&node, event::Attempted::SelfConnection { node })
        {
            return event;
        }
        self.sessions
            .session_to_attempted(&node)
            .map(event::Attempted::attempt)
            .unwrap_or(event::Attempted::missing(node))
    }

    /// Make an outbound connection to another node.
    ///
    /// # State Transition
    ///
    /// A new session will only be created for the given node if a session does
    /// already exist.
    pub fn connect(
        &mut self,
        command::Connect {
            node,
            addr,
            connection_type,
        }: command::Connect,
        now: LocalTime,
    ) -> event::Connect {
        if self.is_disconnected(&node) {
            return event::Connect::disconnected(node);
        }
        if let Some(event) = self.guard_self_session(&node, event::Connect::SelfConnection { node })
        {
            return event;
        }
        if self.is_connecting(&node) {
            return event::Connect::already_connecting(node);
        }
        match self.sessions.get_connected(&node) {
            Some(session) => event::Connect::already_connected(session.clone()),
            None => {
                let record_ip = match addr.host {
                    HostName::Ip(ip) => (!address::is_local(&ip)).then_some(ip),
                    _ => None,
                };
                self.sessions.outbound(node, addr, connection_type, now);
                event::Connect::establish(node, connection_type, record_ip)
            }
        }
    }

    /// Mark a connection as connected to another node.
    ///
    /// # State Transition
    ///
    /// The transition of the connection depends on the kind of the incoming
    /// connection.
    ///
    /// ## Inbound
    ///
    /// The connection transitions to connected regardless of what state of the
    /// session was in before and if the session did not exist.
    ///
    /// If the session existed, before the transition, then it is marked as
    /// inbound.
    ///
    /// ## Outbound
    ///
    /// The connection transitions to connected regardless of what state of the
    /// session was in before, however, it must have had an existing session before.
    pub fn connected(&mut self, connected: command::Connected, now: LocalTime) -> event::Connected {
        match connected {
            command::Connected::Inbound {
                node,
                addr,
                connection_type,
            } => {
                if let Some(event) =
                    self.guard_self_session(&node, event::Connected::SelfConnection { node })
                {
                    return event;
                }
                // In this scenario, it's possible that our peer is persistent, and
                // disconnected. We get an inbound connection before we attempt a re-connection,
                // and therefore we treat it as a regular inbound connection.
                //
                // It's also possible that a disconnection hasn't gone through yet and our
                // peer is still in connected state here, while a new inbound connection from
                // that same peer is made. This results in a new connection from a peer that is
                // already connected from the perspective of the service. This appears to be
                // a bug in the underlying networking library.
                match self.sessions.session_to_connected(
                    &node,
                    now,
                    Some(Link::Inbound),
                    connection_type,
                ) {
                    None => {
                        let session = self.sessions.inbound(node, addr, connection_type, now);
                        event::Connected::established(session)
                    }
                    Some(session) => event::Connected::established(session),
                }
            }
            // TODO(finto): why was the address never used? Or did I miss something
            command::Connected::Outbound {
                node,
                addr: _,
                connection_type,
            } => {
                if let Some(event) =
                    self.guard_self_session(&node, event::Connected::SelfConnection { node })
                {
                    return event;
                }
                // Transitions the session to connected no matter what state it is in
                match self.sessions.session_to_connected(
                    &node,
                    now,
                    Some(Link::Outbound),
                    connection_type,
                ) {
                    None => event::Connected::missing(node),
                    Some(session) => event::Connected::established(session),
                }
            }
        }
    }

    /// Disconnect a node.
    ///
    /// # State Transition
    ///
    /// The [`ConnectionType`] decides how a disconnected node should be
    /// treated.
    ///
    /// ## `Ephemeral`
    ///
    /// If the connection is ephemeral, then the session for that connection is
    /// removed, and the severity of the reason is recorded.
    ///
    /// The severity can then be used for penalizing a node.
    ///
    /// ## `Persistent`
    ///
    /// If the connection is persistent, then the session will remain, and be
    /// marked as disconnected. The connection should then be retried after the
    /// returned delay.
    ///
    /// [`ConnectionType`]: session::ConnectionType
    pub fn disconnected(
        &mut self,
        command::Disconnect {
            node,
            link,
            since,
            connection_type,
        }: command::Disconnect,
        reason: &DisconnectReason,
    ) -> event::Disconnected {
        if let Some(event) =
            self.guard_self_session(&node, event::Disconnected::SelfConnection { node })
        {
            return event;
        }
        let Some(session) = self.sessions.get_session(&node) else {
            return event::Disconnected::missing(node);
        };
        if matches!(session.state(), session::State::Disconnected(_)) {
            return event::Disconnected::already_disconnected(node);
        }
        if *session.link() != link {
            return event::Disconnected::conflict(&session, link);
        }

        match connection_type {
            session::ConnectionType::Ephemeral => {
                let severity = self.reason_severity(reason, since);
                self.sessions
                    .remove_session(&node)
                    .map(|session| event::Disconnected::severed(session, severity))
                    .unwrap_or(event::Disconnected::missing(node))
            }
            session::ConnectionType::Persistent => {
                let delay = self.reconnection_delay(session.attempts());
                let retry_at = since + delay;
                self.sessions
                    .session_to_disconnected(&node, since, retry_at)
                    .map(|session| event::Disconnected::retry(session, delay, retry_at))
                    .unwrap_or(event::Disconnected::missing(node))
            }
        }
    }

    /// Reconnect the node.
    ///
    /// # State Transition
    ///
    /// The session must be in the disconnected state, and transitions to the
    /// initial state.
    pub fn reconnect(
        &mut self,
        command::Reconnect { node }: command::Reconnect,
    ) -> event::Reconnect {
        if let Some(event) =
            self.guard_self_session(&node, event::Reconnect::SelfConnection { node })
        {
            return event;
        }
        self.sessions
            .session_to_initial(&node)
            .map(event::Reconnect::reconnecting)
            .unwrap_or(event::Reconnect::missing(node))
    }

    /// Mark connected nodes as stable.
    ///
    /// If the connected session has lasted longer than the configured stable
    /// threshold duration, then the session will be marked as stable, and the
    /// attempts counter is reset.
    ///
    /// # State Transition
    ///
    /// This does not change the session's state.
    pub fn stabilise(&mut self, now: LocalTime) -> Vec<session::Session<session::Connected>> {
        self.sessions
            .connected_mut()
            .sessions()
            .fold(Vec::new(), |mut stabilised, session| {
                // Only stabilise sessions that are not already marked as stable
                if !session.is_stable() {
                    let stable = session
                        .stabilise(now, self.config.stale())
                        .then_some(session.clone());
                    stabilised.extend(stable);
                    stabilised
                } else {
                    stabilised
                }
            })
    }

    /// Ping any inactive connections to see if they are alive.
    ///
    /// # State Transition
    ///
    /// This does not change the sessions' state.
    pub fn ping<'a>(
        &'a mut self,
        mut ping: impl FnMut() -> message::Ping + 'a,
        now: LocalTime,
    ) -> impl Iterator<Item = event::Ping> + 'a {
        let keep_alive = self.config.keep_alive();
        self.sessions
            .inactive(now, keep_alive)
            .map(move |(_, session)| event::Ping {
                session: session.clone(),
                ping: session.ping(ping(), now),
            })
    }

    /// Process a incoming message from a node.
    ///
    /// The [`Payload`] of the message may alter the state of the session.
    ///
    /// If the node is marked as disconnected, then the message is dropped from
    /// affecting the node's session.
    ///
    /// # State Transition
    ///
    /// Since a message must come from a connected node, the session will
    /// transition from its initial or attempted state to connected.
    ///
    /// [`Payload`]: command::Payload
    pub fn handle_message(
        &mut self,
        command::Message {
            node,
            payload,
            connection_type,
        }: command::Message,
        now: LocalTime,
    ) -> event::HandledMessage {
        if let Some(event) =
            self.guard_self_session(&node, event::HandledMessage::SelfConnection { node })
        {
            return event;
        }
        if self.sessions.is_diconnected(&node) {
            return event::HandledMessage::Disconnected { node };
        }
        let outbound_limit = self.config.outbound.rate_limit;
        let inbound_limit = self.config.inbound.rate_limit;
        let result =
            self.sessions
                .while_connecting(&node, None, connection_type, now, |connected| {
                    let limit: RateLimit = match connected.link() {
                        Link::Outbound => outbound_limit,
                        Link::Inbound => inbound_limit,
                    };
                    if self.limiter.limit(
                        connected.address().clone().into(),
                        Some(&connected.node()),
                        &limit,
                        now,
                    ) {
                        return event::HandledMessage::RateLimited { node };
                    }
                    match payload {
                        Some(command::Payload::Subscribe(subscription)) => {
                            connected.set_subscription(subscription);
                            event::HandledMessage::Subscribed {
                                session: connected.clone(),
                            }
                        }
                        Some(command::Payload::Pong(pong)) => {
                            let pinged = connected.pinged(pong);
                            event::HandledMessage::Pinged {
                                session: connected.clone(),
                                pinged,
                            }
                        }
                        None => event::HandledMessage::Connected {
                            session: connected.clone(),
                        },
                    }
                });
        result.unwrap_or(event::HandledMessage::MissingSession { node })
    }

    /// Add a repository to the given node's subscription.
    ///
    /// Returns `true` if the session existed and the repository was added to
    /// the subscription successfully.
    pub fn subscribe_to(&mut self, node: &NodeId, rid: &RepoId) -> session::SubscribeTo {
        self.sessions.subscribe_to(node, rid)
    }
}

impl Connections {
    /// The [`Sessions`] that are currently being managed.
    pub fn sessions(&self) -> &Sessions {
        &self.sessions
    }

    /// Returns `true` is the session exists for the given node.
    pub fn has_session(&self, node: &NodeId) -> bool {
        self.sessions.has_session_for(node)
    }

    /// Returns the number of outbound connections that are in a "connecting"
    /// state. That is, they are either attempting to connect or have already
    /// connected.
    pub fn number_of_outbound_connections(&self) -> usize {
        self.sessions.number_of_outbound_connections()
    }

    /// Returns the number of inbound connections that are in a "connecting"
    /// state. That is, they are either attempting to connect or have already
    /// connected.
    pub fn number_of_inbound_connections(&self) -> usize {
        self.sessions.number_of_inbound_connections()
    }

    /// Return the [`Session`] for the given [`NodeId`], if it exists.
    /// Note that the session can be in any [`State`].
    ///
    /// [`Session`]: session::Session
    /// [`State`]: session::State
    pub fn session_for(&self, node: &NodeId) -> Option<session::Session<session::State>> {
        self.sessions.get_session(node)
    }

    /// Return the connected [`Session`] for the given [`NodeId`], if it exists.
    ///
    /// [`Session`]: session::Session
    pub fn get_connected(&self, node: &NodeId) -> Option<&session::Session<session::Connected>> {
        self.sessions.get_connected(node)
    }

    /// Return an `Iterator` of all unresponsive, connected [`Session`]s.
    ///
    /// A session is considered unresponsive, if it has be inactive after the
    /// configured stale connection duration.
    ///
    /// [`Session`]: session::Session
    pub fn unresponsive(
        &self,
        now: &LocalTime,
    ) -> impl Iterator<Item = (&NodeId, &session::Session<session::Connected>)> {
        self.sessions.unresponsive(*now, self.config.stale())
    }

    fn guard_self_session<T>(&self, node: &NodeId, event: T) -> Option<T> {
        (&self.local == node).then_some(event)
    }

    fn has_reached_inbound_limit(&self) -> bool {
        self.sessions.connected_inbound() >= self.config.inbound.maximum
    }

    fn has_reached_ip_limit(&mut self, ip: &IpAddr, now: LocalTime) -> bool {
        let addr = HostName::from(*ip);
        self.limiter
            .limit(addr, None, &self.config.inbound.rate_limit, now)
    }

    fn reason_severity(&self, reason: &DisconnectReason, now: LocalTime) -> Severity {
        match reason {
            DisconnectReason::Dial(_)
            | DisconnectReason::Fetch(_)
            | DisconnectReason::Connection(_) => {
                if self.is_online(now) {
                    // If we're "online", there's something wrong with this
                    // peer connection specifically.
                    Severity::Medium
                } else {
                    Severity::Low
                }
            }
            DisconnectReason::Session(e) => e.severity(),
            DisconnectReason::Command
            | DisconnectReason::Conflict
            | DisconnectReason::SelfConnection => Severity::Low,
        }
    }

    /// Try to guess whether we're online or not.
    fn is_online(&self, now: LocalTime) -> bool {
        self.sessions
            .connected()
            .sessions()
            .filter(|s| s.address().is_routable() && *s.last_active() >= now - self.idle())
            .count()
            > 0
    }

    fn idle(&self) -> LocalDuration {
        self.config.idle()
    }

    fn reconnection_delay(&self, attempts: Attempts) -> LocalDuration {
        let attempts = u32::try_from(usize::from(attempts)).unwrap_or(u32::MAX);
        LocalDuration::from_secs(2u64.saturating_pow(attempts)).clamp(
            self.config.reconnection_delay().min_delta,
            self.config.reconnection_delay().max_delta,
        )
    }

    fn is_connecting(&self, node: &NodeId) -> bool {
        self.sessions.is_initial(node) || self.sessions.is_attempted(node)
    }

    fn is_disconnected(&self, node: &NodeId) -> bool {
        self.sessions.is_diconnected(node)
    }
}

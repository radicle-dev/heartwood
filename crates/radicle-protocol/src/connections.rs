// TODO(finto): command should be something else, perhaps `input`?
pub mod command;
pub use command::{Connect, Disconnect};

pub mod commands;
pub mod effects;
pub mod events;

use radicle::node::config::RateLimits;
use radicle::prelude::RepoId;
use session::{HasAttempts as _, Session, Sessions};
mod session;

use std::collections::HashSet;
use std::net::IpAddr;

use localtime::{LocalDuration, LocalTime};
use radicle::node::address;
use radicle::node::{Address, HostName, Link, NodeId, Severity};

use crate::service::limiter::RateLimiter;
use crate::service::{message, DisconnectReason};

/// Minimum amount of time to wait before reconnecting to a peer.
pub const MIN_RECONNECTION_DELTA: LocalDuration = LocalDuration::from_secs(3);
/// Maximum amount of time to wait before reconnecting to a peer.
pub const MAX_RECONNECTION_DELTA: LocalDuration = LocalDuration::from_mins(60);

pub struct Connections {
    /// The state of the connection lifecycle for each node in the network.
    sessions: Sessions,
    /// Keep track of which node connections are meant to be persistent.
    persistent: HashSet<NodeId>,
    /// Keep track of banned IP addresses.
    banned: HashSet<IpAddr>,
    /// Rate limiter of IP hosts.
    limiter: RateLimiter,
    /// Configuration for managing connections.
    config: Config,
}

pub struct Config {
    /// Duration for a connection to be considered idle.
    idle: LocalDuration,
    /// Allowed number of inbound connections
    inbound_limit: usize,
    limits: RateLimits,
    reconnection_delay: ReconnectionDelay,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReconnectionDelay {
    /// The minimum amount of time to wait before attempting a re-connection.
    pub min_delta: LocalDuration,
    /// The maximum amount of time to wait before attempting a re-connection.
    pub max_delta: LocalDuration,
}

impl Default for ReconnectionDelay {
    fn default() -> Self {
        Self {
            min_delta: MIN_RECONNECTION_DELTA,
            max_delta: MAX_RECONNECTION_DELTA,
        }
    }
}

pub enum CommandEvent {
    MissingSession {
        node: NodeId,
    },
    Attempted(Session<session::Attempted>),
    Connected(Session<session::Connected>),
    Disconnected(Session<session::Disconnected>),
    Subscribed {
        node: NodeId,
        subscription: message::Subscribe,
    },
    SubscribedTo {
        node: NodeId,
        rid: RepoId,
    },
}

impl Connections {
    pub fn handle_command(&mut self, command: commands::Command) -> CommandEvent {
        match command {
            commands::Command::Attempt(attempt) => self.attempted(attempt),
            commands::Command::Connect(connect) => self.connected(connect),
            commands::Command::Disconnect(disconnect) => self.disconnected(disconnect),
            commands::Command::Subscribe(subscribe) => self.subscribed(subscribe),
            commands::Command::SubscribeTo(subscribe) => self.subscribed_to(subscribe),
        }
    }

    fn attempted(&mut self, commands::Attempt { node }: commands::Attempt) -> CommandEvent {
        self.sessions
            .session_to_attempted(&node)
            .map(CommandEvent::Attempted)
            .unwrap_or(CommandEvent::MissingSession { node })
    }

    fn connected(
        &mut self,
        commands::Connect {
            node,
            now,
            link,
            persistent,
        }: commands::Connect,
    ) -> CommandEvent {
        self.sessions
            .session_to_connected(&node, now, link, persistent)
            .map(CommandEvent::Connected)
            .unwrap_or(CommandEvent::MissingSession { node })
    }

    fn disconnected(
        &mut self,
        commands::Disconnect {
            node,
            since,
            retry_at,
        }: commands::Disconnect,
    ) -> CommandEvent {
        self.sessions
            .session_to_disconnected(&node, since, retry_at)
            .map(CommandEvent::Disconnected)
            .unwrap_or(CommandEvent::MissingSession { node })
    }

    fn subscribed(
        &mut self,
        commands::Subscribe { node, subscription }: commands::Subscribe,
    ) -> CommandEvent {
        if self.sessions.subscribe(&node, subscription.clone()) {
            CommandEvent::Subscribed { node, subscription }
        } else {
            CommandEvent::MissingSession { node }
        }
    }

    fn subscribed_to(
        &mut self,
        commands::SubscribeTo { node, rid }: commands::SubscribeTo,
    ) -> CommandEvent {
        if self.sessions.subscribe_to(&node, &rid) {
            CommandEvent::SubscribedTo { node, rid }
        } else {
            CommandEvent::MissingSession { node }
        }
    }
}

impl Connections {
    pub fn sessions(&self) -> &Sessions {
        &self.sessions
    }

    pub fn accept(&mut self, ip: IpAddr, now: LocalTime) -> AcceptResult {
        let mut result = AcceptResult::default();
        // Always accept localhost connections, even if we already reached
        // our inbound connection limit.
        if ip.is_loopback() || ip.is_unspecified() {
            result.local_host(ip);
            return result;
        }

        if self.has_reached_inbound_limit() {
            result.inbound_limit_exceeded(ip, self.sessions.connected_inbound());
            return result;
        }

        if self.is_ip_banned(&ip) {
            result.ip_banned(ip);
            return result;
        }

        if self.has_reached_ip_limit(&ip, now) {
            result.host_limited(ip);
            return result;
        }

        result.accepted(ip);
        result
    }

    pub fn connect(&self, connect: Connect) -> ConnectResult {
        let mut result = ConnectResult::default();
        match connect {
            Connect::Inbound(command::Inbound {
                node,
                clock,
                persistent,
            }) => match self.sessions.get_connected(&node) {
                Some(session) => result.already_connected(session.clone(), Link::Inbound),
                None => {
                    result.connect(node, clock, Link::Inbound, persistent);
                    result.send_initial_messages(node, Link::Inbound);
                }
            },
            Connect::Outbound(command::Outbound {
                node,
                addr,
                persistent,
                clock,
            }) => match self.sessions.get_connected(&node) {
                Some(session) => {
                    result.already_connected(session.clone(), Link::Outbound);
                    result.send_initial_messages(node, *session.link());
                }
                None => {
                    if let HostName::Ip(ip) = addr.host {
                        if !address::is_local(&ip) {
                            result.record_ip(node, ip, clock);
                        }
                    }
                    result.connect(node, clock, Link::Outbound, persistent);
                    result.send_initial_messages(node, Link::Outbound);
                }
            },
        }
        result
    }

    pub fn disconnect(&self, disconnect: Disconnect) -> DisconnectResult {
        let mut result = DisconnectResult::default();
        let Disconnect {
            node,
            link,
            reason,
            since,
        } = disconnect;
        let is_persistent = self.is_persistent(&node);
        let Some(session) = self.sessions.get_session(&node) else {
            result.already_disconnected(node);
            return result;
        };
        if *session.link() != link {
            result.link_conflict(node, *session.link(), link);
            return result;
        }

        if is_persistent {
            let delay = self.reconnection_delay(session.attempts().as_u32());
            let retry_at = since + delay;
            result.retry_connection(node, since, retry_at);
            result.disconnect(node, since, Some(retry_at));
        } else {
            let severity = self.reason_severity(reason, since);
            result.record_severity(node, session.address().clone(), severity);
            result.disconnect(node, since, None);
            if link.is_outbound() {
                result.maintain_connnections();
            }
        }
        result
    }

    fn reason_severity(&self, reason: DisconnectReason, now: LocalTime) -> Severity {
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
            .filter(|(_, s)| s.address().is_routable() && *s.last_active() >= now - self.idle())
            .count()
            > 0
    }

    fn idle(&self) -> LocalDuration {
        self.config.idle
    }

    fn is_persistent(&self, node: &NodeId) -> bool {
        self.persistent.contains(node)
    }

    fn is_ip_banned(&self, ip: &IpAddr) -> bool {
        self.banned.contains(ip)
    }

    // TODO: limit is harshing my buzz by taking &mut self here
    fn has_reached_ip_limit(&mut self, ip: &IpAddr, now: LocalTime) -> bool {
        let addr = HostName::from(*ip);
        self.limiter
            .limit(addr, None, &self.config.limits.inbound, now)
    }

    fn has_reached_inbound_limit(&self) -> bool {
        self.sessions.connected_inbound() >= self.config.inbound_limit
    }

    fn reconnection_delay(&self, attempts: u32) -> LocalDuration {
        LocalDuration::from_secs(2u64.pow(attempts)).clamp(
            self.config.reconnection_delay.min_delta,
            self.config.reconnection_delay.max_delta,
        )
    }
}

#[derive(Debug, Default)]
pub struct AcceptResult {
    pub effects: Vec<effects::Accept>,
    pub events: Vec<events::Accept>,
}

impl AcceptResult {
    fn local_host(&mut self, ip: IpAddr) {
        self.effects.push(effects::Accept::LocalHost { ip });
    }

    fn inbound_limit_exceeded(&mut self, ip: IpAddr, connected_inbound: usize) {
        self.events.push(events::Accept::LimitExceeded {
            ip,
            current_inbound: connected_inbound,
        });
    }

    fn ip_banned(&mut self, ip: IpAddr) {
        self.events.push(events::Accept::IpBanned { ip })
    }

    fn host_limited(&mut self, ip: IpAddr) {
        self.events.push(events::Accept::HostLimited { ip })
    }

    fn accepted(&mut self, ip: IpAddr) {
        self.effects.push(effects::Accept::Accepted { ip })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConnectResult {
    pub events: Vec<events::Connect>,
    pub effects: Vec<effects::Connect>,
    pub commands: Vec<commands::Connect>,
}

impl ConnectResult {
    fn already_connected(&mut self, session: Session<session::Connected>, attempted_link: Link) {
        self.events.push(events::Connect::AlreadyConnected {
            session,
            attempted_link,
        });
    }

    fn send_initial_messages(&mut self, node: NodeId, link: Link) {
        self.effects
            .push(effects::Connect::SendInitialMessages { node, link });
    }

    fn record_ip(&mut self, node: NodeId, ip: IpAddr, clock: LocalTime) {
        self.effects
            .push(effects::Connect::RecordIp { node, ip, clock });
    }

    fn connect(&mut self, node: NodeId, clock: LocalTime, link: Link, persistent: bool) {
        self.commands.push(commands::Connect {
            node,
            now: clock,
            link,
            persistent,
        });
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DisconnectResult {
    pub events: Vec<events::Disconnect>,
    pub effects: Vec<effects::Disconnect>,
    pub commands: Vec<commands::Disconnect>,
}

impl DisconnectResult {
    fn already_disconnected(&mut self, node: NodeId) {
        self.events
            .push(events::Disconnect::AlreadyDisconnected { node });
    }

    fn link_conflict(&mut self, node: NodeId, found: Link, expected: Link) {
        self.events.push(events::Disconnect::LinkConflict {
            node,
            found,
            expected,
        });
    }

    fn retry_connection(&mut self, node: NodeId, since: LocalTime, retry_at: LocalTime) {
        self.effects.push(effects::Disconnect::RetryConnection {
            node,
            since,
            retry_at,
        });
    }

    fn maintain_connnections(&mut self) {
        self.effects.push(effects::Disconnect::MaintainConnections);
    }

    fn record_severity(&mut self, node: NodeId, address: Address, severity: Severity) {
        self.effects.push(effects::Disconnect::RecordServerity {
            node,
            address,
            severity,
        })
    }

    fn disconnect(&mut self, node: NodeId, since: LocalTime, retry_at: Option<LocalTime>) {
        self.commands.push(commands::Disconnect {
            node,
            since,
            retry_at,
        });
    }
}

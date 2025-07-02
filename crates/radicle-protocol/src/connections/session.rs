//! State management for node connection states.
//!
//! # Session
//!
//! The main type to describe a node connection is [`Session`]. It has a single
//! generic parameter that describes the current state the session is in, which
//! is one of:
//!   - [`Initial`]
//!   - [`Attempted`]
//!   - [`Connected`]
//!   - [`Disconnected`]
//!
//! Or, if a collection of sessions in various states is required, then
//! [`State`] enumerates all of them.
//!
//! The are two main ways to construct a [`Session`]:
//! - [`Session::outbound`]
//! - [`Session::inbound`]
//!
//! # Sessions
//!
//! The [`Sessions`] type keeps track of what the current state a [`NodeId`] is
//! in, with its corresponding [`Session`].
//!
//! A given [`NodeId`] must only appear in one state at a given time, if a
//! session for it exists.

mod iter;
use iter::SessionsViewMut;
pub use iter::{SessionsIter, SessionsView};

use std::collections::{HashMap, VecDeque};
use std::fmt;

use localtime::{LocalDuration, LocalTime};
use radicle::node::{Address, Link, NodeId, PingState};
use radicle::prelude::RepoId;

use crate::service::{MAX_LATENCIES, ZeroBytes, message};

/// Enumeration of the various session states.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    Initial(Initial),
    Attempted(Attempted),
    Connected(Connected),
    Disconnected(Disconnected),
}

impl From<Initial> for State {
    fn from(value: Initial) -> Self {
        Self::Initial(value)
    }
}

impl From<Attempted> for State {
    fn from(value: Attempted) -> Self {
        Self::Attempted(value)
    }
}

impl From<Connected> for State {
    fn from(value: Connected) -> Self {
        Self::Connected(value)
    }
}

impl From<Disconnected> for State {
    fn from(value: Disconnected) -> Self {
        Self::Disconnected(value)
    }
}

impl From<State> for radicle::node::State {
    fn from(state: State) -> Self {
        match state {
            State::Initial(initial) => Self::from(initial),
            State::Attempted(attempted) => Self::from(attempted),
            State::Connected(connected) => Self::from(connected),
            State::Disconnected(disconnected) => Self::from(disconnected),
        }
    }
}

impl From<Initial> for radicle::node::State {
    fn from(_: Initial) -> Self {
        Self::Initial
    }
}

impl From<Attempted> for radicle::node::State {
    fn from(_: Attempted) -> Self {
        Self::Attempted
    }
}

impl From<Disconnected> for radicle::node::State {
    fn from(Disconnected { since, retry_at }: Disconnected) -> Self {
        Self::Disconnected { since, retry_at }
    }
}

impl From<Connected> for radicle::node::State {
    fn from(
        Connected {
            since,
            ping,
            latencies,
            stable,
        }: Connected,
    ) -> Self {
        Self::Connected {
            since,
            ping,
            latencies,
            stable,
        }
    }
}

/// Keeps track of multiple node sessions and their connection lifecycle.
///
/// Each node has one [`Session`], and can be in of the following states:
/// - [`Initial`]
/// - [`Attempted`]
/// - [`Connected`]
/// - [`Disconnected`]
///
/// # State Transitions
///
/// It is ensured that a given [`NodeId`] can only be in, at most, one state at
/// a given time.
///
/// A [`Session::outbound`] starts in the [`Initial`] state, and can then move
/// to [`Attempted`], [`Connected`], or [`Disconnected`].
///
/// A [`Session::inbound`] starts in the [`Connected`] state immediately – since
/// the connection was established by the incoming node. It can then move to
/// [`Disconnected`].
///
/// A [`Disconnected`] session can be reconnected to, which transitions it to
/// the [`Initial`] state, restarting the lifecycle.
#[derive(Debug, Default)]
pub struct Sessions {
    initial: HashMap<NodeId, Session<Initial>>,
    attempted: HashMap<NodeId, Session<Attempted>>,
    disconnected: HashMap<NodeId, Session<Disconnected>>,
    connected: HashMap<NodeId, Session<Connected>>,
}

impl<'a> IntoIterator for &'a Sessions {
    type Item = (&'a NodeId, Session<State>);
    type IntoIter = SessionsIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl Sessions {
    /// Construct a new [`Sessions`] state.
    pub fn new() -> Self {
        Self {
            initial: HashMap::new(),
            attempted: HashMap::new(),
            disconnected: HashMap::new(),
            connected: HashMap::new(),
        }
    }

    /// Get an iterator over all the sessions, see [`SessionsIter`] for more
    /// information.
    pub fn iter<'a>(&'a self) -> SessionsIter<'a> {
        SessionsIter {
            initial: self.initial.iter(),
            attempted: self.attempted.iter(),
            disconnected: self.disconnected.iter(),
            connected: self.connected.iter(),
        }
    }

    /// Get the number of sessions that are connected and have an [inbound]
    /// link.
    ///
    /// [inbound]: Link::Inbound
    pub fn connected_inbound(&self) -> usize {
        self.connected
            .values()
            .filter(|session| session.link().is_inbound())
            .count()
    }

    /// Get the number of sessions that are connected and have an [outbound]
    /// link.
    ///
    /// [outbound]: Link::Outbound
    pub fn connected_outbound(&self) -> usize {
        self.connected
            .values()
            .filter(|session| session.link().is_outbound())
            .count()
    }

    /// Checks that an existing [`Session`] exists for the given [`NodeId`].
    pub fn has_session_for(&self, node: &NodeId) -> bool {
        self.initial.contains_key(node)
            || self.attempted.contains_key(node)
            || self.disconnected.contains_key(node)
            || self.connected.contains_key(node)
    }

    /// Get all [`Session`]s that are in the [`Connected`] state, along with
    /// their [`NodeId`]s.
    pub fn connected(&self) -> SessionsView<'_, Connected> {
        SessionsView {
            inner: &self.connected,
        }
    }

    /// Get all [`Session`]s that are in the [`Initial`] state, along with
    /// their [`NodeId`]s.
    pub fn initial(&self) -> SessionsView<'_, Initial> {
        SessionsView {
            inner: &self.initial,
        }
    }

    /// Get all [`Session`]s that are in the [`Attempted`] state, along with
    /// their [`NodeId`]s.
    pub fn attempted(&self) -> SessionsView<'_, Attempted> {
        SessionsView {
            inner: &self.attempted,
        }
    }

    /// Get all [`Session`]s that are in the [`Disconnected`] state, along with
    /// their [`NodeId`]s.
    pub fn disconnected(&self) -> SessionsView<'_, Disconnected> {
        SessionsView {
            inner: &self.disconnected,
        }
    }

    /// Check if the given [`NodeId`] has a connected session.
    pub fn is_connected(&self, node: &NodeId) -> bool {
        self.connected.contains_key(node)
    }

    /// Check if the given [`NodeId`] has a disconnected session.
    pub fn is_disconnected(&self, node: &NodeId) -> bool {
        self.disconnected.contains_key(node)
    }

    /// Check if the given [`NodeId`] has an initial session.
    pub fn is_initial(&self, node: &NodeId) -> bool {
        self.initial.contains_key(node)
    }

    /// Check if the given [`NodeId`] has an attempted session.
    pub fn is_attempted(&self, node: &NodeId) -> bool {
        self.attempted.contains_key(node)
    }

    /// Get a [`Session`], for the given [`NodeId`], that can be in any [`State`].
    pub fn get_session(&self, node: &NodeId) -> Option<Session<State>> {
        self.initial
            .get(node)
            .cloned()
            .map(|s| s.into_any_state())
            .or_else(|| {
                self.attempted
                    .get(node)
                    .cloned()
                    .map(|s| s.into_any_state())
            })
            .or_else(|| {
                self.disconnected
                    .get(node)
                    .cloned()
                    .map(|s| s.into_any_state())
            })
            .or_else(|| {
                self.connected
                    .get(node)
                    .cloned()
                    .map(|s| s.into_any_state())
            })
    }

    /// Get the [`Session`], for the given [`NodeId`], that is expected to be in
    /// the [`Connected`] state.
    pub fn get_connected(&self, node: &NodeId) -> Option<&Session<Connected>> {
        self.connected.get(node)
    }

    /// Get a mutable iterator of the [`Sessions`]s that are in the
    /// [`Connected`] state, along with their [`NodeId`]s.
    pub(super) fn connected_mut(&mut self) -> SessionsViewMut<'_, Connected> {
        SessionsViewMut {
            inner: &mut self.connected,
        }
    }

    pub(super) fn unresponsive(
        &self,
        now: LocalTime,
        stale_connection: LocalDuration,
    ) -> impl Iterator<Item = (&NodeId, &Session<Connected>)> {
        self.connected()
            .into_iter()
            .filter(move |(_, session)| session.is_inactive(&now, stale_connection))
    }

    pub(super) fn inactive(
        &mut self,
        now: LocalTime,
        keep_alive: LocalDuration,
    ) -> impl Iterator<Item = (&NodeId, &mut Session<Connected>)> {
        self.connected_mut()
            .into_iter()
            .filter(move |(_, session)| session.is_inactive(&now, keep_alive))
    }

    /// Transition the [`Session`], identified by the [`NodeId`], to the [`Initial`] state.
    ///
    /// If the [`Session`] does not exist, then `None` is returned.
    ///
    /// This is used when reconnecting a disconnected session, that needs to be
    /// kept as a persistent connection.
    pub(super) fn session_to_initial(&mut self, node: &NodeId) -> Option<Session<Initial>> {
        let s = self.disconnected.remove(node)?.into_initial();
        self.initial.insert(*node, s.clone());
        Some(s)
    }

    /// Transition the [`Session`], identified by the [`NodeId`], to the
    /// [`Attempted`] state.
    ///
    /// If the [`Session`] does not exist, then `None` is returned.
    pub(super) fn session_to_attempted(&mut self, node: &NodeId) -> Option<Session<Attempted>> {
        let s = self.initial.remove(node)?.into_attempted();
        self.attempted.insert(*node, s.clone());
        Some(s)
    }

    /// Transition the [`Session`], identified by the [`NodeId`], to the
    /// [`Disconnected`] state.
    ///
    /// The time this [`Session`] was disconnected is marked by `since`, and if
    /// the connection should be retried then a `retry_at` value should be
    /// provided.
    ///
    /// If the [`Session`] does not exist, then `None` is returned.
    pub(super) fn session_to_disconnected(
        &mut self,
        node: &NodeId,
        since: LocalTime,
        retry_at: LocalTime,
    ) -> Option<Session<Disconnected>> {
        match self.remove_session(node) {
            None => None,
            Some(session) => {
                let s = session.into_disconnected(since, retry_at);
                self.disconnected.insert(*node, s.clone());
                Some(s)
            }
        }
    }

    /// Transition the [`Session`], identified by the [`NodeId`], to the
    /// [`Connected`] state.
    ///
    /// The [`Session`] is last active given by the time given for `now`, the
    /// type of [`Link`] is also marked by the provided value, and also keep
    /// track of whether the session should be persisted.
    ///
    /// If the [`Session`] does not exist, then `None` is returned.
    pub(super) fn session_to_connected(
        &mut self,
        node: &NodeId,
        now: LocalTime,
        link: Option<Link>,
        connection_type: ConnectionType,
    ) -> Option<Session<Connected>> {
        let s = self.remove_session(node)?;
        let link = link.unwrap_or(s.link);
        let state = match s.state {
            State::Connected(connected) => connected,
            State::Initial(_) | State::Attempted(_) | State::Disconnected(_) => Connected::new(now),
        };
        let s = Session {
            state,
            id: s.id,
            addr: s.addr,
            link,
            connection_type,
            last_active: now,
            subscription: s.subscription,
            attempts: s.attempts,
        };
        self.connected.insert(*node, s.clone());
        Some(s)
    }

    pub(super) fn subscribe_to(&mut self, node: &NodeId, rid: &RepoId) -> SubscribeTo {
        if let Some(session) = self.connected.get_mut(node) {
            return session.subscribe_to(rid);
        }

        if let Some(session) = self.disconnected.get_mut(node) {
            return session.subscribe_to(rid);
        }

        if let Some(session) = self.attempted.get_mut(node) {
            return session.subscribe_to(rid);
        }

        if let Some(session) = self.initial.get_mut(node) {
            return session.subscribe_to(rid);
        }

        SubscribeTo::Missing { node: *node }
    }

    pub(super) fn remove_session(&mut self, node: &NodeId) -> Option<Session<State>> {
        self.initial
            .remove(node)
            .map(|s| s.into_any_state())
            .or_else(|| self.attempted.remove(node).map(|s| s.into_any_state()))
            .or_else(|| self.disconnected.remove(node).map(|s| s.into_any_state()))
            .or_else(|| self.connected.remove(node).map(|s| s.into_any_state()))
    }

    pub(super) fn outbound(
        &mut self,
        node: NodeId,
        addr: Address,
        connection_type: ConnectionType,
        now: LocalTime,
    ) -> Session<Initial> {
        let session = Session::outbound(node, addr, connection_type, now);
        self.initial.insert(node, session.clone());
        session
    }

    pub(super) fn inbound(
        &mut self,
        node: NodeId,
        addr: Address,
        connection_type: ConnectionType,
        now: LocalTime,
    ) -> Session<Connected> {
        let session = Session::inbound(node, addr, connection_type, now);
        self.connected.insert(node, session.clone());
        session
    }

    pub(super) fn number_of_outbound_connections(&self) -> usize {
        let attempted = self
            .attempted
            .iter()
            .filter(|(_, s)| s.link.is_outbound())
            .count();
        let connected = self
            .connected
            .iter()
            .filter(|(_, s)| s.link.is_outbound())
            .count();
        attempted + connected
    }

    pub(super) fn number_of_inbound_connections(&self) -> usize {
        let attempted = self
            .attempted
            .iter()
            .filter(|(_, s)| s.link.is_inbound())
            .count();
        let connected = self
            .connected
            .iter()
            .filter(|(_, s)| s.link.is_inbound())
            .count();
        attempted + connected
    }

    pub(super) fn while_connecting<F, T>(
        &mut self,
        node: &NodeId,
        link: Option<Link>,
        connection_type: ConnectionType,
        now: LocalTime,
        f: F,
    ) -> Option<T>
    where
        F: FnOnce(&mut Session<Connected>) -> T,
    {
        let s = self.remove_session(node)?;
        let link = link.unwrap_or(s.link);
        let state = match s.state {
            State::Connected(connected) => connected,
            State::Initial(_) | State::Attempted(_) | State::Disconnected(_) => Connected::new(now),
        };
        let mut s = Session {
            id: s.id,
            addr: s.addr,
            link,
            connection_type,
            last_active: now,
            subscription: s.subscription,
            attempts: s.attempts,
            state,
        };
        let result = f(&mut s);
        self.connected.insert(*node, s.clone());
        Some(result)
    }
}

/// Number of attempts made for connecting to a node.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Attempts {
    attempts: usize,
}

impl fmt::Display for Attempts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.attempts)
    }
}

impl PartialEq<usize> for Attempts {
    fn eq(&self, other: &usize) -> bool {
        self.attempts == *other
    }
}

impl PartialOrd<usize> for Attempts {
    fn partial_cmp(&self, other: &usize) -> Option<std::cmp::Ordering> {
        self.attempts.partial_cmp(other)
    }
}

impl Attempts {
    pub fn attempted(self) -> Self {
        Self {
            attempts: self.attempts + 1,
        }
    }

    pub fn attempts(&self) -> usize {
        self.attempts
    }

    fn reset(&mut self) {
        self.attempts = 0;
    }
}

impl From<&Attempts> for usize {
    fn from(Attempts { attempts }: &Attempts) -> Self {
        *attempts
    }
}

impl From<Attempts> for usize {
    fn from(Attempts { attempts }: Attempts) -> Self {
        attempts
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session<S> {
    /// The [`NodeId`] of the session.
    id: NodeId,
    /// The public protocol [`Address`] for the session.
    addr: Address,
    /// The [`Link`] direction for the session.
    link: Link,
    /// Keep track of whether the session should be persisted. That is, if it is
    /// disconnected, reconnection attempts should be made.
    connection_type: ConnectionType,
    /// Last time a message was received from the peer.
    last_active: LocalTime,
    /// The peer's subscription containing the [`RepoId`]'s that this node is
    /// interested in.
    subscription: Option<message::Subscribe>,
    /// Number of attempts over the lifetime of the connection.
    ///
    /// The tracking of attempts is preserved through the state transitions of
    /// the session, and are reset to 0 when the connection is considered
    /// stable.
    attempts: Attempts,
    /// The state the session is in. Can be in the following states:
    ///   - [`Initial`]
    ///   - [`Attempted`]
    ///   - [`Disconnected`]
    ///   - [`Connected`]
    ///
    /// Or the enumeration of all of the above via [`State`].
    state: S,
}

/// A [`Session`] connection type describes how the session should be treated
/// when the session becomes disconnected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectionType {
    /// The connection is ephemeral and the session should be removed on
    /// disconnection.
    Ephemeral,
    /// The connection is persistent and the session should be marked as
    /// disconnected, and a reconnection attempt should be made.
    Persistent,
}

impl ConnectionType {
    fn as_str(&self) -> &'static str {
        match self {
            ConnectionType::Ephemeral => "ephemeral",
            ConnectionType::Persistent => "persistent",
        }
    }
}

impl fmt::Display for ConnectionType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The result of modifying a node's subscription.
pub enum SubscribeTo {
    /// No subscription has been set for the node yet.
    NoSubscription,
    /// The subscription was successful.
    Subscribed,
    /// The node was not found when attempting to modify the subscription.
    Missing { node: NodeId },
}

impl From<Session<State>> for radicle::node::Session {
    fn from(session: Session<State>) -> Self {
        Self {
            nid: session.id,
            link: session.link,
            addr: session.addr,
            state: session.state.into(),
        }
    }
}

impl<S> fmt::Display for Session<S>
where
    S: ToString,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut attrs = Vec::new();
        let state = self.state.to_string();

        if self.link.is_inbound() {
            attrs.push("inbound");
        } else {
            attrs.push("outbound");
        }
        attrs.push(self.connection_type.as_str());
        attrs.push(state.as_str());

        write!(f, "{} [{}]", self.id, attrs.join(" "))
    }
}

impl<S> Session<S> {
    /// Return the node's identifier.
    pub fn node(&self) -> NodeId {
        self.id
    }

    /// Return the state metadata of the session.
    pub fn state(&self) -> &S {
        &self.state
    }

    /// Return the number of attempts that have been made for connection.
    pub fn attempts(&self) -> Attempts {
        self.attempts
    }

    /// Return the [`Address`] of the node.
    pub fn address(&self) -> &Address {
        &self.addr
    }

    /// Returns `true` if the session is subscribed to the given [`RepoId`].
    pub fn is_subscribed_to(&self, rid: &RepoId) -> bool {
        self.subscription
            .as_ref()
            .map(|s| s.filter.contains(rid))
            .unwrap_or(false)
    }

    /// Returns when the session was last active.
    ///
    /// The last active time is updated when the connection performs some
    /// activity, e.g. receiving a message.
    pub fn last_active(&self) -> &LocalTime {
        &self.last_active
    }

    /// Return the type of [`Link`] for the session connection.
    pub fn link(&self) -> &Link {
        &self.link
    }

    /// Returns `true` if the session is a persistent connection.
    pub fn persistent(&self) -> bool {
        matches!(self.connection_type, ConnectionType::Persistent)
    }

    /// Set the [`message::Subscribe`] of this [`Session`].
    pub(super) fn set_subscription(&mut self, subscription: message::Subscribe) {
        self.subscription = Some(subscription);
    }

    /// Subscribe to the given [`RepoId`], if the [`message::Subscribe`] has
    /// been set.
    fn subscribe_to(&mut self, rid: &RepoId) -> SubscribeTo {
        if let Some(ref mut sub) = self.subscription {
            sub.filter.insert(rid);
            return SubscribeTo::Subscribed;
        }
        SubscribeTo::NoSubscription
    }

    fn into_disconnected(self, since: LocalTime, retry_at: LocalTime) -> Session<Disconnected> {
        self.transition(Disconnected { since, retry_at })
    }

    #[allow(unused)]
    fn seen(&mut self, since: LocalTime) {
        self.last_active = since;
    }

    fn into_any_state<T>(self) -> Session<T>
    where
        T: From<S>,
    {
        self.map(|state| state.into())
    }

    fn transition<T>(self, next: T) -> Session<T> {
        self.map(|_| next)
    }

    fn map<T, F>(self, f: F) -> Session<T>
    where
        F: FnOnce(S) -> T,
    {
        Session {
            id: self.id,
            addr: self.addr,
            link: self.link,
            connection_type: self.connection_type,
            last_active: self.last_active,
            subscription: self.subscription,
            attempts: self.attempts,
            state: f(self.state),
        }
    }
}

/// The session is in an initial state, with no extra metadata.
///
/// An initial state indicates that it is going to attempt a connection, whether
/// through a fresh connection or a reconnection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Initial;

impl Session<Initial> {
    /// Construct a [`Session`] with the [`Initial`] state, and a [`Link`] that
    /// is [`Outbound`].
    ///
    /// The session begins with no subscription, and no attempts made.
    ///
    /// [`Outbound`]: Link::Outbound
    pub fn outbound(
        id: NodeId,
        addr: Address,
        connection_type: ConnectionType,
        last_active: LocalTime,
    ) -> Self {
        Self {
            id,
            addr,
            link: Link::Outbound,
            connection_type,
            state: Initial,
            last_active,
            subscription: None,
            attempts: Attempts::default(),
        }
    }

    /// Transition the [`Session`] to an [`Attempted`] state, incrementing the
    /// number of attempts made.
    fn into_attempted(mut self) -> Session<Attempted> {
        self.attempts = self.attempts.attempted();
        self.transition(Attempted)
    }
}

/// The session is in an attempted state, with no extra metadata.
///
/// An attempted state indicates that at least one attempt was made to connect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Attempted;

/// The session is in an disconnected state.
///
/// A disconnected state indicates that the session was connected at one point,
/// and a reconnection should be made.
///
/// # Metadata
///
/// [`Session::is_stable`] reports when a connection is considered stable.
///
/// [`Session::is_inactive`] reports when a connection is considered inactive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Connected {
    /// Connected since this time.
    since: LocalTime,
    /// Ping state.
    ping: PingState,
    /// Measured latencies for this peer.
    latencies: VecDeque<LocalDuration>,
    /// Whether the connection is stable.
    stable: bool,
}

impl fmt::Display for Connected {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("connected")
    }
}

impl Connected {
    /// Create a new [`Connected`] state, where `since` is the time of
    /// connection.
    fn new(since: LocalTime) -> Self {
        Self {
            since,
            ping: PingState::default(),
            latencies: VecDeque::default(),
            stable: false,
        }
    }
}

/// A received pong message for a connected session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pong {
    pub now: LocalTime,
    pub zeroes: ZeroBytes,
}

/// The result of a connected session receiving a pong message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pinged {
    /// The recorded latency of the received pong.
    pub latency: LocalDuration,
}

impl Session<Connected> {
    /// Construct a [`Session`] with the [`Connected`] state, and a [`Link`] that
    /// is [`Inbound`].
    ///
    /// The session begins with no subscription, and no attempts made.
    ///
    /// [`Inbound`]: Link::Inbound
    pub fn inbound(
        id: NodeId,
        addr: Address,
        connection_type: ConnectionType,
        now: LocalTime,
    ) -> Self {
        Self {
            id,
            addr,
            link: Link::Inbound,
            connection_type,
            last_active: now,
            subscription: None,
            state: Connected::new(now),
            attempts: Attempts::default(),
        }
    }

    /// Returns true if the connection is considered stable.
    ///
    /// A stable connection is one which has a connected time that is before the
    /// current time, and the duration of the connection exceeds a configured
    /// threshold.
    pub fn is_stable(&self) -> bool {
        self.state.stable
    }

    /// Checks if the [`Session`] is inactive, i.e. the time passed is greater
    /// than the `delta`.
    pub fn is_inactive(&self, now: &LocalTime, delta: LocalDuration) -> bool {
        *now - self.last_active >= delta
    }

    /// A ping was sent to connected nodes, and this session is now awaiting a response.
    pub(super) fn ping(&mut self, ping: message::Ping, since: LocalTime) -> message::Ping {
        self.state.ping = PingState::AwaitingResponse {
            len: ping.ponglen,
            since,
        };
        ping
    }

    /// A pong was received from a connected node.
    ///
    /// The current session must be awaiting a response from a sent ping. If so,
    /// it checks that this is the corresponding pong, and records the latency
    /// between the sent ping and the received pong.
    pub(super) fn pinged(&mut self, Pong { zeroes, now }: Pong) -> Option<Pinged> {
        if let PingState::AwaitingResponse {
            len: ponglen,
            since,
        } = self.state.ping
            && (ponglen as usize) == zeroes.len()
        {
            self.state.ping = PingState::Ok;
            let latency = now - since;
            self.state.latencies.push_back(latency);
            // TODO(finto): MAX_LATENCIES should likely be configured
            // somewhere else
            if self.state.latencies.len() > MAX_LATENCIES {
                self.state.latencies.pop_front();
            }
            return Some(Pinged { latency });
        }
        None
    }

    /// Checks the idleness of a connection, marking its connectivity as stable,
    /// and reset its attempt counter.
    ///
    /// A stable connection is one which has a connected time that is before the
    /// current time, and the duration of the connection exceeds a configured
    /// threshold.
    pub(super) fn stabilise(&mut self, now: LocalTime, stable_threshold: LocalDuration) -> bool {
        let Connected {
            since,
            ref mut stable,
            ..
        } = self.state;
        if now >= since && now.duration_since(since) >= stable_threshold {
            *stable = true;
            self.attempts.reset();
            true
        } else {
            false
        }
    }
}

/// The session is in an disconnected state.
///
/// A disconnected state indicates that the session was connected at one point,
/// and a reconnection should be made.
///
/// # Metadata
///
/// [`Session::should_retry_at`] reports when a reconnection should occur.
///
/// [`Session::disconnected_since`] reports how long the session has been disconnected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Disconnected {
    /// Since when has this peer been disconnected.
    since: LocalTime,
    /// When to retry the connection.
    retry_at: LocalTime,
}

impl Session<Disconnected> {
    /// Returns when the session should attempt a reconnection.
    pub fn should_retry_at(&self) -> &LocalTime {
        &self.state.retry_at
    }

    /// Returns when the session was recorded as disconnected.
    pub fn disconnected_since(&self) -> &LocalTime {
        &self.state.since
    }

    /// Transition the [`Session`] to an [`Initial`] state.
    fn into_initial(self) -> Session<Initial> {
        self.transition(Initial)
    }
}

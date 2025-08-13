use std::collections::{HashMap, HashSet, VecDeque};

use localtime::{LocalDuration, LocalTime};
use radicle::{
    node::{Address, Link, NodeId, PingState},
    prelude::RepoId,
};

use crate::service::{message, ZeroBytes, MAX_LATENCIES};

/// Time after which a connection is considered stable.
#[allow(unused)]
pub const CONNECTION_STABLE_THRESHOLD: LocalDuration = LocalDuration::from_mins(1);

#[derive(Clone, Debug)]
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

impl HasAttempts for State {
    fn attempts(&self) -> Attempts {
        match self {
            State::Initial(initial) => initial.attempts,
            State::Attempted(attempted) => attempted.attempts,
            State::Connected(connected) => connected.attempts,
            State::Disconnected(disconnected) => disconnected.attempts,
        }
    }
}

/// Marker type for when a [`NodeId`] is missing from [`Sessions`].
pub struct Missing;

pub struct Sessions {
    initial: HashMap<NodeId, Session<Initial>>,
    attempted: HashMap<NodeId, Session<Attempted>>,
    disconnected: HashMap<NodeId, Session<Disconnected>>,
    connected: HashMap<NodeId, Session<Connected>>,
}

impl Sessions {
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
    pub fn connected(&self) -> impl Iterator<Item = (&NodeId, &Session<Connected>)> {
        self.connected.iter()
    }

    /// Transition the [`Session`], identified by the [`NodeId`], to the
    /// [`Attempted`] state.
    ///
    /// If the [`Session`] does not exist, then `None` is returned.
    pub fn session_to_attempted(&mut self, node: &NodeId) -> Option<Session<Attempted>> {
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
    pub fn session_to_disconnected(
        &mut self,
        node: &NodeId,
        since: LocalTime,
        retry_at: Option<LocalTime>,
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
    pub fn session_to_connected(
        &mut self,
        node: &NodeId,
        now: LocalTime,
        link: Link,
        persistent: bool,
    ) -> Option<Session<Connected>> {
        let s = self.remove_session(node)?;
        let state = match s.state {
            State::Initial(initial) => Connected::from_initial(initial, now),
            State::Attempted(attempted) => Connected::from_attempted(attempted, now),
            State::Connected(connected) => connected,
            State::Disconnected(disconnected) => Connected::from_disconnected(disconnected, now),
        };
        Some(Session {
            state,
            id: s.id,
            addr: s.addr,
            link,
            persistent,
            last_active: now,
            subscribe: s.subscribe,
        })
    }

    /// Transition a [`Disconnected`] [`Session`] into an [`Initial`] state,
    /// meaning that it should be re-connected to.
    ///
    /// If the [`NodeId`] was not in a [`Disconnected`] state then `None` is
    /// returned.
    pub fn reconnect(&mut self, node: &NodeId) -> Option<Session<Initial>> {
        let s = self.disconnected.remove(node)?.into_initial();
        self.initial.insert(*node, s.clone());
        Some(s)
    }

    /// Get a [`Session`] that can be in any [`State`].
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

    pub fn subscribe(&mut self, node: &NodeId, subscription: message::Subscribe) -> bool {
        if let Some(session) = self.connected.get_mut(node) {
            session.set_subscription(subscription);
            return true;
        }

        if let Some(session) = self.disconnected.get_mut(node) {
            session.set_subscription(subscription);
            return true;
        }

        if let Some(session) = self.attempted.get_mut(node) {
            session.set_subscription(subscription);
            return true;
        }

        if let Some(session) = self.initial.get_mut(node) {
            session.set_subscription(subscription);
            return true;
        }

        false
    }

    pub fn subscribe_to(&mut self, node: &NodeId, rid: &RepoId) -> bool {
        if let Some(session) = self.connected.get_mut(node) {
            session.subscribe_to(rid);
            return true;
        }

        if let Some(session) = self.disconnected.get_mut(node) {
            session.subscribe_to(rid);
            return true;
        }

        if let Some(session) = self.attempted.get_mut(node) {
            session.subscribe_to(rid);
            return true;
        }

        if let Some(session) = self.initial.get_mut(node) {
            session.subscribe_to(rid);
            return true;
        }

        false
    }

    pub fn pinged(&mut self, node: &NodeId, pong: Pong) -> Result<Option<Pinged>, Missing> {
        let session = self.connected.get_mut(node).ok_or(Missing)?;
        Ok(session.pinged(pong))
    }

    fn remove_session(&mut self, node: &NodeId) -> Option<Session<State>> {
        self.initial
            .remove(node)
            .map(|s| s.into_any_state())
            .or_else(|| self.attempted.remove(node).map(|s| s.into_any_state()))
            .or_else(|| self.disconnected.remove(node).map(|s| s.into_any_state()))
            .or_else(|| self.connected.remove(node).map(|s| s.into_any_state()))
    }

    /// Get the [`Session`], for the given [`NodeId`], that is expected to be in
    /// the [`Connected`] state.
    pub fn get_connected(&self, node: &NodeId) -> Option<&Session<Connected>> {
        self.connected.get(node)
    }

    #[allow(unused)]
    fn inbound(&mut self, node: NodeId, addr: Address, persistent: bool, now: LocalTime) {
        self.connected
            .insert(node, Session::inbound(node, addr, persistent, now));
    }
}

pub trait HasAttempts {
    fn attempts(&self) -> Attempts;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Attempts {
    /// Connection attempts. For persistent peers, Tracks
    /// how many times we've attempted to connect. We reset this to zero
    /// upon successful connection, once the connection is stable.
    attempts: usize,
}

impl Attempts {
    fn new(attempts: usize) -> Self {
        Attempts { attempts }
    }

    pub fn attempted(self) -> Self {
        Self {
            attempts: self.attempts + 1,
        }
    }

    pub fn reset(&mut self) {
        self.attempts = 0;
    }

    pub fn attempts(&self) -> usize {
        self.attempts
    }

    pub(super) fn as_u32(&self) -> u32 {
        self.attempts as u32
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
    /// disconnected, re-connection attempts should be made.
    persistent: bool,
    /// Last time a message was received from the peer.
    last_active: LocalTime,
    /// Peer subscription.
    subscribe: Option<message::Subscribe>,
    /// The state the session is in. Can be in the following states:
    ///   - [`Initial`]
    ///   - [`Attempted`]
    ///   - [`Disconnected`]
    ///   - [`Connected`]
    state: S,
}

impl<S: HasAttempts> HasAttempts for Session<S> {
    fn attempts(&self) -> Attempts {
        self.state.attempts()
    }
}

impl<S> Session<S> {
    pub fn node(&self) -> NodeId {
        self.id
    }

    pub fn address(&self) -> &Address {
        &self.addr
    }

    /// Set the [`message::Subscribe`] of this [`Session`].
    pub fn set_subscription(&mut self, subscription: message::Subscribe) {
        self.subscribe = Some(subscription);
    }

    /// Subscribe to the given [`RepoId`], if the [`message::Subscribe`] has
    /// been set.
    pub fn subscribe_to(&mut self, rid: &RepoId) {
        if let Some(ref mut sub) = self.subscribe {
            sub.filter.insert(rid);
        }
    }

    pub fn last_active(&self) -> &LocalTime {
        &self.last_active
    }

    pub fn link(&self) -> &Link {
        &self.link
    }

    pub fn as_outbound(&mut self) {
        self.link = Link::Outbound;
    }

    pub fn into_disconnected(
        self,
        since: LocalTime,
        retry_at: Option<LocalTime>,
    ) -> Session<Disconnected>
    where
        S: HasAttempts,
    {
        self.map(|s| Disconnected {
            since,
            retry_at,
            attempts: s.attempts(),
        })
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

    fn map<T, F>(self, f: F) -> Session<T>
    where
        F: FnOnce(S) -> T,
    {
        Session {
            id: self.id,
            addr: self.addr,
            link: self.link,
            persistent: self.persistent,
            last_active: self.last_active,
            subscribe: self.subscribe,
            state: f(self.state),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Initial {
    attempts: Attempts,
}

impl Initial {
    pub fn new() -> Self {
        Self::with_attempts(Attempts::new(1))
    }

    pub fn with_attempts(attempts: Attempts) -> Self {
        Self { attempts }
    }
}

impl Default for Initial {
    fn default() -> Self {
        Self::new()
    }
}

impl Session<Initial> {
    pub fn outbound(id: NodeId, addr: Address, persistent: bool, last_active: LocalTime) -> Self {
        Self {
            id,
            addr,
            link: Link::Outbound,
            persistent,
            state: Initial::new(),
            last_active,
            subscribe: None,
        }
    }

    /// Transition the [`Session`] to an [`Attempted`] state, incrementing the
    /// number of attempts made.
    pub fn into_attempted(self) -> Session<Attempted> {
        self.map(|s| Attempted::new(s.attempts.attempted()))
    }

    /// Transition the [`Session`] into the [`Connected`] state.
    pub fn into_connected(self, since: LocalTime) -> Session<Connected> {
        self.map(|s| Connected::new(since, s.attempts))
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Attempted {
    attempts: Attempts,
}

impl Attempted {
    pub fn new(attempts: Attempts) -> Self {
        Attempted { attempts }
    }
}

impl Session<Attempted> {
    /// Transition the [`Session`] into the [`Connected`] state.
    pub fn into_connected(self, since: LocalTime) -> Session<Connected> {
        self.map(|s| Connected::new(since, s.attempts))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Connected {
    /// Connected since this time.
    since: LocalTime,
    /// Ping state.
    ping: PingState,
    /// Ongoing fetches.
    fetching: HashSet<RepoId>,
    /// Measured latencies for this peer.
    latencies: VecDeque<LocalDuration>,
    /// Whether the connection is stable.
    stable: bool,
    /// Number of attempts over the lifetime of the connection. This includes if
    /// the connection is degraded back to an [`Initial`] state through a
    /// [`Session::reconnect`].
    attempts: Attempts,
}

impl HasAttempts for Connected {
    fn attempts(&self) -> Attempts {
        self.attempts
    }
}

impl Connected {
    /// Create a new [`Connected`] state, where `since` is the time of
    /// connection, and `attempts` is the number of attempted connections in the
    /// lifetime of the [`Session`].
    pub fn new(since: LocalTime, attempts: Attempts) -> Self {
        Self {
            since,
            ping: PingState::default(),
            fetching: HashSet::default(),
            latencies: VecDeque::default(),
            stable: false,
            attempts,
        }
    }

    /// Create a fresh [`Connected`] state, using `since` as the [`LocalTime`] for
    /// when this connection was made.
    pub fn fresh(since: LocalTime) -> Self {
        Self::new(since, Attempts::new(0))
    }

    fn from_initial(initial: Initial, since: LocalTime) -> Self {
        Self::new(since, initial.attempts)
    }

    fn from_attempted(attempted: Attempted, since: LocalTime) -> Self {
        Self::new(since, attempted.attempts)
    }

    fn from_disconnected(disconnected: Disconnected, since: LocalTime) -> Self {
        Self::new(since, disconnected.attempts)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ping {
    pub since: LocalTime,
    pub rng: fastrand::Rng,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pong {
    pub now: LocalTime,
    pub zeroes: ZeroBytes,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pinged {
    pub latency: LocalDuration,
}

impl Session<Connected> {
    pub fn inbound(id: NodeId, addr: Address, persistent: bool, now: LocalTime) -> Self {
        Self {
            id,
            addr,
            link: Link::Inbound,
            persistent,
            last_active: now,
            subscribe: None,
            state: Connected::fresh(now),
        }
    }

    /// Checks if the [`Session`] is inactive, i.e. the time passed is greater
    /// than the `delta`.
    pub fn is_inactive(&self, now: &LocalTime, delta: LocalDuration) -> bool {
        *now - self.last_active >= delta
    }

    pub fn ping(&mut self, mut ping: Ping) -> message::Ping {
        let msg = message::Ping::new(&mut ping.rng);
        self.state.ping = PingState::AwaitingResponse {
            len: msg.ponglen,
            since: ping.since,
        };
        msg
    }

    pub fn pinged(&mut self, Pong { zeroes, now }: Pong) -> Option<Pinged> {
        if let PingState::AwaitingResponse {
            len: ponglen,
            since,
        } = self.state.ping
        {
            if (ponglen as usize) == zeroes.len() {
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
        }
        None
    }

    pub fn idle(&mut self, now: LocalTime, stable_threshold: LocalDuration) {
        let Connected {
            since,
            ref mut stable,
            ref mut attempts,
            ..
        } = self.state;
        if now >= since && now.duration_since(since) >= stable_threshold {
            *stable = true;
            attempts.reset();
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Disconnected {
    /// Since when has this peer been disconnected.
    since: LocalTime,
    /// When to retry the connection.
    retry_at: Option<LocalTime>,
    /// Number of attempts while disconnected.
    attempts: Attempts,
}

impl Session<Disconnected> {
    pub fn disconnected_since(&self) -> &LocalTime {
        &self.state.since
    }

    pub fn should_retry_at(&self) -> Option<&LocalTime> {
        self.state.retry_at.as_ref()
    }

    /// Transition the [`Session`] to an [`Initial`] state.
    fn into_initial(self) -> Session<Initial> {
        self.map(|s| Initial::with_attempts(s.attempts))
    }
}

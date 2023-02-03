//! A simple P2P network simulator. Acts as the _reactor_, but without doing any I/O.
#![allow(clippy::collapsible_if)]
#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut, Range};
use std::rc::Rc;
use std::sync::Arc;
use std::{fmt, io};

use localtime::{LocalDuration, LocalTime};
use log::*;

use crate::crypto::Signer;
use crate::git::raw as git;
use crate::node::{FetchError, FetchResult};
use crate::prelude::Address;
use crate::service::reactor::Io;
use crate::service::{DisconnectReason, Event, Message, NodeId};
use crate::storage::{Namespaces, RefUpdate};
use crate::storage::{WriteRepository, WriteStorage};
use crate::test::peer::Service;
use crate::Link;

/// Minimum latency between peers.
pub const MIN_LATENCY: LocalDuration = LocalDuration::from_millis(1);
/// Maximum number of events buffered per peer.
pub const MAX_EVENTS: usize = 2048;

/// A simulated peer. Service instances have to be wrapped in this type to be simulated.
pub trait Peer<S, G>:
    Deref<Target = Service<S, G>> + DerefMut<Target = Service<S, G>> + 'static
{
    /// Initialize the peer. This should at minimum initialize the service with the
    /// current time.
    fn init(&mut self);
    /// Get the peer address.
    fn addr(&self) -> Address;
    /// Get the peer id.
    fn id(&self) -> NodeId;
}

/// Simulated service input.
#[derive(Debug, Clone)]
pub enum Input {
    /// Connection attempt underway.
    Connecting {
        /// Remote peer id.
        id: NodeId,
        /// Address used to connect.
        addr: Address,
    },
    /// New connection with a peer.
    Connected {
        /// Remote peer id.
        id: NodeId,
        /// Link direction.
        link: Link,
    },
    /// Disconnected from peer.
    Disconnected(NodeId, Rc<DisconnectReason>),
    /// Received a message from a remote peer.
    Received(NodeId, Vec<Message>),
    /// Fetch completed for a node.
    Fetched(Arc<FetchResult>),
    /// Used to advance the state machine after some wall time has passed.
    Wake,
}

/// A scheduled service input.
#[derive(Debug, Clone)]
pub struct Scheduled {
    /// The node for which this input is scheduled.
    pub node: NodeId,
    /// The remote peer from which this input originates.
    /// If the input originates from the local node, this should be set to the zero address.
    pub remote: NodeId,
    /// The input being scheduled.
    pub input: Input,
}

impl fmt::Display for Scheduled {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.input {
            Input::Received(from, msgs) => {
                for msg in msgs {
                    write!(f, "{} <- {} ({:?})", self.node, from, msg)?;
                }
                Ok(())
            }
            Input::Connected {
                id: addr,
                link: Link::Inbound,
                ..
            } => write!(f, "{} <== {}: Connected", self.node, addr),
            Input::Connected {
                id: addr,
                link: Link::Outbound,
                ..
            } => write!(f, "{} ==> {}: Connected", self.node, addr),
            Input::Connecting { id, .. } => {
                write!(f, "{} => {}: Connecting", self.node, id)
            }
            Input::Disconnected(addr, reason) => {
                write!(f, "{} =/= {}: Disconnected: {}", self.node, addr, reason)
            }
            Input::Wake => {
                write!(f, "{}: Tock", self.node)
            }
            Input::Fetched(result) => {
                write!(
                    f,
                    "{} <~ {} ({}): FetchCompleted",
                    self.node, result.remote, result.rid
                )
            }
        }
    }
}

/// Inbox of scheduled state machine inputs to be delivered to the simulated nodes.
#[derive(Debug)]
pub struct Inbox {
    /// The set of scheduled inputs. We use a `BTreeMap` to ensure inputs are always
    /// ordered by scheduled delivery time.
    messages: BTreeMap<LocalTime, Scheduled>,
}

impl Inbox {
    /// Add a scheduled input to the inbox.
    fn insert(&mut self, mut time: LocalTime, msg: Scheduled) {
        // Make sure we don't overwrite an existing message by using the same time slot.
        while self.messages.contains_key(&time) {
            time = time + MIN_LATENCY;
        }
        self.messages.insert(time, msg);
    }

    /// Get the next scheduled input to be delivered.
    fn next(&mut self) -> Option<(LocalTime, Scheduled)> {
        self.messages
            .iter()
            .next()
            .map(|(time, scheduled)| (*time, scheduled.clone()))
    }

    /// Get the last message sent between two peers. Only checks one direction.
    fn last(&self, node: &NodeId, remote: &NodeId) -> Option<(&LocalTime, &Scheduled)> {
        self.messages
            .iter()
            .rev()
            .find(|(_, v)| &v.node == node && &v.remote == remote)
    }
}

/// Simulation options.
#[derive(Debug, Clone)]
pub struct Options {
    /// Minimum and maximum latency between nodes, in seconds.
    pub latency: Range<u64>,
    /// Probability that network I/O fails.
    /// A rate of `1.0` means 100% of I/O fails.
    pub failure_rate: f64,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            latency: Range::default(),
            failure_rate: 0.,
        }
    }
}

/// A peer-to-peer node simulation.
pub struct Simulation<S, G> {
    /// Inbox of inputs to be delivered by the simulation.
    inbox: Inbox,
    /// Events emitted during simulation.
    events: BTreeMap<NodeId, VecDeque<Event>>,
    /// Priority events that should happen immediately.
    priority: VecDeque<Scheduled>,
    /// Simulated latencies between nodes.
    latencies: BTreeMap<(NodeId, NodeId), LocalDuration>,
    /// Network partitions between two nodes.
    partitions: BTreeSet<(NodeId, NodeId)>,
    /// Set of existing connections between nodes.
    connections: BTreeSet<(NodeId, NodeId)>,
    /// Set of connection attempts.
    attempts: BTreeSet<(NodeId, NodeId)>,
    /// Simulation options.
    opts: Options,
    /// Start time of simulation.
    start_time: LocalTime,
    /// Current simulation time. Updated when a scheduled message is processed.
    time: LocalTime,
    /// RNG.
    rng: fastrand::Rng,
    /// Storage type.
    storage: PhantomData<S>,
    /// Signer type.
    signer: PhantomData<G>,
}

impl<S: WriteStorage + 'static, G: Signer> Simulation<S, G> {
    /// Create a new simulation.
    pub fn new(time: LocalTime, rng: fastrand::Rng, opts: Options) -> Self {
        Self {
            inbox: Inbox {
                messages: BTreeMap::new(),
            },
            events: BTreeMap::new(),
            priority: VecDeque::new(),
            partitions: BTreeSet::new(),
            latencies: BTreeMap::new(),
            connections: BTreeSet::new(),
            attempts: BTreeSet::new(),
            opts,
            start_time: time,
            time,
            rng,
            storage: PhantomData,
            signer: PhantomData,
        }
    }

    /// Check whether the simulation is done, ie. there are no more messages to process.
    pub fn is_done(&self) -> bool {
        self.inbox.messages.is_empty()
    }

    /// Total amount of simulated time elapsed.
    #[allow(dead_code)]
    pub fn elapsed(&self) -> LocalDuration {
        self.time - self.start_time
    }

    /// Check whether the simulation has settled, ie. the only messages left to process
    /// are (periodic) timeouts.
    pub fn is_settled(&self) -> bool {
        self.inbox
            .messages
            .iter()
            .all(|(_, s)| matches!(s.input, Input::Wake))
    }

    /// Get a node's emitted events.
    pub fn events(&mut self, node: &NodeId) -> impl Iterator<Item = Event> + '_ {
        self.events.entry(*node).or_default().drain(..)
    }

    /// Get the latency between two nodes. The minimum latency between nodes is 1 millisecond.
    pub fn latency(&self, from: NodeId, to: NodeId) -> LocalDuration {
        self.latencies
            .get(&(from, to))
            .cloned()
            .map(|l| {
                if l <= MIN_LATENCY {
                    l
                } else {
                    // Create variance in the latency. The resulting latency
                    // will be between half, and two times the base latency.
                    let millis = l.as_millis();

                    if self.rng.bool() {
                        // More latency.
                        LocalDuration::from_millis(millis + self.rng.u128(0..millis))
                    } else {
                        // Less latency.
                        LocalDuration::from_millis(millis - self.rng.u128(0..millis / 2))
                    }
                }
            })
            .unwrap_or_else(|| MIN_LATENCY)
    }

    /// Initialize peers.
    pub fn initialize<'a, P>(self, peers: impl IntoIterator<Item = &'a mut P>) -> Self
    where
        P: Peer<S, G>,
    {
        for peer in peers.into_iter() {
            peer.init();
        }
        self
    }

    /// Run the simulation while the given predicate holds.
    pub fn run_while<'a, P>(
        &mut self,
        peers: impl IntoIterator<Item = &'a mut P>,
        pred: impl Fn(&Self) -> bool,
    ) where
        P: Peer<S, G>,
    {
        let mut nodes: BTreeMap<_, _> = peers.into_iter().map(|p| (p.id(), p)).collect();

        while self.step_(&mut nodes) {
            if !pred(self) {
                break;
            }
        }
    }

    /// Process one scheduled input from the inbox, using the provided peers.
    /// This function should be called until it returns `false`, or some desired state is reached.
    /// Returns `true` if there are more messages to process.
    pub fn step<'a, P: Peer<S, G>>(&mut self, peers: impl IntoIterator<Item = &'a mut P>) -> bool {
        let mut nodes: BTreeMap<_, _> = peers.into_iter().map(|p| (p.id(), p)).collect();
        self.step_(&mut nodes)
    }

    fn step_<P: Peer<S, G>>(&mut self, nodes: &mut BTreeMap<NodeId, &mut P>) -> bool {
        if !self.opts.latency.is_empty() {
            // Configure latencies.
            for (i, from) in nodes.keys().enumerate() {
                for to in nodes.keys().skip(i + 1) {
                    let range = self.opts.latency.clone();
                    let latency = LocalDuration::from_millis(
                        self.rng
                            .u128(range.start as u128 * 1_000..range.end as u128 * 1_000),
                    );

                    self.latencies.entry((*from, *to)).or_insert(latency);
                    self.latencies.entry((*to, *from)).or_insert(latency);
                }
            }
        }

        // Create and heal partitions.
        // TODO: These aren't really "network" partitions, as they are only
        // between individual nodes. We need to think about more realistic
        // scenarios. We should also think about creating various network
        // topologies.
        if self.time.as_secs() % 10 == 0 {
            for (i, x) in nodes.keys().enumerate() {
                for y in nodes.keys().skip(i + 1) {
                    if self.is_fallible() {
                        self.partitions.insert((*x, *y));
                    } else {
                        self.partitions.remove(&(*x, *y));
                    }
                }
            }
        }

        // Schedule any messages in the pipes.
        for peer in nodes.values_mut() {
            let id = peer.id();

            for o in peer.by_ref() {
                self.schedule(&id, o);
            }
        }
        // Next high-priority message.
        let priority = self.priority.pop_front().map(|s| (self.time, s));

        if let Some((time, next)) = priority.or_else(|| self.inbox.next()) {
            let elapsed = (time - self.start_time).as_millis();
            if matches!(next.input, Input::Wake) {
                trace!(target: "sim", "{:05} {}", elapsed, next);
            } else {
                // TODO: This can be confusing, since this event may not actually be passed to
                // the service. It would be best to only log the events that are being sent
                // to the service, or to log when an input is being dropped.
                info!(target: "sim", "{:05} {} ({})", elapsed, next, self.inbox.messages.len());
            }
            assert!(time >= self.time, "Time only moves forwards!");

            self.time = time;
            self.inbox.messages.remove(&time);

            let Scheduled { input, node, .. } = next;

            if let Some(ref mut p) = nodes.get_mut(&node) {
                p.tick(time);

                match input {
                    Input::Connecting { id, addr } => {
                        if self.attempts.insert((node, id)) {
                            // TODO: Also call `inbound` for inbound attempts.
                            p.attempted(id, &addr);
                        }
                    }
                    Input::Connected { id, link } => {
                        let conn = (node, id);

                        let attempted = link.is_outbound() && self.attempts.remove(&conn);
                        if attempted || link.is_inbound() {
                            if self.connections.insert(conn) {
                                p.connected(id, link);
                            }
                        }
                    }
                    Input::Disconnected(id, reason) => {
                        let conn = (node, id);
                        let attempt = self.attempts.remove(&conn);
                        let connection = self.connections.remove(&conn);

                        // Can't be both attempting and connected.
                        assert!(!(attempt && connection));

                        if attempt || connection {
                            p.disconnected(id, &reason);
                        }
                    }
                    Input::Wake => p.wake(),
                    Input::Received(id, msgs) => {
                        for msg in msgs {
                            p.received_message(id, msg);
                        }
                    }
                    Input::Fetched(result) => {
                        let result = Arc::try_unwrap(result).unwrap();
                        let mut repo = p.storage().repository(result.rid).unwrap();

                        fetch(&mut repo, &result.remote, result.namespaces.clone()).unwrap();
                        p.fetched(result);
                    }
                }
                for o in p.by_ref() {
                    self.schedule(&node, o);
                }
            } else {
                panic!("Node {node} not found when attempting to schedule {input:?}",);
            }
        }
        !self.is_done()
    }

    /// Process a service output event from a node.
    pub fn schedule(&mut self, node: &NodeId, out: Io) {
        let node = *node;

        match out {
            Io::Write(receiver, msgs) => {
                if msgs.is_empty() {
                    return;
                }
                // If the other end has disconnected the sender with some latency, there may not be
                // a connection remaining to use.
                if self.connections.get(&(node, receiver)).is_none() {
                    return;
                }
                let sender = node;

                if self.is_partitioned(sender, receiver) {
                    // Drop message if nodes are partitioned.
                    info!(
                        target: "sim",
                        "{} -> {} (DROPPED)",
                         sender, receiver,
                    );
                    return;
                }

                // Schedule message in the future, ensuring messages don't arrive out-of-order
                // between two peers.
                let latency = self.latency(node, receiver);
                let time = self
                    .inbox
                    .last(&receiver, &sender)
                    .map(|(k, _)| *k)
                    .unwrap_or_else(|| self.time);
                let time = time + latency;
                let elapsed = (time - self.start_time).as_millis();

                for msg in &msgs {
                    info!(
                        target: "sim",
                        "{:05} {} -> {} ({:?}) (+{})",
                        elapsed, sender, receiver, msg, latency
                    );
                }

                self.inbox.insert(
                    time,
                    Scheduled {
                        remote: sender,
                        node: receiver,
                        input: Input::Received(sender, msgs),
                    },
                );
            }
            Io::Connect(remote, addr) => {
                assert!(remote != node, "self-connections are not allowed");

                let latency = self.latency(node, remote);

                self.inbox.insert(
                    self.time + MIN_LATENCY,
                    Scheduled {
                        node,
                        remote,
                        input: Input::Connecting { id: remote, addr },
                    },
                );

                // Fail to connect if the nodes are partitioned.
                if self.is_partitioned(node, remote) {
                    log::info!(target: "sim", "{} -/-> {} (partitioned)", node, remote);

                    // Sometimes, the service gets a failure input, other times it just hangs.
                    if self.rng.bool() {
                        self.inbox.insert(
                            self.time + MIN_LATENCY,
                            Scheduled {
                                node,
                                remote,
                                input: Input::Disconnected(
                                    remote,
                                    Rc::new(DisconnectReason::Connection(Arc::new(
                                        io::Error::from(io::ErrorKind::UnexpectedEof),
                                    ))),
                                ),
                            },
                        );
                    }
                    return;
                }

                self.inbox.insert(
                    // The remote will get the connection attempt with some latency.
                    self.time + latency,
                    Scheduled {
                        node: remote,
                        remote: node,
                        input: Input::Connected {
                            id: node,
                            link: Link::Inbound,
                        },
                    },
                );
                self.inbox.insert(
                    // The local node will have established the connection after some latency.
                    self.time + latency,
                    Scheduled {
                        remote,
                        node,
                        input: Input::Connected {
                            id: remote,
                            link: Link::Outbound,
                        },
                    },
                );
            }
            Io::Disconnect(remote, reason) => {
                // The local node is immediately disconnected.
                self.priority.push_back(Scheduled {
                    remote,
                    node,
                    input: Input::Disconnected(remote, Rc::new(reason)),
                });

                // Nb. It's possible for disconnects to happen simultaneously from both ends, hence
                // it can be that a node will try to disconnect a remote that is already
                // disconnected from the other side.
                //
                // It's also possible that the connection was only attempted and never succeeded,
                // in which case we would return here.
                if !self.connections.contains(&(node, remote)) {
                    debug!(target: "sim", "Ignoring disconnect of {remote} from {node}");
                    return;
                };
                let latency = self.latency(node, remote);

                // The remote node receives the disconnection with some delay.
                self.inbox.insert(
                    self.time + latency,
                    Scheduled {
                        node: remote,
                        remote: node,
                        input: Input::Disconnected(
                            node,
                            Rc::new(DisconnectReason::Connection(Arc::new(io::Error::from(
                                io::ErrorKind::ConnectionReset,
                            )))),
                        ),
                    },
                );
            }
            Io::Wakeup(duration) => {
                let time = self.time + duration;

                if !matches!(
                    self.inbox.messages.get(&time),
                    Some(Scheduled {
                        input: Input::Wake,
                        ..
                    })
                ) {
                    self.inbox.insert(
                        time,
                        Scheduled {
                            node,
                            // The remote is not applicable for this type of output.
                            remote: [0; 32].into(),
                            input: Input::Wake,
                        },
                    );
                }
            }
            Io::Event(event) => {
                let events = self.events.entry(node).or_insert_with(VecDeque::new);
                if events.len() >= MAX_EVENTS {
                    warn!(target: "sim", "Dropping event: buffer is full");
                } else {
                    events.push_back(event);
                }
            }
            Io::Fetch(fetch) => {
                if self.is_fallible() {
                    self.inbox.insert(
                        self.time + LocalDuration::from_secs(3),
                        Scheduled {
                            node,
                            remote: fetch.remote,
                            input: Input::Fetched(Arc::new(FetchResult {
                                rid: fetch.repo,
                                initiated: fetch.initiated,
                                remote: fetch.remote,
                                namespaces: fetch.namespaces,
                                result: Err(FetchError::Io(io::ErrorKind::Other.into())),
                            })),
                        },
                    );
                } else {
                    self.inbox.insert(
                        self.time + LocalDuration::from_secs(3),
                        Scheduled {
                            node,
                            remote: fetch.remote,
                            input: Input::Fetched(Arc::new(FetchResult {
                                rid: fetch.repo,
                                initiated: fetch.initiated,
                                remote: fetch.remote,
                                namespaces: fetch.namespaces,
                                result: Ok(vec![]),
                            })),
                        },
                    );
                }
            }
        }
    }

    /// Check whether we should fail the next operation.
    fn is_fallible(&self) -> bool {
        self.rng.f64() % 1.0 < self.opts.failure_rate
    }

    /// Check whether two nodes are partitioned.
    fn is_partitioned(&self, a: NodeId, b: NodeId) -> bool {
        self.partitions.contains(&(a, b)) || self.partitions.contains(&(b, a))
    }
}

/// Perform a fetch between two local repositories.
/// This has the same outcome as doing a "real" fetch, but suffices for the simulation, and
/// doesn't require running nodes.
fn fetch<W: WriteRepository>(
    repo: &mut W,
    node: &NodeId,
    namespaces: impl Into<Namespaces>,
) -> Result<Vec<RefUpdate>, radicle::storage::FetchError> {
    let namespace = match namespaces.into() {
        Namespaces::All => None,
        Namespaces::One(ns) => Some(ns),
    };
    let mut updates = Vec::new();
    let mut callbacks = git::RemoteCallbacks::new();
    let mut opts = git::FetchOptions::default();
    let refspec = if let Some(namespace) = namespace {
        opts.prune(git::FetchPrune::On);
        format!("refs/namespaces/{namespace}/refs/*:refs/namespaces/{namespace}/refs/*")
    } else {
        opts.prune(git::FetchPrune::Off);
        "refs/namespaces/*:refs/namespaces/*".to_owned()
    };

    callbacks.update_tips(|name, old, new| {
        if let Ok(name) = radicle::git::RefString::try_from(name) {
            if name.to_namespaced().is_some() {
                updates.push(RefUpdate::from(name, old, new));
                // Returning `true` ensures the process is not aborted.
                return true;
            }
        }
        false
    });
    opts.remote_callbacks(callbacks);

    // Fetch from the remote into the staging copy.
    let mut remote = repo.raw().remote_anonymous(
        radicle::storage::git::transport::remote::Url {
            node: *node,
            repo: repo.id(),
            namespace,
        }
        .to_string()
        .as_str(),
    )?;
    remote.fetch(&[refspec], Some(&mut opts), None)?;
    drop(opts);

    repo.verify()?;
    repo.set_head()?;

    Ok(updates)
}

//! Core protocol state machine without encoding/decoding concerns
//!
//! This module defines the core protocol state machine, focusing only on
//! business logic and state transitions. All encoding/decoding remains in radicle-node.

use std::collections::HashMap;
use std::time::Duration;

use radicle::node::NodeId;

use crate::filter::Filter;

/// Protocol state machine error types
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Protocol state error
    #[error("invalid state transition: {0}")]
    InvalidState(&'static str),

    /// Protocol timeout
    #[error("protocol timeout: {0}")]
    Timeout(&'static str),

    /// Unexpected message type
    #[error("unexpected message: {0}")]
    UnexpectedMessage(&'static str),

    /// Protocol violation
    #[error("protocol violation: {0}")]
    ProtocolViolation(&'static str),
}

/// Result type for protocol state machine operations
pub type Result<T> = std::result::Result<T, Error>;

/// Protocol event types that can be processed by the state machine
#[derive(Debug, Clone)]
pub enum Event<'a> {
    /// Message received (already decoded by radicle-node)
    MessageReceived {
        /// The message that was received
        message: &'a Message,
        /// The remote peer that sent the message
        from: NodeId,
    },

    /// Send a message to a peer
    SendMessage {
        /// The message to send
        message: Message,
        /// The peer to send the message to
        to: NodeId,
    },

    /// Connection established with a peer
    ConnectionEstablished {
        /// The peer that connected
        peer: NodeId,
        /// Whether this is an inbound or outbound connection
        inbound: bool,
    },

    /// Connection lost with a peer
    ConnectionLost {
        /// The peer that disconnected
        peer: NodeId,
    },

    /// Protocol timer expired
    TimerExpired {
        /// The timer that expired
        timer: TimerType,
    },

    /// Subscription changed
    SubscriptionChanged {
        /// The new filter
        filter: Filter,
    },
}

/// Types of protocol timers
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerType {
    /// Timer for periodic node announcements
    NodeAnnouncement,
    /// Timer for inventory announcements
    InventoryAnnouncement,
    /// Timer for ping/pong health checks
    PingCheck,
    /// Timer for pruning old announcements
    Prune,
}

/// Message types from the protocol
/// This is a placeholder - actual message types would be imported from the message module
#[derive(Debug, Clone)]
pub enum Message {
    /// Subscribe to gossip messages
    Subscribe,
    /// Gossip announcement
    Announcement,
    /// Information message
    Info,
    /// Ping message
    Ping,
    /// Pong message
    Pong,
}

/// Protocol actions to be performed by the I/O layer
#[derive(Debug)]
pub enum Action {
    /// Send message to a peer (radicle-node handles encoding)
    SendMessage {
        /// The message to send
        message: Message,
        /// The peer to send to
        to: NodeId,
    },

    /// Start a timer
    StartTimer {
        /// The type of timer
        timer: TimerType,
        /// The duration of the timer
        duration: Duration,
    },

    /// Close a connection with a peer
    CloseConnection {
        /// The peer to disconnect from
        peer: NodeId,
        /// Reason for disconnection
        reason: String,
    },

    /// Multiple actions to perform
    Multiple(Vec<Action>),

    /// No action to perform
    None,
}

/// Protocol state machine
pub struct Protocol {
    /// Current state of the protocol
    state: State,
    /// Local node ID
    local_id: NodeId,
    /// Configuration
    config: ProtocolConfig,
}

/// Protocol configuration
#[derive(Debug, Clone)]
pub struct ProtocolConfig {
    /// Node announcement interval
    pub node_announcement_interval: Duration,
    /// Inventory announcement interval
    pub inventory_announcement_interval: Duration,
    /// Ping interval
    pub ping_interval: Duration,
    /// Prune interval for old announcements
    pub prune_interval: Duration,
}

impl Default for ProtocolConfig {
    fn default() -> Self {
        Self {
            node_announcement_interval: Duration::from_secs(3600), // 1 hour
            inventory_announcement_interval: Duration::from_secs(1800), // 30 minutes
            ping_interval: Duration::from_secs(60),                // 1 minute
            prune_interval: Duration::from_secs(86400),            // 1 day
        }
    }
}

/// Protocol state
#[derive(Debug, Clone)]
enum State {
    /// Initial state
    Initialized,
    /// Connected state
    Connected {
        /// Connected peers
        peers: HashMap<NodeId, PeerState>,
        /// Current subscription filter
        filter: Filter,
    },
}

/// State of a connected peer
#[derive(Debug, Clone)]
struct PeerState {
    /// Whether this is an inbound connection
    inbound: bool,
    /// Last ping sent time
    last_ping: Option<std::time::Instant>,
    /// Last announcement received time
    last_announcement: Option<std::time::Instant>,
}

impl Protocol {
    /// Create a new protocol state machine with the given configuration
    pub fn new(local_id: NodeId, config: ProtocolConfig) -> Self {
        Self {
            state: State::Initialized,
            local_id,
            config,
        }
    }

    /// Handle a protocol event
    pub fn handle_event(&mut self, event: Event) -> Result<Action> {
        match (&self.state, event) {
            // Handle connection establishment
            (State::Initialized, Event::ConnectionEstablished { peer, inbound }) => {
                self.transition_to_connected(peer, inbound)
            }

            // Handle events in connected state
            (State::Connected { .. }, Event::ConnectionEstablished { peer, inbound }) => {
                self.handle_connection_established(peer, inbound)
            }
            (State::Connected { .. }, Event::ConnectionLost { peer }) => {
                self.handle_connection_lost(peer)
            }
            (State::Connected { .. }, Event::MessageReceived { message, from }) => {
                self.handle_message_received(message, from)
            }
            (State::Connected { .. }, Event::SendMessage { message, to }) => {
                self.handle_send_message(message, to)
            }
            (State::Connected { .. }, Event::TimerExpired { timer }) => {
                self.handle_timer_expired(timer)
            }
            (State::Connected { .. }, Event::SubscriptionChanged { filter }) => {
                self.handle_subscription_changed(filter)
            }

            // Invalid state transitions
            (State::Initialized, _) => Err(Error::InvalidState("protocol not connected")),
        }
    }

    // State transition to connected
    fn transition_to_connected(&mut self, peer: NodeId, inbound: bool) -> Result<Action> {
        let mut peers = HashMap::new();
        peers.insert(
            peer,
            PeerState {
                inbound,
                last_ping: None,
                last_announcement: None,
            },
        );

        self.state = State::Connected {
            peers,
            filter: Filter::default(),
        };

        // Start timers and send initial subscription
        let actions = vec![
            Action::StartTimer {
                timer: TimerType::NodeAnnouncement,
                duration: self.config.node_announcement_interval,
            },
            Action::StartTimer {
                timer: TimerType::InventoryAnnouncement,
                duration: self.config.inventory_announcement_interval,
            },
            Action::StartTimer {
                timer: TimerType::PingCheck,
                duration: self.config.ping_interval,
            },
            Action::StartTimer {
                timer: TimerType::Prune,
                duration: self.config.prune_interval,
            },
            // Send subscription to the new peer
            Action::SendMessage {
                message: Message::Subscribe,
                to: peer,
            },
        ];

        Ok(Action::Multiple(actions))
    }

    // Handle a new connection
    fn handle_connection_established(&mut self, peer: NodeId, inbound: bool) -> Result<Action> {
        if let State::Connected { peers, .. } = &mut self.state {
            peers.insert(
                peer,
                PeerState {
                    inbound,
                    last_ping: None,
                    last_announcement: None,
                },
            );

            // Send subscription to the new peer
            Ok(Action::SendMessage {
                message: Message::Subscribe,
                to: peer,
            })
        } else {
            Err(Error::InvalidState("not in connected state"))
        }
    }

    // Handle connection loss
    fn handle_connection_lost(&mut self, peer: NodeId) -> Result<Action> {
        if let State::Connected { peers, .. } = &mut self.state {
            peers.remove(&peer);
            Ok(Action::None)
        } else {
            Err(Error::InvalidState("not in connected state"))
        }
    }

    // Handle incoming message
    fn handle_message_received(&mut self, message: &Message, from: NodeId) -> Result<Action> {
        if let State::Connected { peers, .. } = &mut self.state {
            // Update peer state based on message type
            if let Some(peer) = peers.get_mut(&from) {
                peer.last_announcement = Some(std::time::Instant::now());

                // Process the message based on its type
                match message {
                    Message::Ping => {
                        // Send a pong response
                        Ok(Action::SendMessage {
                            message: Message::Pong,
                            to: from,
                        })
                    }
                    Message::Pong => {
                        // Update peer ping state
                        peer.last_ping = None; // Clear the ping state
                        Ok(Action::None)
                    }
                    Message::Announcement => {
                        // Process announcement (in a real implementation, this would do more)
                        Ok(Action::None)
                    }
                    Message::Subscribe => {
                        // Process subscription request
                        Ok(Action::None)
                    }
                    Message::Info => {
                        // Process info message
                        Ok(Action::None)
                    }
                }
            } else {
                Err(Error::ProtocolViolation("message from unknown peer"))
            }
        } else {
            Err(Error::InvalidState("not in connected state"))
        }
    }

    // Handle request to send a message
    fn handle_send_message(&mut self, message: Message, to: NodeId) -> Result<Action> {
        if let State::Connected { peers, .. } = &self.state {
            // Check if the peer is connected
            if peers.contains_key(&to) {
                Ok(Action::SendMessage { message, to })
            } else {
                Err(Error::ProtocolViolation("peer not connected"))
            }
        } else {
            Err(Error::InvalidState("not in connected state"))
        }
    }

    // Handle a timer expiration
    fn handle_timer_expired(&mut self, timer: TimerType) -> Result<Action> {
        match timer {
            TimerType::NodeAnnouncement => {
                // Logic for periodic node announcement
                let mut actions = vec![];

                if let State::Connected { peers, .. } = &self.state {
                    // Send node announcement to all connected peers
                    for peer in peers.keys() {
                        actions.push(Action::SendMessage {
                            message: Message::Announcement,
                            to: *peer,
                        });
                    }
                }

                // Restart the timer
                actions.push(Action::StartTimer {
                    timer: TimerType::NodeAnnouncement,
                    duration: self.config.node_announcement_interval,
                });

                if actions.len() == 1 {
                    // Only the timer restart action
                    Ok(actions.pop().unwrap())
                } else {
                    Ok(Action::Multiple(actions))
                }
            }
            TimerType::InventoryAnnouncement => {
                // Logic for inventory announcements
                let mut actions = vec![];

                if let State::Connected { peers, .. } = &self.state {
                    // Send inventory announcement to all connected peers
                    for peer in peers.keys() {
                        actions.push(Action::SendMessage {
                            message: Message::Announcement,
                            to: *peer,
                        });
                    }
                }

                // Restart the timer
                actions.push(Action::StartTimer {
                    timer: TimerType::InventoryAnnouncement,
                    duration: self.config.inventory_announcement_interval,
                });

                if actions.len() == 1 {
                    // Only the timer restart action
                    Ok(actions.pop().unwrap())
                } else {
                    Ok(Action::Multiple(actions))
                }
            }
            TimerType::PingCheck => {
                // Logic for sending pings and checking timeouts
                let mut actions = vec![];

                if let State::Connected { peers, .. } = &mut self.state {
                    // Check which peers need to be pinged
                    for (peer_id, peer) in peers.iter_mut() {
                        match peer.last_ping {
                            None => {
                                // No ping in progress, send a new one
                                peer.last_ping = Some(std::time::Instant::now());
                                actions.push(Action::SendMessage {
                                    message: Message::Ping,
                                    to: *peer_id,
                                });
                            }
                            Some(time) => {
                                // Check for ping timeout
                                if time.elapsed() > Duration::from_secs(30) {
                                    // Ping timeout, disconnect peer
                                    actions.push(Action::CloseConnection {
                                        peer: *peer_id,
                                        reason: "ping timeout".to_string(),
                                    });
                                }
                            }
                        }
                    }
                }

                // Restart the timer
                actions.push(Action::StartTimer {
                    timer: TimerType::PingCheck,
                    duration: self.config.ping_interval,
                });

                if actions.len() == 1 {
                    // Only the timer restart action
                    Ok(actions.pop().unwrap())
                } else {
                    Ok(Action::Multiple(actions))
                }
            }
            TimerType::Prune => {
                // Logic for pruning old announcements would be implemented here
                // Simply restart the timer for now
                Ok(Action::StartTimer {
                    timer: TimerType::Prune,
                    duration: self.config.prune_interval,
                })
            }
        }
    }

    // Handle subscription change
    fn handle_subscription_changed(&mut self, filter: Filter) -> Result<Action> {
        // First get a clone of the peers to avoid borrow issues
        let peers_clone = match &self.state {
            State::Connected { peers, .. } => peers.clone(),
            _ => return Err(Error::InvalidState("not in connected state")),
        };

        // Update the filter
        self.state = State::Connected {
            peers: peers_clone.clone(),
            filter,
        };

        // Send updated subscription to all peers
        let mut actions = Vec::new();
        for peer in peers_clone.keys() {
            actions.push(Action::SendMessage {
                message: Message::Subscribe,
                to: *peer,
            });
        }

        if actions.is_empty() {
            Ok(Action::None)
        } else if actions.len() == 1 {
            Ok(actions.pop().unwrap())
        } else {
            Ok(Action::Multiple(actions))
        }
    }
}

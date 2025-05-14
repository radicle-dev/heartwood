//! Protocol adapter for the radicle-protocol sans-IO implementation
//!
//! This module bridges between the radicle-protocol state machine and the
//! radicle-node I/O layer, handling all serialization/deserialization.

use std::collections::HashMap;
use std::time::Duration;

use radicle::node::NodeId;
use radicle_protocol::message::{
    Announcement, AnnouncementMessage, Info, InventoryAnnouncement, Message as ProtocolMessage,
    NodeAnnouncement, Ping, RefsAnnouncement, Subscribe, ZeroBytes,
};
use radicle_protocol::{
    Action, Error as ProtocolError, Event, Protocol, ProtocolConfig, TimerType,
};

use crate::service::io::Io;
use crate::service::message::Message;
use crate::service::DisconnectReason;
use crate::wire::{self, deserialize, serialize};

/// Protocol state for a single peer connection
pub struct ProtocolAdapter {
    /// The protocol state machine
    protocol: Protocol,
    /// Pending timers - maps timer types to when they expire
    timers: HashMap<TimerType, std::time::Instant>,
}

/// Error returned by the protocol adapter
#[derive(thiserror::Error, Debug)]
pub enum Error {
    /// Protocol error
    #[error("protocol error: {0}")]
    Protocol(#[from] ProtocolError),

    /// Serialization/deserialization error
    #[error("serialization error: {0}")]
    Serialization(#[from] wire::Error),
}

/// Result type for protocol adapter operations
pub type Result<T> = std::result::Result<T, Error>;

impl ProtocolAdapter {
    /// Create a new protocol adapter with the given configuration
    pub fn new(local_id: NodeId, config: ProtocolConfig) -> Self {
        Self {
            protocol: Protocol::new(local_id, config),
            timers: HashMap::new(),
        }
    }

    /// Handle an incoming message from a peer
    pub fn handle_message(&mut self, message: &Message, from: NodeId) -> Result<Vec<Io>> {
        // Convert the radicle-node Message to a protocol Message
        let protocol_message = self.convert_incoming_message(message)?;

        // Pass the message to the protocol state machine
        let event = Event::MessageReceived {
            message: &protocol_message,
            from,
        };

        // Get the protocol's response
        let action = self.protocol.handle_event(event)?;

        // Convert the action to radicle-node I/O operations
        self.handle_action(action, from)
    }

    /// Handle a new connection
    pub fn handle_connection(&mut self, peer: NodeId, inbound: bool) -> Result<Vec<Io>> {
        let event = Event::ConnectionEstablished { peer, inbound };
        let action = self.protocol.handle_event(event)?;

        self.handle_action(action, peer)
    }

    /// Handle a connection loss
    pub fn handle_disconnection(&mut self, peer: NodeId) -> Result<Vec<Io>> {
        let event = Event::ConnectionLost { peer };
        let action = self.protocol.handle_event(event)?;

        self.handle_action(action, peer)
    }

    /// Handle a subscription change
    pub fn handle_subscription(
        &mut self,
        filter: radicle_protocol::filter::Filter,
    ) -> Result<Vec<Io>> {
        let event = Event::SubscriptionChanged { filter };
        let action = self.protocol.handle_event(event)?;

        // Node ID doesn't matter here since actions specify their own destination
        self.handle_action(action, NodeId::default())
    }

    /// Check if any timers have expired and process them
    pub fn check_timers(&mut self) -> Result<Vec<Io>> {
        let now = std::time::Instant::now();
        let mut io = Vec::new();

        // Find all expired timers
        let expired: Vec<_> = self
            .timers
            .iter()
            .filter(|(_, &expire_time)| now >= expire_time)
            .map(|(&timer, _)| timer)
            .collect();

        // Process each expired timer
        for timer in expired {
            // Remove the timer
            self.timers.remove(&timer);

            // Create a timer event
            let event = Event::TimerExpired { timer };
            let action = self.protocol.handle_event(event)?;

            // Process the action
            let mut timer_io = self.handle_action(action, NodeId::default())?;
            io.append(&mut timer_io);
        }

        Ok(io)
    }

    /// Convert a radicle-node Message to a protocol Message
    fn convert_incoming_message(&self, message: &Message) -> Result<ProtocolMessage> {
        match message {
            Message::Subscribe(subscribe) => {
                // Deserialize the Subscribe message
                let filter = subscribe.filter.clone();
                let since = subscribe.since;
                let until = subscribe.until;

                Ok(ProtocolMessage::Subscribe(Subscribe {
                    filter,
                    since,
                    until,
                }))
            }
            Message::Announcement(announcement) => {
                // Deserialize the Announcement
                Ok(ProtocolMessage::Announcement(Announcement {
                    node: announcement.node,
                    signature: announcement.signature,
                    message: match &announcement.message {
                        AnnouncementMessage::Node(node_ann) => {
                            AnnouncementMessage::Node(NodeAnnouncement {
                                version: node_ann.version,
                                features: node_ann.features.clone(),
                                timestamp: node_ann.timestamp,
                                alias: node_ann.alias.clone(),
                                addresses: node_ann.addresses.clone(),
                                nonce: node_ann.nonce,
                                agent: node_ann.agent.clone(),
                            })
                        }
                        AnnouncementMessage::Inventory(inv_ann) => {
                            AnnouncementMessage::Inventory(InventoryAnnouncement {
                                inventory: inv_ann.inventory.clone(),
                                timestamp: inv_ann.timestamp,
                            })
                        }
                        AnnouncementMessage::Refs(refs_ann) => {
                            AnnouncementMessage::Refs(RefsAnnouncement {
                                rid: refs_ann.rid,
                                refs: refs_ann.refs.clone(),
                                timestamp: refs_ann.timestamp,
                            })
                        }
                    },
                }))
            }
            Message::Info(info) => {
                // Deserialize the Info message
                Ok(ProtocolMessage::Info(match info {
                    Info::RefsAlreadySynced { rid, at } => {
                        Info::RefsAlreadySynced { rid: *rid, at: *at }
                    }
                }))
            }
            Message::Ping(ping) => {
                // Deserialize the Ping message
                Ok(ProtocolMessage::Ping(Ping {
                    ponglen: ping.ponglen,
                    zeroes: ZeroBytes::new(ping.zeroes.len() as u16),
                }))
            }
            Message::Pong { zeroes } => {
                // Deserialize the Pong message
                Ok(ProtocolMessage::Pong {
                    zeroes: ZeroBytes::new(zeroes.len() as u16),
                })
            }
        }
    }

    /// Convert a protocol Message to a radicle-node Message
    fn convert_outgoing_message(&self, message: ProtocolMessage) -> Result<Message> {
        match message {
            ProtocolMessage::Subscribe(subscribe) => {
                // Create a wire-compatible Subscribe message
                let message = Message::Subscribe(crate::service::message::Subscribe {
                    filter: subscribe.filter,
                    since: subscribe.since,
                    until: subscribe.until,
                });

                Ok(message)
            }
            ProtocolMessage::Announcement(announcement) => {
                // Create a wire-compatible Announcement message
                let wire_message = match announcement.message {
                    AnnouncementMessage::Node(node_ann) => {
                        crate::service::message::AnnouncementMessage::Node(
                            crate::service::message::NodeAnnouncement {
                                version: node_ann.version,
                                features: node_ann.features,
                                timestamp: node_ann.timestamp,
                                alias: node_ann.alias,
                                addresses: node_ann.addresses,
                                nonce: node_ann.nonce,
                                agent: node_ann.agent,
                            },
                        )
                    }
                    AnnouncementMessage::Inventory(inv_ann) => {
                        crate::service::message::AnnouncementMessage::Inventory(
                            crate::service::message::InventoryAnnouncement {
                                inventory: inv_ann.inventory,
                                timestamp: inv_ann.timestamp,
                            },
                        )
                    }
                    AnnouncementMessage::Refs(refs_ann) => {
                        crate::service::message::AnnouncementMessage::Refs(
                            crate::service::message::RefsAnnouncement {
                                rid: refs_ann.rid,
                                refs: refs_ann.refs,
                                timestamp: refs_ann.timestamp,
                            },
                        )
                    }
                };

                Ok(Message::Announcement(
                    crate::service::message::Announcement {
                        node: announcement.node,
                        signature: announcement.signature,
                        message: wire_message,
                    },
                ))
            }
            ProtocolMessage::Info(info) => {
                // Create a wire-compatible Info message
                let wire_info = match info {
                    Info::RefsAlreadySynced { rid, at } => {
                        crate::service::message::Info::RefsAlreadySynced { rid, at }
                    }
                };

                Ok(Message::Info(wire_info))
            }
            ProtocolMessage::Ping(ping) => {
                // Create a wire-compatible Ping message
                Ok(Message::Ping(crate::service::message::Ping {
                    ponglen: ping.ponglen,
                    zeroes: crate::service::message::ZeroBytes::new(ping.zeroes.len() as u16),
                }))
            }
            ProtocolMessage::Pong { zeroes } => {
                // Create a wire-compatible Pong message
                Ok(Message::Pong {
                    zeroes: crate::service::message::ZeroBytes::new(zeroes.len() as u16),
                })
            }
        }
    }

    /// Handle a protocol action and convert it to radicle-node I/O operations
    fn handle_action(&mut self, action: Action, peer: NodeId) -> Result<Vec<Io>> {
        match action {
            Action::SendMessage { message, to } => {
                // Convert the protocol message to a radicle-node message
                let message = self.convert_outgoing_message(message)?;

                // Create an I/O operation to send the message
                Ok(vec![Io::Write(to, vec![message])])
            }
            Action::StartTimer { timer, duration } => {
                // Register the timer
                let expire_time = std::time::Instant::now() + duration;
                self.timers.insert(timer, expire_time);

                // Create an I/O operation to wake up when the timer expires
                let local_duration = radicle::node::timestamp::LocalDuration::from_std(duration)
                    .unwrap_or(radicle::node::timestamp::LocalDuration::from_secs(
                        duration.as_secs(),
                    ));

                Ok(vec![Io::Wakeup(local_duration)])
            }
            Action::CloseConnection { peer, reason } => {
                // Create an I/O operation to disconnect
                Ok(vec![Io::Disconnect(
                    peer,
                    DisconnectReason::Protocol(reason),
                )])
            }
            Action::Multiple(actions) => {
                // Process each action and collect the I/O operations
                let mut io = Vec::new();

                for action in actions {
                    let mut action_io = self.handle_action(action, peer)?;
                    io.append(&mut action_io);
                }

                Ok(io)
            }
            Action::None => Ok(Vec::new()),
        }
    }
}

/// Convert radicle::node::timestamp::LocalDuration to std::time::Duration
#[inline]
fn local_to_std_duration(d: radicle::node::timestamp::LocalDuration) -> Duration {
    Duration::from_secs(d.as_secs())
}

/// Create a default protocol configuration
pub fn default_protocol_config() -> ProtocolConfig {
    ProtocolConfig {
        node_announcement_interval: Duration::from_secs(3600), // 1 hour
        inventory_announcement_interval: Duration::from_secs(1800), // 30 minutes
        ping_interval: Duration::from_secs(60),                // 1 minute
        prune_interval: Duration::from_secs(86400),            // 1 day
    }
}

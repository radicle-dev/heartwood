//! Radicle Protocol Implementation
//!
//! This crate provides the core protocol logic for Radicle in a "sans I/O" style,
//! separating protocol state from I/O concerns.
//!
//! ## Pure Protocol Logic Design
//!
//! The protocol is implemented as a pure business logic state machine, with absolutely
//! no encoding/decoding concerns. All serialization/deserialization remains in radicle-node,
//! keeping a clean separation of concerns.
//!
//! ### Key components:
//!
//! * **State Machine**: The protocol operates as a state machine with explicit transitions
//! * **Events**: External events drive the state machine
//! * **Actions**: The state machine responds with actions to be performed by the I/O layer
//! * **No Serialization**: The protocol deals only with parsed messages, not bytes
//! * **Clear Boundaries**: Complete separation between protocol logic and I/O/encoding
//! * **Timeouts**: Protocol timeouts are handled explicitly by the state machine

pub mod filter;
pub mod gossip;
pub mod message;
pub mod prelude;
pub mod protocol;

// Re-export core components for easy access
pub use protocol::{Action, Error, Event, Message, Protocol, ProtocolConfig, TimerType};

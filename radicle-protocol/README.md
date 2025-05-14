# Radicle Protocol

## Status

This crate is currently work in progress. It aims to extract the protocol-level logic from radicle-node to clearly separate concerns between I/O and protocol logic.

## Pure Protocol Logic Design

The protocol implementation follows a key principle: **complete separation between protocol logic and encoding/decoding**.

- All protocol logic, state transitions, timeouts, etc. are implemented in this crate
- All serialization/deserialization logic stays in radicle-node

This creates a clean architecture where:

1. **Protocol Logic**: Defined entirely in terms of parsed messages and state transitions
2. **Encoding/Decoding**: Handled exclusively by radicle-node

### Core Architecture

#### State Machine

The protocol operates as an explicit state machine with well-defined states and transitions. The state machine:

- Processes events from the I/O layer with already parsed messages
- Updates internal state based on protocol rules
- Returns actions to be performed by the I/O layer
- Never performs I/O operations directly
- Never handles any serialization/deserialization

#### Event-Driven API

The protocol is driven by events that come from the I/O layer:

```rust
pub enum Event<'a> {
    MessageReceived { message: &'a Message, from: NodeId },
    ConnectionEstablished { peer: NodeId, inbound: bool },
    ConnectionLost { peer: NodeId },
    TimerExpired { timer: TimerType },
    // ...
}
```

Note that `MessageReceived` contains already parsed `Message` objects, not raw bytes.

#### Action-Based Response

The protocol responds with actions to be performed by the I/O layer:

```rust
pub enum Action {
    SendMessage { message: Message, to: NodeId },
    StartTimer { timer: TimerType, duration: Duration },
    CloseConnection { peer: NodeId, reason: String },
    // ...
}
```

The `SendMessage` action contains a protocol-level `Message` object, not serialized bytes. The radicle-node
is responsible for serializing this message before sending it on the network.

## Implementation Progress

1. Protocol state machine architecture implemented
   - Event-driven API
   - Action-based responses
   - Proper state transitions
   - Timer handling

## Next Steps

1. Complete the message types implementation
   - Ensure all protocol messages are properly defined without serialization concerns

2. Extract remaining protocol logic from radicle-node
   - Complete the gossip protocol implementation
   - Move service filter logic
   - Move other protocol-specific components

3. Update radicle-node to use radicle-protocol
   - Create adapter between protocol logic and network I/O
   - Handle all serialization/deserialization in radicle-node
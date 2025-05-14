# Next Steps for Radicle Protocol Implementation

## Separation of Protocol Logic from Encoding/Decoding

The key principle we're following is to keep protocol logic completely separate from encoding/decoding. This means:

1. The protocol (in radicle-protocol) deals with pure business logic and state transitions
2. All serialization/deserialization remains in radicle-node

## Specific Tasks

### 1. Message Types

Refine the protocol message types to contain only the business logic:

- Define all message variants in terms of their semantic meaning
- Remove any wire format, encoding, or decoding concerns
- Focus on the protocol behavior, not the wire representation

### 2. Complete Protocol State Machine

Implement the full protocol state machine behavior:

- Complete event handlers for all protocol events
- Define transitions for all protocol states
- Implement all protocol message handling logic
- Handle peer connections, announcements, subscriptions, etc.

### 3. Extract Remaining Logic from radicle-node

Identify and move all protocol business logic from radicle-node to radicle-protocol:

- Gossip protocol logic
- Filter/subscription logic
- Peer management logic
- Announcement management

### 4. radicle-node Integration

Create an adapter layer in radicle-node that:

1. Receives network I/O (incoming connections, messages, etc.)
2. Deserializes bytes into protocol messages
3. Passes decoded messages to the protocol state machine
4. Receives actions from the protocol state machine
5. Serializes protocol messages into bytes for network transmission
6. Handles all I/O operations

## Architectural Example

```
┌─────────────────────┐     ┌──────────────────────────┐
│ radicle-node        │     │ radicle-protocol         │
│                     │     │                          │
│ ┌─────────────────┐ │     │ ┌────────────────────┐  │
│ │ Network I/O     │ │     │ │                    │  │
│ └─────────────────┘ │     │ │                    │  │
│         │           │     │ │                    │  │
│ ┌─────────────────┐ │     │ │                    │  │
│ │ Serialization/  │ │     │ │   Protocol Logic   │  │
│ │ Deserialization │◄┼─────┼─┤   State Machine    │  │
│ └─────────────────┘ │     │ │                    │  │
│         │           │     │ │                    │  │
│ ┌─────────────────┐ │     │ │                    │  │
│ │ Protocol Adapter│◄┼────►│ │                    │  │
│ └─────────────────┘ │     │ └────────────────────┘  │
└─────────────────────┘     └──────────────────────────┘
```

## Testing Strategy

With this separation, testing becomes cleaner:

1. **Protocol Tests**
   - Test protocol state transitions with pure message objects
   - Test protocol logic with mocked events
   - No need for serialization/deserialization in these tests

2. **Serialization Tests** (in radicle-node)
   - Test encoding/decoding separately from protocol logic
   - Ensure wire format compatibility

3. **Integration Tests**
   - Test the full stack together, but with clear boundaries
//! Property-Based Tests for Connection State Management

mod arbitrary;
mod invariants;

mod helpers;

// Properties
mod address;
mod attempt_counter;
mod command;
mod connection_type;
mod inbound_outbound;
mod iterator;
mod link_direction;
mod ping_pong;
mod rate_limiting;
mod state_machine_model;
mod state_transition;
mod subscription;
mod timing;
mod uniqueness;

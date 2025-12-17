//! Invariant checking functions for connection state management.

use std::collections::HashSet;

use radicle::node::{Link, NodeId};

use crate::connections::session::Sessions;
use crate::connections::state::Connections;

// =============================================================================
// Error Types
// =============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvariantViolation {
    /// A node appears in multiple state collections
    DuplicateSession { node: NodeId },
    /// The local node appears in a session collection
    LocalNodeInSession { node: NodeId },
    /// Session existence check is inconsistent with state checks
    SessionExistenceInconsistent { node: NodeId },
    /// Link count mismatch
    LinkCountMismatch {
        link: Link,
        counted: usize,
        reported: usize,
    },
}

impl std::fmt::Display for InvariantViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateSession { node } => {
                write!(f, "Node {:?} appears in multiple states", node)
            }
            Self::LocalNodeInSession { node } => {
                write!(f, "Local node {:?} found in sessions", node)
            }
            Self::SessionExistenceInconsistent { node } => {
                write!(f, "Session existence inconsistent for node {:?}", node)
            }
            Self::LinkCountMismatch {
                link,
                counted,
                reported,
            } => {
                write!(
                    f,
                    "{:?} count mismatch: counted={}, reported={}",
                    link, counted, reported
                )
            }
        }
    }
}

impl std::error::Error for InvariantViolation {}

// =============================================================================
// Invariant Checking Functions
// =============================================================================

/// Check all core invariants on a Connections instance.
pub fn check_invariants(
    connections: &Connections,
    local: &NodeId,
) -> Result<(), InvariantViolation> {
    let sessions = connections.sessions();
    check_single_session_per_node(sessions)?;
    check_local_node_exclusion(sessions, local)?;
    check_session_existence_consistency(sessions)?;
    check_link_count_consistency(sessions)?;
    Ok(())
}

/// A node should only appear in the sessions exactly once, or not at all.
pub fn check_single_session_per_node(sessions: &Sessions) -> Result<(), InvariantViolation> {
    let mut seen_nodes: HashSet<NodeId> = HashSet::new();
    for (node, _) in sessions.iter() {
        if !seen_nodes.insert(*node) {
            return Err(InvariantViolation::DuplicateSession { node: *node });
        }
    }
    Ok(())
}

/// The local node should never appear in any session collection.
pub fn check_local_node_exclusion(
    sessions: &Sessions,
    local: &NodeId,
) -> Result<(), InvariantViolation> {
    if sessions.has_session_for(local) {
        return Err(InvariantViolation::LocalNodeInSession { node: *local });
    }
    Ok(())
}

/// For every session, the corresponding node should appear exactly once.
pub fn check_session_existence_consistency(sessions: &Sessions) -> Result<(), InvariantViolation> {
    for (node, _) in sessions.iter() {
        let has_session = sessions.has_session_for(node);
        let state_count = sessions.is_initial(node) as u8
            + sessions.is_attempted(node) as u8
            + sessions.get_connected(node).is_some() as u8
            + sessions.is_disconnected(node) as u8;

        if has_session && state_count != 1 {
            return Err(InvariantViolation::SessionExistenceInconsistent { node: *node });
        }
        if !has_session && state_count != 0 {
            return Err(InvariantViolation::SessionExistenceInconsistent { node: *node });
        }
    }
    Ok(())
}

/// For every connected session, the computed link counts should match.
pub fn check_link_count_consistency(sessions: &Sessions) -> Result<(), InvariantViolation> {
    let mut inbound_count = 0;
    let mut outbound_count = 0;

    for session in sessions.connected().sessions() {
        match session.link() {
            Link::Inbound => inbound_count += 1,
            Link::Outbound => outbound_count += 1,
        }
    }

    if inbound_count != sessions.connected_inbound() {
        return Err(InvariantViolation::LinkCountMismatch {
            link: Link::Inbound,
            counted: inbound_count,
            reported: sessions.connected_inbound(),
        });
    }
    if outbound_count != sessions.connected_outbound() {
        return Err(InvariantViolation::LinkCountMismatch {
            link: Link::Outbound,
            counted: outbound_count,
            reported: sessions.connected_outbound(),
        });
    }
    Ok(())
}

// =============================================================================
// State Transition Oracle
// =============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SessionState {
    Initial,
    Attempted,
    Connected,
    Disconnected,
}

/// Check if a state transition is explicitly invalid.
pub fn is_invalid_transition(from: SessionState, to: SessionState) -> bool {
    matches!(
        (from, to),
        (SessionState::Attempted, SessionState::Initial)
            | (SessionState::Connected, SessionState::Initial)
            | (SessionState::Connected, SessionState::Attempted)
            | (SessionState::Disconnected, SessionState::Attempted)
    )
}

/// Determine the current state of a session.
pub fn get_session_state(sessions: &Sessions, node: &NodeId) -> Option<SessionState> {
    if sessions.is_initial(node) {
        Some(SessionState::Initial)
    } else if sessions.is_attempted(node) {
        Some(SessionState::Attempted)
    } else if sessions.get_connected(node).is_some() {
        Some(SessionState::Connected)
    } else if sessions.is_disconnected(node) {
        Some(SessionState::Disconnected)
    } else {
        None // Session doesn't exist
    }
}

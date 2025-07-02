use std::collections::HashMap;
use std::collections::hash_map;

use radicle::node::NodeId;

use super::State;
use super::{Attempted, Connected, Disconnected, Initial, Session};

/// Provides an [`Iterator`] over all the sessions.
///
/// The order of the sessions are in:
/// - [`Connected`]
/// - [`Attempted`]
/// - [`Initial`]
/// - [`Disconnected`]
pub struct SessionsIter<'a> {
    pub(super) initial: hash_map::Iter<'a, NodeId, Session<Initial>>,
    pub(super) attempted: hash_map::Iter<'a, NodeId, Session<Attempted>>,
    pub(super) disconnected: hash_map::Iter<'a, NodeId, Session<Disconnected>>,
    pub(super) connected: hash_map::Iter<'a, NodeId, Session<Connected>>,
}

impl<'a> Iterator for SessionsIter<'a> {
    type Item = (&'a NodeId, Session<State>);

    fn next(&mut self) -> Option<Self::Item> {
        self.connected
            .next()
            .map(|(n, s)| (n, s.clone().into_any_state()))
            .or_else(|| {
                self.attempted
                    .next()
                    .map(|(n, s)| (n, s.clone().into_any_state()))
            })
            .or_else(|| {
                self.initial
                    .next()
                    .map(|(n, s)| (n, s.clone().into_any_state()))
            })
            .or_else(|| {
                self.disconnected
                    .next()
                    .map(|(n, s)| (n, s.clone().into_any_state()))
            })
    }
}

/// A view into sessions of a particular state.
///
/// - [`SessionsView::into_iter`]: to iterate over both the [`NodeId`] and [`Session`]s.
/// - [`SessionsView::node_ids`]: to iterate over just the [`NodeId`]s.
/// - [`SessionsView::sessions`]: to iterate over just the [`Session`]s.
pub struct SessionsView<'a, S> {
    pub(super) inner: &'a HashMap<NodeId, Session<S>>,
}

impl<'a, S> SessionsView<'a, S> {
    /// Return an iterator over the [`NodeId`]s of these sessions.
    pub fn node_ids(self) -> hash_map::Keys<'a, NodeId, Session<S>> {
        self.inner.keys()
    }

    /// Return an iterator over the [`Session`]s.
    pub fn sessions(self) -> hash_map::Values<'a, NodeId, Session<S>> {
        self.inner.values()
    }

    /// Returns the number of sessions.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Returns `true` if there are no sessions.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

impl<'a, S> IntoIterator for SessionsView<'a, S> {
    type Item = (&'a NodeId, &'a Session<S>);
    type IntoIter = hash_map::Iter<'a, NodeId, Session<S>>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.iter()
    }
}

pub(in crate::connections) struct SessionsViewMut<'a, S> {
    pub inner: &'a mut HashMap<NodeId, Session<S>>,
}

impl<'a, S> SessionsViewMut<'a, S> {
    /// Return an iterator over the [`Session`]s.
    pub fn sessions(self) -> hash_map::ValuesMut<'a, NodeId, Session<S>> {
        self.inner.values_mut()
    }
}

impl<'a, S> IntoIterator for SessionsViewMut<'a, S> {
    type Item = (&'a NodeId, &'a mut Session<S>);
    type IntoIter = hash_map::IterMut<'a, NodeId, Session<S>>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.iter_mut()
    }
}

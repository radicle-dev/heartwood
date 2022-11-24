#![allow(clippy::collapsible_if)]
#![allow(clippy::collapsible_else_if)]
#![allow(clippy::type_complexity)]
pub mod clock;
pub mod lwwmap;
pub mod lwwreg;
pub mod lwwset;
pub mod ord;
pub mod redactable;
pub mod thread;

#[cfg(test)]
mod test;

////////////////////////////////////////////////////////////////////////////////

use std::collections::{BTreeMap, HashMap};
use std::hash::Hash;

/// A join-semilattice.
pub trait Semilattice: Sized {
    /// Merge an other semilattice into this one.
    ///
    /// This operation should obbey the semilattice laws and should thus be idempotent,
    /// associative and commutative.
    fn merge(&mut self, other: Self);

    /// Like [`Semilattice::merge`] but takes and returns a new semilattice.
    fn join(mut self, other: Self) -> Self {
        self.merge(other);
        self
    }
}

impl<K: Ord, V: Semilattice> Semilattice for BTreeMap<K, V> {
    fn merge(&mut self, other: Self) {
        use std::collections::btree_map::Entry;

        for (k, v) in other {
            match self.entry(k) {
                Entry::Occupied(mut e) => {
                    e.get_mut().merge(v);
                }
                Entry::Vacant(e) => {
                    e.insert(v);
                }
            }
        }
    }
}

impl<K: Hash + PartialEq + Eq, V: Semilattice> Semilattice for HashMap<K, V> {
    fn merge(&mut self, other: Self) {
        use std::collections::hash_map::Entry;

        for (k, v) in other {
            match self.entry(k) {
                Entry::Occupied(mut e) => {
                    e.get_mut().merge(v);
                }
                Entry::Vacant(e) => {
                    e.insert(v);
                }
            }
        }
    }
}

pub fn fold<S>(i: impl IntoIterator<Item = S>) -> S
where
    S: Semilattice + Default,
{
    i.into_iter().fold(S::default(), S::join)
}

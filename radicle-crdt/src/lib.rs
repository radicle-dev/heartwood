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

/// Reduce an iterator of semilattice values to its least upper bound.
pub fn fold<S>(i: impl IntoIterator<Item = S>) -> S
where
    S: Semilattice + Default,
{
    i.into_iter().fold(S::default(), S::join)
}

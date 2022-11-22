#![allow(clippy::collapsible_if)]
#![allow(clippy::collapsible_else_if)]
#![allow(clippy::type_complexity)]
pub mod lwwmap;
pub mod lwwreg;
pub mod lwwset;
pub mod ord;
pub mod thread;

#[cfg(test)]
mod test;

////////////////////////////////////////////////////////////////////////////////

/// A join-semilattice.
pub trait Semilattice: Default {
    /// Join or "merge" two semilattices into one.
    fn join(self, other: Self) -> Self;
}

/// Reduce an iterator of semilattice values to its least upper bound.
pub fn fold<S>(i: impl IntoIterator<Item = S>) -> S
where
    S: Semilattice,
{
    i.into_iter().fold(S::default(), S::join)
}

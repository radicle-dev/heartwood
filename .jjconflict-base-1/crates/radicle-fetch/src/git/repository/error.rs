use radicle::git::{
    self, Oid,
    fmt::{Namespaced, Qualified},
};
use thiserror::Error;

#[derive(Debug, Error)]
#[error("could not open Git ODB")]
pub struct Contains(#[source] pub git::raw::Error);

#[derive(Debug, Error)]
pub enum Ancestry {
    #[error("missing {oid} while checking ancestry")]
    Missing { oid: Oid },
    #[error("failed to check ancestry for {old} and {new}: {err}")]
    Check {
        old: Oid,
        new: Oid,
        #[source]
        err: git::raw::Error,
    },
    #[error("failed to peel object to commit {oid}: {err}")]
    Peel {
        oid: Oid,
        #[source]
        err: git::raw::Error,
    },
    #[error("failed to find object {oid}: {err}")]
    Object {
        oid: Oid,
        #[source]
        err: git::raw::Error,
    },
}

#[derive(Debug, Error)]
#[error("failed to resolve {name} to its Oid")]
pub struct Resolve {
    pub name: Qualified<'static>,
    #[source]
    pub err: git::raw::Error,
}

#[derive(Debug, Error)]
pub enum Update {
    #[error(transparent)]
    Ancestry(#[from] Ancestry),
    #[error("failed to create reference from {name} to {target}")]
    Create {
        name: Namespaced<'static>,
        target: Oid,
        #[source]
        err: git::raw::Error,
    },
    #[error("failed to delete reference {name}")]
    Delete {
        name: Namespaced<'static>,
        #[source]
        err: git::raw::Error,
    },
    #[error("failed to find ref {name}")]
    Find {
        name: Namespaced<'static>,
        #[source]
        err: git::raw::Error,
    },
    #[error("non-fast-forward update of {name} (current: {cur}, new: {new})")]
    NonFF {
        name: Namespaced<'static>,
        new: Oid,
        cur: Oid,
    },
    #[error("failed to peel ref to object")]
    Peel(#[source] git::raw::Error),
    #[error(transparent)]
    Resolve(#[from] Resolve),

    #[error("refusing to update symbolic ref {name}")]
    Symbolic { name: Namespaced<'static> },
}

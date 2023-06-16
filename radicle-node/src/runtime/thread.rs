use std::thread;

pub use thread::*;

use radicle::prelude::NodeId;

/// Spawn an OS thread.
pub fn spawn<D, F, T>(nid: &NodeId, label: D, f: F) -> thread::JoinHandle<T>
where
    D: std::fmt::Display,
    F: FnOnce() -> T,
    F: Send + 'static,
    T: Send + 'static,
{
    thread::Builder::new()
        .name(name(nid, label))
        .spawn(f)
        .expect("thread::spawn: thread label must not contain NULL bytes")
}

/// Spawn a scoped OS thread.
pub fn spawn_scoped<'scope, 'env, D, F, T>(
    nid: &NodeId,
    label: D,
    scope: &'scope thread::Scope<'scope, 'env>,
    f: F,
) -> thread::ScopedJoinHandle<'scope, T>
where
    D: std::fmt::Display,
    F: FnOnce() -> T,
    F: Send + 'scope,
    T: Send + 'scope,
{
    thread::Builder::new()
        .name(name(nid, label))
        .spawn_scoped(scope, f)
        .expect("thread::spawn_scoped: thread label must not contain NULL bytes")
}

pub fn name<D: std::fmt::Display>(nid: &NodeId, label: D) -> String {
    if cfg!(debug_assertions) {
        format!("{nid} {:<14}", format!("<{label}>"))
    } else {
        format!("{label}")
    }
}

use std::net;

use crate::identity::ProjId;
use crate::storage::{Error, WriteStorage};

/// Default port of the `git` transport protocol.
pub const PROTOCOL_PORT: u16 = 9418;

pub fn fetch<S: WriteStorage>(
    _proj: &ProjId,
    remote: &net::SocketAddr,
    mut storage: S,
) -> Result<(), Error> {
    let _repo = storage.repository();

    let url = format!("git://{}", remote);
    let refs: &[&str] = &[];
    let mut remote = git2::Remote::create_detached(&url)?;
    let mut opts = git2::FetchOptions::default();

    remote.fetch(refs, Some(&mut opts), None)?;

    Ok(())
}

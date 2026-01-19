pub mod git;
pub mod handle;
pub mod policy;
pub mod transport;

pub(crate) mod sigrefs;

mod refs;
mod stage;
mod state;

use std::io;
use std::time::Instant;

use gix_protocol::handshake;

pub use gix_protocol::{transport::bstr::ByteSlice, RemoteProgress};
pub use handle::Handle;
pub use policy::{Allowed, BlockList, Scope};
use radicle::storage::git::Repository;
pub use state::{FetchLimit, FetchResult};
pub use transport::Transport;

use radicle::crypto::PublicKey;
use radicle::storage::refs::RefsAt;
use radicle::storage::ReadRepository as _;
use state::FetchState;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Handshake(Box<HandshakeError>),
    #[error("failed to load `rad/id`")]
    Identity {
        #[source]
        err: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
    #[error(transparent)]
    Protocol(#[from] state::error::Protocol),
    #[error("missing `rad/id`")]
    MissingRadId,
    #[error("attempted to replicate from self")]
    ReplicateSelf,
}

impl From<HandshakeError> for Error {
    fn from(err: HandshakeError) -> Self {
        Self::Handshake(Box::new(err))
    }
}

#[derive(Debug, Error)]
pub enum HandshakeError {
    #[error("failed to perform fetch handshake: {0}")]
    Gix(handshake::Error),
    #[error("an I/O error occurred during the fetch handshake ({0})")]
    Io(io::Error),
}

/// Pull changes from the `remote`.
///
/// It is expected that the local peer has a copy of the repository
/// and is pulling new changes. If the repository does not exist, then
/// [`clone`] should be used.
pub fn pull<R, S>(
    handle: &mut Handle<R, S>,
    limit: FetchLimit,
    remote: PublicKey,
    refs_at: Option<Vec<RefsAt>>,
) -> Result<FetchResult, Error>
where
    R: AsRef<Repository>,
    S: transport::ConnectionStream,
{
    let start = Instant::now();
    let local = *handle.local();
    if local == remote {
        return Err(Error::ReplicateSelf);
    }
    let handshake = perform_handshake(handle)?;
    let state = FetchState::default();

    // N.b. ensure that we ignore the local peer's key.
    handle.blocked.extend([local]);
    let result = state
        .run(handle, &handshake, limit, remote, refs_at)
        .map_err(Error::Protocol);

    log::debug!(
        "Finished pull of {} ({}ms)",
        handle.repository().id(),
        start.elapsed().as_millis()
    );
    result
}

/// Clone changes from the `remote`.
///
/// It is expected that the local peer has an empty repository which
/// they want to populate with the `remote`'s view of the project.
pub fn clone<R, S>(
    handle: &mut Handle<R, S>,
    limit: FetchLimit,
    remote: PublicKey,
) -> Result<FetchResult, Error>
where
    R: AsRef<Repository>,
    S: transport::ConnectionStream,
{
    let start = Instant::now();
    if *handle.local() == remote {
        return Err(Error::ReplicateSelf);
    }
    let handshake = perform_handshake(handle)?;
    let state = FetchState::default();
    let result = state
        .run(handle, &handshake, limit, remote, None)
        .map_err(Error::Protocol);
    let elapsed = start.elapsed().as_millis();
    let rid = handle.repository().id();

    match &result {
        Ok(_) => {
            log::debug!("Finished clone of {rid} from {remote} ({elapsed}ms)",);
        }
        Err(e) => {
            log::debug!("Clone of {rid} from {remote} failed with '{e}' ({elapsed}ms)",);
        }
    }
    result
}

fn perform_handshake<R, S>(handle: &mut Handle<R, S>) -> Result<handshake::Outcome, Error>
where
    S: transport::ConnectionStream,
{
    handle
        .transport
        .handshake()
        .map_err(handle_handshake_err)
        .map_err(Error::from)
}

fn handle_handshake_err(err: handshake::Error) -> HandshakeError {
    match err {
        handshake::Error::Transport(error) => match error {
            gix_transport::client::Error::Io(error) => HandshakeError::Io(error),
            err => HandshakeError::Gix(handshake::Error::Transport(err)),
        },
        err => HandshakeError::Gix(err),
    }
}

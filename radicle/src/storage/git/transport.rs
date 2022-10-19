//! Git sub-transport used for fetching radicle data.
//!
//! To have control over the communication, and to allow git streams to be multiplexed over
//! existing TCP connections, we implement the [`git2::transport::SmartSubtransport`] trait.
//!
//! We choose `rad` as the URL scheme for this custom transport, and include only the identity
//! of the repository we're looking to fetch, eg. `rad://zP1GztjSdYNHK7jpdrXbaJ6Ki2Ke`, since
//! we expect a connection to a host to already be established.
//!
//! We then maintain a map from identifier to stream, for all active streams, ie. streams that
//! are associated with an underlying TCP connection. When a URL is requested, we lookup
//! the stream and return it to the [`git2`] smart-protocol implementation, so that it can carry
//! out the git smart protocol.
//!
//! This module is meant to be used by first registering our transport with [`register`] and then
//! adding or removing streams through [`Smart`], which can be obtained via [`Smart::singleton`].
mod url;

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::atomic;
use std::sync::{Arc, Mutex};

use git2::transport::SmartSubtransportStream;
use once_cell::sync::Lazy;

use crate::git;
use crate::identity::Id;

pub use url::{Url, UrlError};

/// The map of git smart sub-transport streams. We keep a global map because we have
/// no control over how [`git2::transport::register`] instantiates our [`Smart`] transport
/// or its underlying streams.
static STREAMS: Lazy<Arc<Mutex<HashMap<Id, Stream>>>> = Lazy::new(Default::default);

/// The stream associated with a repository.
type Stream = Box<dyn SmartSubtransportStream>;

/// Git transport protocol over an I/O stream.
#[derive(Clone)]
pub struct Smart {
    /// The underlying active streams, keyed by repository identifier.
    streams: Arc<Mutex<HashMap<Id, Stream>>>,
}

impl Smart {
    /// Get access to the radicle smart transport protocol.
    /// The returned object has mutable access to the underlying stream map, and is safe to clone.
    pub fn singleton() -> Self {
        Self {
            streams: STREAMS.clone(),
        }
    }

    /// Take a stream from the map.
    /// This makes the stream unavailable until it is re-inserted.
    pub fn take(&self, id: &Id) -> Option<Stream> {
        #[allow(clippy::unwrap_used)]
        self.streams.lock().unwrap().remove(id)
    }

    pub fn insert(&self, id: Id, stream: Stream) {
        #[allow(clippy::unwrap_used)]
        self.streams.lock().unwrap().insert(id, stream);
    }
}

impl git2::transport::SmartSubtransport for Smart {
    /// Run a git service on this transport.
    ///
    /// Based on the URL, which must be of the form `rad://zP1GztjSdYNHK7jpdrXbaJ6Ki2Ke`,
    /// we retrieve an underlying stream and return it.
    ///
    /// We only support the upload-pack service, since only fetches are authorized by the
    /// remote.
    fn action(
        &self,
        url: &str,
        action: git2::transport::Service,
    ) -> Result<Box<dyn git2::transport::SmartSubtransportStream>, git2::Error> {
        let url = Url::from_str(url).map_err(|e| git2::Error::from_str(e.to_string().as_str()))?;

        if let Some(stream) = self.take(&url.id) {
            match action {
                git2::transport::Service::UploadPackLs => {}
                git2::transport::Service::UploadPack => {}
                git2::transport::Service::ReceivePack => {
                    return Err(git2::Error::from_str(
                        "git-receive-pack is not supported with the custom transport",
                    ));
                }
                git2::transport::Service::ReceivePackLs => {
                    return Err(git2::Error::from_str(
                        "git-receive-pack is not supported with the custom transport",
                    ));
                }
            }
            Ok(stream)
        } else {
            Err(git2::Error::from_str(&format!(
                "repository {} does not have an associated stream",
                url.id
            )))
        }
    }

    fn close(&self) -> Result<(), git2::Error> {
        Ok(())
    }
}

/// Register the radicle transport with `git`.
///
/// Returns an error if called more than once.
///
pub fn register() -> Result<(), git2::Error> {
    static REGISTERED: atomic::AtomicBool = atomic::AtomicBool::new(false);

    // Registration is not thread-safe, so make sure we prevent re-entrancy.
    if !REGISTERED.swap(true, atomic::Ordering::SeqCst) {
        unsafe {
            let prefix = git::url::Scheme::Radicle.to_string();
            git2::transport::register(&prefix, move |remote| {
                git2::transport::Transport::smart(remote, false, Smart::singleton())
            })
        }
    } else {
        Err(git2::Error::from_str(
            "custom git transport is already registered",
        ))
    }
}

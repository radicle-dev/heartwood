//! Git sub-transport used for fetching radicle data.
//!
//! To have control over the communication, and to allow git streams to be multiplexed over
//! existing TCP connections, we implement the [`git2::transport::SmartSubtransport`] trait.
//!
//! We choose `heartwood` as the URL scheme for this custom transport, and include the node we'd
//! like to fetch from, as well as the repository. We expect the TCP stream to already be
//! established when this transport is called.
//!
//! We then maintain a map from node identifier to stream, for all active TCP connections. When a
//! URL is requested, we lookup the associated stream and return it to the [`git2`] smart-protocol
//! implementation, so that it can carry out the git smart protocol.
//!
//! This module is meant to be used by registering streams with [`register`].
pub mod mock;
pub mod url;

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Mutex;
use std::sync::Once;

use git2::transport::SmartSubtransportStream;
use once_cell::sync::Lazy;

use crate::storage::RemoteId;

pub use url::{Url, UrlError};

/// The map of git smart sub-transport streams. We keep a global map because we have
/// no control over how [`git2::transport::register`] instantiates our [`Smart`] transport
/// or its underlying streams.
static STREAMS: Lazy<Mutex<HashMap<RemoteId, Stream>>> = Lazy::new(Default::default);

/// The stream associated with a repository.
type Stream = Box<dyn SmartSubtransportStream>;

/// Git transport protocol over an I/O stream.
#[derive(Clone)]
struct Smart;

impl git2::transport::SmartSubtransport for Smart {
    /// Run a git service on this transport.
    ///
    /// Based on the URL, which must be a valid [`Url`],
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
        let mut streams = STREAMS.lock().expect("lock isn't poisoned");

        if let Some(stream) = streams.remove(&url.node) {
            match action {
                git2::transport::Service::UploadPackLs | git2::transport::Service::UploadPack => {}
                git2::transport::Service::ReceivePack | git2::transport::Service::ReceivePackLs => {
                    return Err(git2::Error::from_str(
                        "git-receive-pack is not supported with the custom transport",
                    ));
                }
            }
            Ok(stream)
        } else {
            Err(git2::Error::from_str(&format!(
                "node {} does not have an associated stream",
                url.node
            )))
        }
    }

    fn close(&self) -> Result<(), git2::Error> {
        Ok(())
    }
}

/// Register the radicle transport with `git`.
///
/// This function can be called more than once. Only one transport will be registered.
///
pub fn register(node: RemoteId, stream: impl SmartSubtransportStream) {
    static REGISTER: Once = Once::new();

    // Registration is not thread-safe, so make sure we prevent re-entrancy.
    REGISTER.call_once(|| unsafe {
        git2::transport::register(Url::SCHEME, move |remote| {
            git2::transport::Transport::smart(remote, false, Smart)
        })
        .expect("remote transport registration");
    });

    STREAMS
        .lock()
        .expect("lock isn't poisoned")
        .insert(node, Box::new(stream));
}

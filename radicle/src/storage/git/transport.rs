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
//! adding or removing streams through [`Smart`], which can be obtained by calling [`smart`].
use std::collections::HashMap;
use std::io;
use std::str::FromStr;
use std::sync::atomic;
use std::sync::{Arc, Mutex};

use crossbeam_channel as chan;
use once_cell::sync::Lazy;

use crate::git;
use crate::identity::Id;

/// The map of git smart sub-transport streams. We keep a global map because we have
/// no control over how [`git2::transport::register`] instantiates our [`Smart`] transport
/// or its underlying streams.
static STREAMS: Lazy<Arc<Mutex<HashMap<Id, Stream>>>> = Lazy::new(Default::default);

/// Git transport protocol over an I/O stream.
#[derive(Clone)]
pub struct Smart {
    /// The underlying active streams, keyed by repository identifier.
    streams: Arc<Mutex<HashMap<Id, Stream>>>,
}

impl Smart {
    pub fn get(&self, id: &Id) -> Option<Stream> {
        self.streams.lock().unwrap().get(id).cloned()
    }

    pub fn insert(&self, id: Id, stream: Stream) {
        self.streams.lock().unwrap().insert(id, stream);
    }

    pub fn remove(&self, id: &Id) {
        self.streams.lock().unwrap().remove(id);
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
        let url = git::Url::from_bytes(url.as_bytes())
            .map_err(|e| git2::Error::from_str(e.to_string().as_str()))?;
        let id = Id::from_str(url.host.unwrap_or_default().as_str())
            .map_err(|_| git2::Error::from_str("Git URL does not contain a valid project id"))?;

        if url.scheme != git::url::Scheme::Radicle {
            return Err(git2::Error::from_str("Git URL scheme must be `rad`"));
        }

        if let Some(stream) = self.get(&id) {
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
            Ok(Box::new(stream))
        } else {
            Err(git2::Error::from_str(&format!(
                "repository {id} does not have an associated stream"
            )))
        }
    }

    fn close(&self) -> Result<(), git2::Error> {
        Ok(())
    }
}

/// A byte stream connected to some I/O source.
/// One of these is created for every git operation, eg. `fetch`.
#[derive(Clone)]
pub struct Stream {
    /// Send bytes to the network.
    send: chan::Sender<Vec<u8>>,
    /// Receive bytes from the network.
    recv: chan::Receiver<Vec<u8>>,
    /// Bytes read from the receive channel that didn't fit in the read buffer.
    pending: Vec<u8>,
}

impl Stream {
    /// Create a new stream from a sender and receiver.
    pub fn new(send: chan::Sender<Vec<u8>>, recv: chan::Receiver<Vec<u8>>) -> Self {
        Self {
            send,
            recv,
            pending: Vec::new(),
        }
    }
}

impl io::Write for Stream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.send
            .send(buf.to_owned())
            .map_err(|e| io::Error::new(io::ErrorKind::BrokenPipe, e))?;

        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl io::Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let bytes = self
            .recv
            .recv()
            .map_err(|e| io::Error::new(io::ErrorKind::BrokenPipe, e))?;
        self.pending.extend(&bytes);

        // There must be a nicer way to do this...
        let count = buf.len().min(self.pending.len());
        buf[..count].copy_from_slice(&self.pending[..count]);
        self.pending.drain(..count);

        Ok(count)
    }
}

/// Register the radicle transport with `git`.
///
/// Returns an error if called more than once.
///
pub fn register(prefix: &str) -> Result<(), git2::Error> {
    static REGISTERED: atomic::AtomicBool = atomic::AtomicBool::new(false);

    // Registration is not thread-safe, so make sure we prevent re-entrancy.
    if !REGISTERED.swap(true, atomic::Ordering::SeqCst) {
        unsafe {
            git2::transport::register(prefix, move |remote| {
                git2::transport::Transport::smart(remote, false, self::smart())
            })
        }
    } else {
        Err(git2::Error::from_str(
            "custom git transport is already registered",
        ))
    }
}

/// Get access to the radicle smart transport protocol.
///
/// The returned object has mutable access to the underlying stream map, and is safe to clone.
///
pub fn smart() -> Smart {
    Smart {
        streams: STREAMS.clone(),
    }
}

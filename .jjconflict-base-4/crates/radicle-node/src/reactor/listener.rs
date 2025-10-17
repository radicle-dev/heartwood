use mio::event::{Event, Source};
use mio::net::{TcpListener, TcpStream};
use mio::{Interest, Registry, Token};
use std::io::Result;

use std::net::SocketAddr;
use std::time::Duration;

use crate::reactor::EventHandler;

/// A reactor-manageable TCP listener which can
/// be aware of additional encryption, authentication and other forms of
/// transport-layer protocols which will be automatically injected into accepted
/// connections.
#[derive(Debug)]
pub struct Listener(TcpListener);

impl Source for Listener {
    fn register(&mut self, registry: &Registry, token: Token, interests: Interest) -> Result<()> {
        self.0.register(registry, token, interests)
    }

    fn reregister(&mut self, registry: &Registry, token: Token, interests: Interest) -> Result<()> {
        self.0.reregister(registry, token, interests)
    }

    fn deregister(&mut self, registry: &Registry) -> Result<()> {
        self.0.deregister(registry)
    }
}

impl Listener {
    pub fn bind(addr: SocketAddr) -> Result<Self> {
        Ok(Self(TcpListener::bind(addr)?))
    }

    /// Returns the local [`std::net::SocketAddr`] on which self accepts
    /// connections.
    pub fn local_addr(&self) -> std::net::SocketAddr {
        self.0.local_addr().expect("TCP listener has local address")
    }

    fn accept(&mut self) -> Result<(TcpStream, SocketAddr)> {
        /// Maximum time to wait when reading from a socket.
        const READ_TIMEOUT: Duration = Duration::from_secs(6);

        /// Maximum time to wait when writing to a socket.
        const WRITE_TIMEOUT: Duration = Duration::from_secs(3);

        let (stream, peer) = self.0.accept()?;
        let stream = std::net::TcpStream::from(stream);
        stream.set_read_timeout(Some(READ_TIMEOUT))?;
        stream.set_write_timeout(Some(WRITE_TIMEOUT))?;
        stream.set_nonblocking(true)?;
        Ok((TcpStream::from_std(stream), peer))
    }
}

impl EventHandler for Listener {
    type Reaction = Result<(TcpStream, SocketAddr)>;

    fn interests(&self) -> Option<Interest> {
        Some(Interest::READABLE)
    }

    fn handle(&mut self, event: &Event) -> Vec<Self::Reaction> {
        if !event.is_readable() {
            return vec![];
        }

        vec![self.accept()]
    }
}

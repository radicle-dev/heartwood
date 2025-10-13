use std::error;
use std::fmt::{Debug, Display};
use std::io;
use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr};

use cyphernet::encrypt::noise::NoiseState;
use cyphernet::proxy::socks5;

use mio::event::Source;
use mio::net::TcpStream;
use mio::{Interest, Registry, Token};

pub type NoiseSession<E, D, S> = Protocol<NoiseState<E, D>, S>;
pub type Socks5Session<S> = Protocol<socks5::Socks5, S>;

pub trait Session: Send + Read + Write {
    type Inner: Session;
    type Artifact: Display;

    fn is_established(&self) -> bool {
        self.artifact().is_some()
    }

    fn run_handshake(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn display(&self) -> String {
        self.artifact()
            .map(|artifact| artifact.to_string())
            .unwrap_or_else(|| "<no-id>".to_string())
    }

    fn artifact(&self) -> Option<Self::Artifact>;

    fn stream(&mut self) -> &mut TcpStream;

    fn disconnect(self) -> io::Result<()>;
}

pub trait StateMachine: Sized + Send {
    const NAME: &'static str;

    type Artifact;

    type Error: error::Error + Send + Sync + 'static;

    fn next_read_len(&self) -> usize;

    fn advance(&mut self, input: &[u8]) -> Result<Vec<u8>, Self::Error>;

    fn artifact(&self) -> Option<Self::Artifact>;

    // Blocking
    fn run_handshake<RW>(&mut self, stream: &mut RW) -> io::Result<()>
    where
        RW: Read + Write,
    {
        let mut input = vec![];
        while !self.is_complete() {
            let act = self.advance(&input).map_err(|err| {
                log::error!(target: Self::NAME, "Handshake failure: {err}");
                io::Error::other(err)
            })?;
            if !act.is_empty() {
                log::trace!(target: Self::NAME, "Sending handshake act {act:02x?}");

                stream.write_all(&act)?;
            }
            if !self.is_complete() {
                input = vec![0u8; self.next_read_len()];
                stream.read_exact(&mut input)?;

                log::trace!(target: Self::NAME, "Receiving handshake act {input:02x?}");
            }
        }

        log::debug!(target: Self::NAME, "Handshake protocol {} successfully completed", Self::NAME);
        Ok(())
    }

    fn is_complete(&self) -> bool {
        self.artifact().is_some()
    }
}

#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct ProtocolArtifact<M: StateMachine, S: Session> {
    pub(crate) session: S::Artifact,
    pub(crate) state: M::Artifact,
}

impl<M: StateMachine, S: Session> Display for ProtocolArtifact<M, S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProtocolArtifact")
            .field("session", &"<omitted>")
            .field("state", &"<omitted>")
            .finish()
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct Protocol<M: StateMachine, S: Session> {
    pub(crate) state: M,
    pub(crate) session: S,
}

impl<M: StateMachine, S: Session> Protocol<M, S> {
    pub fn new(session: S, state_machine: M) -> Self {
        Self {
            state: state_machine,
            session,
        }
    }
}

impl<M: StateMachine, S: Session> io::Read for Protocol<M, S> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        log::trace!(target: M::NAME, "Reading event");

        if self.state.is_complete() || !self.session.is_established() {
            log::trace!(target: M::NAME, "Passing reading to inner not yet established session");
            return self.session.read(buf);
        }

        let len = self.state.next_read_len();
        let mut input = vec![0u8; len];
        self.session.read_exact(&mut input)?;

        log::trace!(target: M::NAME, "Received handshake act: {input:02x?}");

        if !input.is_empty() {
            let output = self.state.advance(&input).map_err(|err| {
                log::error!(target: M::NAME, "Handshake failure: {err}");
                io::Error::other(err)
            })?;

            if !output.is_empty() {
                log::trace!(target: M::NAME, "Sending handshake act on read: {output:02x?}");
                self.session.write_all(&output)?;
            }
        }

        Ok(0)
    }
}

impl<M: StateMachine, S: Session> Write for Protocol<M, S> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        log::trace!(target: M::NAME, "Writing event (state_complete={}, session_established={})", self.state.is_complete(), self.session.is_established());

        if self.state.is_complete() || !self.session.is_established() {
            log::trace!(target: M::NAME, "Passing writing to inner session");
            return self.session.write(buf);
        }

        if self.state.next_read_len() == 0 {
            log::trace!(target: M::NAME, "Starting handshake protocol");

            let act = self.state.advance(&[]).map_err(|err| {
                log::error!(target: M::NAME, "Handshake failure: {err}");
                io::Error::other(err)
            })?;

            if !act.is_empty() {
                log::trace!(target: M::NAME, "Sending handshake act on write: {act:02x?}");
                self.session.write_all(&act)?;
            } else {
                log::trace!(target: M::NAME, "Handshake complete, passing data to inner session");
                return self.session.write(buf);
            }
        }

        if buf.is_empty() {
            Ok(0)
        } else {
            Err(io::ErrorKind::Interrupted.into())
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        self.session.flush()
    }
}

impl<M: StateMachine, S: Session> Session for Protocol<M, S> {
    type Inner = S;
    type Artifact = ProtocolArtifact<M, S>;

    fn run_handshake(&mut self) -> io::Result<()> {
        log::debug!(target: M::NAME, "Starting handshake protocol {}", M::NAME);

        if !self.session.is_established() {
            self.session.run_handshake()?;
        }

        self.state.run_handshake(self.session.stream())
    }

    fn artifact(&self) -> Option<Self::Artifact> {
        Some(ProtocolArtifact {
            session: self.session.artifact()?,
            state: self.state.artifact()?,
        })
    }

    fn stream(&mut self) -> &mut TcpStream {
        self.session.stream()
    }

    fn disconnect(self) -> io::Result<()> {
        self.session.disconnect()
    }
}

impl<M: StateMachine, S: Session + Source> Source for Protocol<M, S> {
    fn register(
        &mut self,
        registry: &Registry,
        token: Token,
        interests: Interest,
    ) -> io::Result<()> {
        self.session.register(registry, token, interests)
    }

    fn reregister(
        &mut self,
        registry: &Registry,
        token: Token,
        interests: Interest,
    ) -> io::Result<()> {
        self.session.reregister(registry, token, interests)
    }

    fn deregister(&mut self, registry: &Registry) -> io::Result<()> {
        self.session.deregister(registry)
    }
}

impl Session for TcpStream {
    type Inner = Self;
    type Artifact = SocketAddr;

    fn artifact(&self) -> Option<Self::Artifact> {
        self.peer_addr().ok()
    }

    fn stream(&mut self) -> &mut TcpStream {
        self
    }

    fn disconnect(self) -> io::Result<()> {
        self.shutdown(Shutdown::Both)
    }
}

mod impl_noise {
    use cyphernet::encrypt::noise::{error::NoiseError as Error, NoiseState as Noise};
    use cyphernet::{Digest, Ecdh};

    use super::*;

    #[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
    pub struct NoiseArtifact<E: Ecdh, D: Digest> {
        pub handshake_hash: D::Output,
        pub remote_static_key: Option<E::Pk>,
    }

    impl<E: Ecdh, D: Digest> StateMachine for Noise<E, D> {
        const NAME: &'static str = "noise";
        type Artifact = NoiseArtifact<E, D>;
        type Error = Error;

        fn next_read_len(&self) -> usize {
            self.next_read_len()
        }

        fn advance(&mut self, input: &[u8]) -> Result<Vec<u8>, Self::Error> {
            self.advance(input)
        }

        fn artifact(&self) -> Option<Self::Artifact> {
            self.get_handshake_hash().map(|hh| NoiseArtifact {
                handshake_hash: hh,
                remote_static_key: self.get_remote_static_key(),
            })
        }
    }
}

mod impl_socks5 {
    use cyphernet::addr::{Host as _, HostName, NetAddr};
    use cyphernet::proxy::socks5::{Error, Socks5};

    use super::*;

    impl StateMachine for Socks5 {
        const NAME: &'static str = "socks5";

        type Artifact = NetAddr<HostName>;
        type Error = Error;

        fn next_read_len(&self) -> usize {
            self.next_read_len()
        }

        fn advance(&mut self, input: &[u8]) -> Result<Vec<u8>, Self::Error> {
            self.advance(input)
        }

        fn artifact(&self) -> Option<Self::Artifact> {
            match self {
                Socks5::Initial(addr, false) if !addr.requires_proxy() => Some(addr.clone()),
                Socks5::Active(addr) => Some(addr.clone()),
                _ => None,
            }
        }
    }
}

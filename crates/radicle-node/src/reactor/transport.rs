use std::collections::VecDeque;
use std::fmt::{Debug, Display, Formatter};
use std::io::Write;
use std::{fmt, io};

use mio::event::{Event, Source};
use mio::{Interest, Registry, Token};
use radicle::node::Link;

use crate::reactor::session::Session;
use crate::reactor::{EventHandler, WriteAtomic};

const READ_BUFFER_SIZE: usize = u16::MAX as usize;

/// An event happening for a [`Transport`] network transport and delivered to
/// a [`ReactionHandler`].
///
/// [`ReactionHandler`]: crate::reactor::ReactionHandler
pub enum SessionEvent<S: Session> {
    Established(S::Artifact),
    Data(Vec<u8>),
    Terminated(io::Error),
}

/// A state of [`Transport`] network transport.
#[derive(Clone, Copy, Ord, PartialOrd, Eq, PartialEq, Hash, Debug)]
pub enum TransportState {
    /// The transport is initiated, but the connection has not been established yet.
    /// This happens only for outgoing connections due to the use of
    /// non-blocking calls to `connect`. The state changes once
    /// we receive the first notification on a `write` event on this resource
    /// from the reactor.
    Init,

    /// The connection is established, but the session handshake is still in
    /// progress. This happens while encryption handshake, authentication and
    /// other protocols injected into the session haven't completed yet.
    Handshake,

    /// The session is active. All handshakes have completed.
    Active,

    /// Session was terminated (for an unspecified reason, e.g. local shutdown,
    /// remote orderly shutdown, connectivity issue, dropped connections,
    /// encryption, or authentication problem etc.
    /// Reading and writing from the resource in
    /// this state will result in an error ([`io::Error`]).
    Terminated,
}

/// Transport is an adaptor around a specific [`Session`] (implementing
/// session management, including optional handshake, encoding, etc.) to be used
/// as a transport resource in a [`crate::reactor::Reactor`].
pub struct Transport<S: Session> {
    state: TransportState,
    session: S,
    link_direction: Link,
    write_intent: bool,
    read_buffer: Box<[u8; READ_BUFFER_SIZE]>,
    write_buffer: VecDeque<u8>,
}

impl<S: Session> std::fmt::Debug for Transport<S> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("Transport")
            .field("session", &self.session.display())
            .field("state", &self.state)
            .field("link_direction", &self.link_direction)
            .field("write_intent", &self.write_intent)
            .finish()
    }
}

impl<S: Session + Source> Source for Transport<S> {
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

impl<S: Session> Display for Transport<S> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self.session.artifact() {
            None => f
                .debug_struct("Transport")
                .field("state", &self.state)
                .field("link_direction", &self.link_direction)
                .field("write_intent", &self.write_intent)
                .finish(),
            Some(id) => Display::fmt(&id, f),
        }
    }
}

impl<S: Session> Transport<S> {
    /// Constructs reactor-managed resource around an existing [`Session`].
    ///
    /// Must not be called for connections created in a non-blocking mode!
    ///
    /// # Errors
    ///
    /// If a session can be put into a non-blocking mode.
    pub fn with_session(session: S, link_direction: Link) -> io::Result<Self> {
        let state = if session.is_established() {
            // If we are disconnected, we will get instantly updated from the
            // reactor and the state will change automatically
            TransportState::Active
        } else {
            TransportState::Handshake
        };
        Ok(Self {
            state,
            session,
            link_direction,
            write_intent: true,
            read_buffer: Box::new([0u8; READ_BUFFER_SIZE]),
            write_buffer: VecDeque::new(),
        })
    }

    pub fn display(&self) -> impl Display {
        self.session.display()
    }

    fn terminate(&mut self, reason: io::Error) -> SessionEvent<S> {
        log::trace!(target: "transport", "Terminating session {self} due to {reason:?}");

        self.state = TransportState::Terminated;
        SessionEvent::Terminated(reason)
    }

    fn handle_io(&mut self, interest: Interest) -> Option<SessionEvent<S>> {
        if self.state == TransportState::Terminated {
            log::warn!(target: "transport", "Transport {self} is terminated, ignoring I/O event");
            return None;
        }

        let mut force_write_intent = false;
        if self.state == TransportState::Init {
            log::debug!(target: "transport", "Transport {self} is connected, initializing handshake");

            force_write_intent = true;
            self.state = TransportState::Handshake;
        } else if self.state == TransportState::Handshake {
            debug_assert!(!self.session.is_established());

            log::trace!(target: "transport", "Transport {self} got I/O while in handshake mode");
        }

        let resp = match interest {
            Interest::READABLE => self.handle_readable(),
            Interest::WRITABLE => self.handle_writable(),
            _ => unreachable!(),
        };

        if force_write_intent {
            self.write_intent = true;
        } else if self.state == TransportState::Handshake {
            // During handshake, after each read we need to write and then wait
            self.write_intent = interest == Interest::READABLE;
        }

        if matches!(&resp, Some(SessionEvent::Terminated(e)) if e.kind() == io::ErrorKind::ConnectionReset)
            && self.state != TransportState::Handshake
        {
            log::debug!(target: "transport", "Peer {self} has reset the connection");

            self.state = TransportState::Terminated;
            resp
        } else if self.session.is_established() && self.state == TransportState::Handshake {
            log::debug!(target: "transport", "Handshake with {self} is complete");

            // We just got connected; may need to send output
            self.write_intent = true;
            self.state = TransportState::Active;
            Some(SessionEvent::Established(
                self.session.artifact().expect("session is established"),
            ))
        } else {
            resp
        }
    }

    fn handle_writable(&mut self) -> Option<SessionEvent<S>> {
        if !self.session.is_established() {
            let _ = self.session.write(&[]);
            self.write_intent = true;
            return None;
        }
        match self.flush() {
            Ok(_) => None,
            // In this case, the write could not complete. Leave `needs_flush` set
            // to be notified when the socket is ready to write again.
            Err(err)
                if matches!(
                    err.kind(),
                    io::ErrorKind::WouldBlock
                        | io::ErrorKind::WriteZero
                        | io::ErrorKind::OutOfMemory
                        | io::ErrorKind::Interrupted
                ) =>
            {
                log::warn!(target: "transport", "Resource {} was not able to consume any data even though it has announced its write readiness", self.display());
                self.write_intent = true;
                None
            }
            Err(err) => Some(self.terminate(err)),
        }
    }

    fn handle_readable(&mut self) -> Option<SessionEvent<S>> {
        // Since `poll`, which this reactor is based on, is *level-triggered*,
        // we will be notified again if there is still data to be read on the socket.
        // Hence, there is no use in putting this socket read in a loop, as the second
        // invocation would likely block.
        match self.session.read(self.read_buffer.as_mut()) {
            Ok(0) if !self.session.is_established() => None,
            Ok(0) => Some(SessionEvent::Terminated(
                io::ErrorKind::ConnectionReset.into(),
            )),
            Ok(len) => Some(SessionEvent::Data(self.read_buffer[..len].to_vec())),
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                // This should not happen, since this function is only called
                // when there's data on the socket. We leave it here in case external
                // conditions change.

                log::warn!(target: "transport",
                    "WOULD_BLOCK on resource which had read intent - probably normal thing to happen"
                );
                None
            }
            Err(err) => Some(self.terminate(err)),
        }
    }

    fn flush_buffer(&mut self) -> io::Result<()> {
        let orig_len = self.write_buffer.len();

        log::trace!(target: "transport", "Resource {} is flushing its buffer of {orig_len} bytes", self.display());
        let len =
            self.session.write(self.write_buffer.make_contiguous()).or_else(|err| {
                match err.kind() {
                    io::ErrorKind::WouldBlock
                    | io::ErrorKind::OutOfMemory
                    | io::ErrorKind::WriteZero
                    | io::ErrorKind::Interrupted => {
                        log::warn!(target: "transport", "Resource {} kernel buffer is full (system message is '{err}')", self.display());
                        Ok(0)
                    },
                    _ => {
                        log::error!(target: "transport", "Resource {} failed write operation with message '{err}'", self.display());
                        Err(err)
                    },
                }
            })?;
        if orig_len > len {
            log::debug!(target: "transport", "Resource {} was able to consume only a part of the buffered data ({len} of {orig_len} bytes)", self.display());
            self.write_intent = true;
        } else {
            log::trace!(target: "transport", "Resource {} was able to consume all of the buffered data ({len} of {orig_len} bytes)", self.display());
            self.write_intent = false;
        }
        self.write_buffer.drain(..len);
        Ok(())
    }
}

impl<S: Session + Source> EventHandler for Transport<S> {
    type Reaction = SessionEvent<S>;

    fn interests(&self) -> Option<Interest> {
        use mio::Interest;
        use TransportState::*;

        match self.state {
            Init => Some(Interest::WRITABLE),
            Active | Handshake if self.write_intent => {
                Some(Interest::READABLE | Interest::WRITABLE)
            }
            Active | Handshake => Some(Interest::READABLE),
            Terminated => None,
        }
    }

    fn handle(&mut self, event: &Event) -> Vec<Self::Reaction> {
        let mut events = Vec::with_capacity(2);
        if event.is_writable() {
            if let Some(event) = self.handle_io(Interest::WRITABLE) {
                events.push(event);
            }
        }
        if event.is_readable() {
            if let Some(event) = self.handle_io(Interest::READABLE) {
                events.push(event);
            }
        }
        events
    }
}

impl<S: Session> Write for Transport<S> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.write_atomic(buf).map(|_| buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        let res = self.flush_buffer();
        self.session.flush().and(res)
    }
}

impl<S: Session> WriteAtomic for Transport<S> {
    fn is_ready_to_write(&self) -> bool {
        self.state == TransportState::Active
    }

    fn write_or_buf(&mut self, buf: &[u8]) -> io::Result<()> {
        if buf.is_empty() {
            return Ok(());
        }
        self.write_buffer.extend(buf);
        self.flush_buffer()
    }
}

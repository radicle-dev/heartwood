use std::collections::VecDeque;
use std::error;
use std::fmt::{Debug, Display};
use std::io;
use std::io::{Read, Write};
use std::net::SocketAddr;

use cyphernet::proxy::socks5;

use mio::event::Source;
use mio::net::TcpStream;
use mio::{Interest, Registry, Token};

pub type NoiseSession<E, D, S> = Protocol<impl_noise::NoiseTransport<E, D>, S>;
pub type Socks5Session<S> = Protocol<socks5::Socks5, S>;
pub use impl_noise::NoiseTransport;

// SECURITY FIX (local patch, not upstream): post-handshake traffic was
// previously passed straight through to the raw socket with zero
// involvement from the Noise state machine -- see `encrypt_transport`/
// `decrypt_transport` below and their use in `Protocol::read`/`write`.
//
// `NoiseState::Active`'s `sending_cipher`/`receiving_cipher` are NOT
// already role-correct -- checked this directly in cyphernet 0.5.4's
// `noise::state` source rather than assuming it. Both places that produce
// `HandshakeAct::Split` (the tail end of `write_message` and of
// `read_message`) call the same `SymmetricState::split()`, which returns
// `(c1, c2)` in the fixed Noise-spec order regardless of which party is
// calling, and that pair is destructured straight into
// `Active { sending_cipher: c1, receiving_cipher: c2, .. }` either way. Per
// the spec, `c1` is always initiator-to-responder and `c2` is always
// responder-to-initiator, so the responder's actual send cipher is `c2`
// (cyphernet's `receiving_cipher` field) and its actual receive cipher is
// `c1` (cyphernet's `sending_cipher` field) -- the opposite of what the
// field names say. `NoiseState` has no way to apply this itself: the
// `is_initiator` flag lives on the inner handshake state, which is dropped
// once it reaches `Active`. `NoiseTransport` below exists to carry that
// flag across and do the swap on the responder side; skipping it is what
// produced "Transport decryption failed: ChaCha20Poly1305 AEAD encryptor
// error" on literally the first transport frame in testing.
//
// Wire format for transport-phase frames, length-prefixed because AEAD
// operates on discrete messages but TCP has none:
//   [4-byte big-endian ciphertext length][ciphertext (= plaintext + 16-byte
//   Poly1305 tag)]
// This is a breaking wire change versus the public Radicle network --
// intentional. This patch is for a closed group of nodes that all run it,
// not for interop with unpatched peers.
const FRAME_LEN_PREFIX: usize = 4;

pub trait Session: Send + Read + Write {
    type Artifact: Display;

    fn is_established(&self) -> bool {
        self.artifact().is_some()
    }

    fn artifact(&self) -> Option<Self::Artifact>;

    /// Whether this session has already-processed bytes buffered
    /// internally that still need to reach the raw socket. Needed because
    /// `Protocol`'s encrypted write path cannot safely report "0 bytes
    /// consumed" to ask its caller to retry with the same plaintext --
    /// re-encrypting identical plaintext after a partial flush would
    /// advance the AEAD nonce again and corrupt whatever ciphertext bytes
    /// already reached the kernel send buffer from the first attempt. So
    /// a partially-flushed frame must be buffered locally instead -- but
    /// `Transport`'s flow-control (`write_intent`) only tracks its OWN
    /// buffer by default, and would stop asking mio for further
    /// `WRITABLE` events once that drains, even if bytes are still stuck
    /// here. This lets that check see this layer's hidden backlog too, so
    /// a connection under write backpressure can't silently stall
    /// mid-frame. Default `false` matches every session that doesn't do
    /// its own internal buffering (e.g. a raw `TcpStream`).
    fn has_pending_write(&self) -> bool {
        false
    }
}

pub trait StateMachine: Sized + Send {
    const NAME: &'static str;

    type Artifact;

    type Error: error::Error + Send + Sync + 'static;

    fn next_read_len(&self) -> usize;

    fn advance(&mut self, input: &[u8]) -> Result<Vec<u8>, Self::Error>;

    fn artifact(&self) -> Option<Self::Artifact>;

    fn is_complete(&self) -> bool {
        self.artifact().is_some()
    }

    /// Whether this state machine provides transport-phase confidentiality
    /// and authenticity once the handshake is complete (true for Noise,
    /// false for Socks5, which isn't a security protocol). Side-effect-free
    /// by design: callers must be able to check this WITHOUT invoking
    /// `encrypt_transport`/`decrypt_transport`, since those consume a nonce
    /// on every call (even for an empty message) -- probing capability by
    /// calling them would desynchronize the sender's and receiver's nonce
    /// counters, which is exactly the class of subtle bug this patch exists
    /// to avoid introducing.
    fn has_transport_crypto(&self) -> bool {
        false
    }

    /// Encrypt one post-handshake application message for transport.
    /// `None` means this state machine provides no transport-phase
    /// confidentiality/authenticity (e.g. Socks5, which isn't a security
    /// protocol) -- callers fall back to raw passthrough in that case,
    /// identical to this patch's pre-existing behavior for such sessions.
    fn encrypt_transport(&mut self, _plaintext: &[u8]) -> Option<Result<Vec<u8>, Self::Error>> {
        None
    }

    /// Decrypt one post-handshake application message received from the
    /// transport. See `encrypt_transport`.
    fn decrypt_transport(&mut self, _ciphertext: &[u8]) -> Option<Result<Vec<u8>, Self::Error>> {
        None
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

#[derive(Clone, Eq, PartialEq, Debug)]
pub struct Protocol<M: StateMachine, S: Session> {
    pub(crate) machine: M,
    pub(crate) session: S,
    /// Decrypted application bytes already produced but not yet returned to
    /// the caller of `read()` -- needed because one decrypted transport
    /// frame can be larger than the buffer a single `read()` call provides.
    read_plaintext: VecDeque<u8>,
    /// Raw bytes read from the underlying socket that don't yet form a
    /// complete length-prefixed ciphertext frame.
    read_raw: Vec<u8>,
    /// An already-encrypted, length-prefixed frame (or the unsent tail of
    /// one) that hasn't been fully written to the underlying socket yet.
    /// `write_pending[write_pending_sent..]` is what remains to send. Once
    /// a frame is encrypted its nonce is spent -- this must be fully
    /// flushed before any new plaintext is encrypted, never re-derived.
    write_pending: Vec<u8>,
    write_pending_sent: usize,
}

impl<M: StateMachine, S: Session> Protocol<M, S> {
    pub fn new(session: S, machine: M) -> Self {
        Self {
            machine,
            session,
            read_plaintext: VecDeque::new(),
            read_raw: Vec::new(),
            write_pending: Vec::new(),
            write_pending_sent: 0,
        }
    }

    /// Push as much of `write_pending[write_pending_sent..]` to the raw
    /// socket as it will currently accept. Returns `Ok(())` whether or not
    /// everything was sent -- callers check `write_pending.is_empty()`
    /// afterward to know if more remains. Only an error OTHER than
    /// WouldBlock/Interrupted is surfaced, matching how the rest of this
    /// file treats those two as "try again later", not failures.
    fn flush_pending(&mut self) -> io::Result<()> {
        while self.write_pending_sent < self.write_pending.len() {
            match self
                .session
                .write(&self.write_pending[self.write_pending_sent..])
            {
                Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
                Ok(n) => self.write_pending_sent += n,
                // Interrupted means the syscall was interrupted by a signal
                // and should be retried immediately (the `while` loop does
                // that on its own) -- NOT the same as WouldBlock, which
                // means genuinely stop and wait for a future writable
                // event. Matches how `handle_readable` elsewhere in this
                // codebase already distinguishes the two.
                Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => return Ok(()),
                Err(e) => return Err(e),
            }
        }
        self.write_pending.clear();
        self.write_pending_sent = 0;
        Ok(())
    }

    /// Post-handshake `read()` for a state machine that provides transport
    /// crypto. Pulls whatever raw bytes are currently available, decrypts
    /// every complete length-prefixed frame found, and serves plaintext
    /// out of `read_plaintext` -- carrying over anything that didn't fit in
    /// `buf` to the NEXT call rather than discarding it. The reactor's
    /// `handle_readable` loop (see `reactor/transport.rs`) calls `read()`
    /// repeatedly until it sees `WouldBlock`, so leftover buffered
    /// plaintext here is guaranteed to be drained on an immediate follow-up
    /// call, not stranded.
    fn read_encrypted(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if !self.read_plaintext.is_empty() {
            return Ok(drain_into(&mut self.read_plaintext, buf));
        }

        let mut tmp = [0u8; 8192];
        loop {
            match self.session.read(&mut tmp) {
                Ok(0) => break, // peer closed, or nothing more buffered right now
                Ok(n) => self.read_raw.extend_from_slice(&tmp[..n]),
                Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) => return Err(e),
            }
        }

        while self.read_raw.len() >= FRAME_LEN_PREFIX {
            let len_bytes: [u8; FRAME_LEN_PREFIX] = self.read_raw[..FRAME_LEN_PREFIX]
                .try_into()
                .expect("slice length matches FRAME_LEN_PREFIX");
            let len = u32::from_be_bytes(len_bytes) as usize;

            if self.read_raw.len() < FRAME_LEN_PREFIX + len {
                break; // frame hasn't fully arrived yet
            }

            let ciphertext: Vec<u8> = self
                .read_raw
                .drain(..FRAME_LEN_PREFIX + len)
                .skip(FRAME_LEN_PREFIX)
                .collect();

            match self.machine.decrypt_transport(&ciphertext) {
                Some(Ok(plaintext)) => self.read_plaintext.extend(plaintext),
                Some(Err(err)) => {
                    log::error!(target: M::NAME, "Transport decryption failed: {err}");
                    return Err(io::Error::other(err));
                }
                None => unreachable!("has_transport_crypto() returned true for this machine"),
            }
        }

        if !self.read_plaintext.is_empty() {
            return Ok(drain_into(&mut self.read_plaintext, buf));
        }

        Err(io::ErrorKind::WouldBlock.into())
    }

    /// Post-handshake `write()` for a state machine that provides
    /// transport crypto. Finishes flushing any previously-encrypted,
    /// not-yet-fully-sent frame FIRST -- while that's non-empty, new
    /// plaintext is never accepted, because encrypting it would require
    /// advancing the nonce again while ciphertext from the earlier,
    /// still-unflushed frame may already be sitting in the kernel's send
    /// buffer; interleaving the two would corrupt the stream the receiver
    /// sees. Once nothing is pending, `buf` is encrypted as a single
    /// frame; the returned byte count reflects all of `buf` the instant
    /// it's captured here (now irreversibly encrypted), not how much of
    /// the resulting ciphertext actually reached the socket -- any
    /// unflushed remainder stays in `write_pending`, visible to the
    /// caller via `has_pending_write()` so flow control downstream can't
    /// treat this connection as idle while bytes are still stuck here.
    fn write_encrypted(&mut self, buf: &[u8]) -> io::Result<usize> {
        if !self.write_pending.is_empty() {
            self.flush_pending()?;
            if !self.write_pending.is_empty() {
                return Err(io::ErrorKind::WouldBlock.into());
            }
        }

        if buf.is_empty() {
            return Ok(0);
        }

        let ciphertext = self
            .machine
            .encrypt_transport(buf)
            .expect("has_transport_crypto() returned true for this machine")
            .map_err(|err| {
                log::error!(target: M::NAME, "Transport encryption failed: {err}");
                io::Error::other(err)
            })?;

        let mut framed = Vec::with_capacity(FRAME_LEN_PREFIX + ciphertext.len());
        framed.extend_from_slice(&(ciphertext.len() as u32).to_be_bytes());
        framed.extend_from_slice(&ciphertext);

        self.write_pending = framed;
        self.write_pending_sent = 0;
        self.flush_pending()?;

        Ok(buf.len())
    }
}

/// Drain up to `buf.len()` bytes from the front of `queue` into `buf`.
/// Shared by the two "serve already-decrypted plaintext" spots in `read()`.
fn drain_into(queue: &mut VecDeque<u8>, buf: &mut [u8]) -> usize {
    let n = queue.len().min(buf.len());
    for slot in buf.iter_mut().take(n) {
        *slot = queue.pop_front().expect("checked len above");
    }
    n
}

impl<M: StateMachine, S: Session> io::Read for Protocol<M, S> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        log::trace!(target: M::NAME, "Reading event");

        if self.machine.is_complete() || !self.session.is_established() {
            if !self.machine.has_transport_crypto() {
                log::trace!(target: M::NAME, "Passing reading to inner not yet established session");
                return self.session.read(buf);
            }
            return self.read_encrypted(buf);
        }

        let len = self.machine.next_read_len();
        let mut input = vec![0u8; len];
        self.session.read_exact(&mut input)?;

        log::trace!(target: M::NAME, "Received handshake act: {input:02x?}");

        if !input.is_empty() {
            let output = self.machine.advance(&input).map_err(|err| {
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
        log::trace!(target: M::NAME, "Writing event (state_complete={}, session_established={})", self.machine.is_complete(), self.session.is_established());

        if self.machine.is_complete() || !self.session.is_established() {
            if !self.machine.has_transport_crypto() {
                log::trace!(target: M::NAME, "Passing writing to inner session");
                return self.session.write(buf);
            }
            return self.write_encrypted(buf);
        }

        if self.machine.next_read_len() == 0 {
            log::trace!(target: M::NAME, "Starting handshake protocol");

            let act = self.machine.advance(&[]).map_err(|err| {
                log::error!(target: M::NAME, "Handshake failure: {err}");
                io::Error::other(err)
            })?;

            if !act.is_empty() {
                log::trace!(target: M::NAME, "Sending handshake act on write: {act:02x?}");
                self.session.write_all(&act)?;
            } else {
                log::trace!(target: M::NAME, "Handshake complete, passing data to inner session");
                if !self.machine.has_transport_crypto() {
                    return self.session.write(buf);
                }
                return self.write_encrypted(buf);
            }
        }

        if buf.is_empty() {
            Ok(0)
        } else {
            Err(io::ErrorKind::Interrupted.into())
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        if !self.write_pending.is_empty() {
            self.flush_pending()?;
        }
        self.session.flush()
    }
}

impl<M: StateMachine, S: Session> Session for Protocol<M, S> {
    type Artifact = ProtocolArtifact<M, S>;

    fn artifact(&self) -> Option<Self::Artifact> {
        Some(ProtocolArtifact {
            session: self.session.artifact()?,
            state: self.machine.artifact()?,
        })
    }

    fn has_pending_write(&self) -> bool {
        !self.write_pending.is_empty() || self.session.has_pending_write()
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
    type Artifact = SocketAddr;

    fn artifact(&self) -> Option<Self::Artifact> {
        self.peer_addr().ok()
    }
}

mod impl_noise {
    use cyphernet::encrypt::noise::{NoiseState as Noise, error::NoiseError as Error};
    use cyphernet::{Digest, Ecdh};

    use super::*;

    #[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
    pub struct NoiseArtifact<E: Ecdh, D: Digest> {
        pub handshake_hash: D::Output,
        pub remote_static_key: Option<E::Pk>,
    }

    /// `NoiseState` plus the one bit of context it loses once the handshake
    /// finishes: which side we were. See the comment above `NoiseSession`
    /// for why that bit is needed to pick the right cipher per direction.
    pub struct NoiseTransport<E: Ecdh, D: Digest> {
        state: Noise<E, D>,
        is_initiator: bool,
    }

    impl<E: Ecdh, D: Digest> NoiseTransport<E, D> {
        pub fn new(state: Noise<E, D>, is_initiator: bool) -> Self {
            Self {
                state,
                is_initiator,
            }
        }
    }

    impl<E: Ecdh, D: Digest> StateMachine for NoiseTransport<E, D> {
        const NAME: &'static str = "noise";
        type Artifact = NoiseArtifact<E, D>;
        type Error = Error;

        fn next_read_len(&self) -> usize {
            self.state.next_read_len()
        }

        fn advance(&mut self, input: &[u8]) -> Result<Vec<u8>, Self::Error> {
            self.state.advance(input)
        }

        fn has_transport_crypto(&self) -> bool {
            matches!(self.state, Noise::Active { .. })
        }

        fn encrypt_transport(&mut self, plaintext: &[u8]) -> Option<Result<Vec<u8>, Self::Error>> {
            match &mut self.state {
                Noise::Active {
                    sending_cipher,
                    receiving_cipher,
                    ..
                } => {
                    // Empty associated data: matches the Noise spec's own
                    // transport-message definition (EncryptWithAd(k, zerolen,
                    // plaintext)) -- not an improvised choice. Sequencing/
                    // replay protection comes from the per-direction nonce
                    // each CipherState already increments internally on
                    // every call, not from anything carried in `ad`.
                    let cipher = if self.is_initiator {
                        sending_cipher
                    } else {
                        receiving_cipher
                    };
                    Some(cipher.encrypt_with_ad(&[], plaintext).map_err(Into::into))
                }
                _ => None,
            }
        }

        fn decrypt_transport(&mut self, ciphertext: &[u8]) -> Option<Result<Vec<u8>, Self::Error>> {
            match &mut self.state {
                Noise::Active {
                    sending_cipher,
                    receiving_cipher,
                    ..
                } => {
                    let cipher = if self.is_initiator {
                        receiving_cipher
                    } else {
                        sending_cipher
                    };
                    Some(cipher.decrypt_with_ad(&[], ciphertext).map_err(Into::into))
                }
                _ => None,
            }
        }

        fn artifact(&self) -> Option<Self::Artifact> {
            self.state.get_handshake_hash().map(|hh| NoiseArtifact {
                handshake_hash: hh,
                remote_static_key: self.state.get_remote_static_key(),
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

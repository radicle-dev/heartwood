use std::fmt;
use std::io::{Read, Write};
use std::ops::DerefMut;
use std::os::unix::net::UnixStream;
use std::path::Path;

use byteorder::{BigEndian, ByteOrder, WriteBytesExt};
use log::*;
use thiserror::Error;
use zeroize::Zeroize as _;

use crate::agent::msg;
use crate::agent::Constraint;
use crate::encoding::{self, Encodable};
use crate::encoding::{Buffer, Encoding, Reader};

/// An ed25519 Signature.
pub type Signature = [u8; 64];

#[derive(Debug, Error)]
pub enum Error {
    /// Agent protocol error.
    #[error("Agent protocol error")]
    AgentProtocolError,
    #[error("Agent failure")]
    AgentFailure,
    #[error("Unable to connect to ssh-agent. The environment variable `SSH_AUTH_SOCK` was set, but it points to a nonexistent file or directory.")]
    BadAuthSock,
    #[error(transparent)]
    Encoding(#[from] encoding::Error),
    #[error("Environment variable `{0}` not found")]
    EnvVar(&'static str),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Private(Box<dyn std::error::Error + Send + Sync + 'static>),
    #[error(transparent)]
    Public(Box<dyn std::error::Error + Send + Sync + 'static>),
    #[error(transparent)]
    Signature(Box<dyn std::error::Error + Send + Sync + 'static>),
}

/// SSH agent client.
pub struct AgentClient<S> {
    stream: S,
}

// https://tools.ietf.org/html/draft-miller-ssh-agent-00#section-4.1
impl<S> AgentClient<S> {
    /// Connect to an SSH agent via the provided stream (on Unix, usually a Unix-domain socket).
    pub fn connect(stream: S) -> Self {
        AgentClient { stream }
    }
}

pub trait ClientStream: Sized + Send + Sync {
    /// Send an agent request through the stream and read the response.
    fn request(&mut self, req: &[u8]) -> Result<Buffer, Error>;

    /// How to connect the streaming socket
    fn connect<P>(path: P) -> Result<AgentClient<Self>, Error>
    where
        P: AsRef<Path> + Send;

    fn connect_env() -> Result<AgentClient<Self>, Error> {
        let Ok(var) = std::env::var("SSH_AUTH_SOCK") else {
            return Err(Error::EnvVar("SSH_AUTH_SOCK"));
        };
        match Self::connect(var) {
            Err(Error::Io(io_err)) if io_err.kind() == std::io::ErrorKind::NotFound => {
                Err(Error::BadAuthSock)
            }
            err => err,
        }
    }
}

impl<S: ClientStream> AgentClient<S> {
    /// Send a key to the agent, with a (possibly empty) slice of constraints
    /// to apply when using the key to sign.
    pub fn add_identity<K>(&mut self, key: &K, constraints: &[Constraint]) -> Result<(), Error>
    where
        K: Encodable,
        K::Error: std::error::Error + Send + Sync + 'static,
    {
        let mut buf = Buffer::default();

        buf.resize(4, 0);

        if constraints.is_empty() {
            buf.push(msg::ADD_IDENTITY)
        } else {
            buf.push(msg::ADD_ID_CONSTRAINED)
        }
        key.write(&mut buf);

        if !constraints.is_empty() {
            for cons in constraints {
                match *cons {
                    Constraint::KeyLifetime { seconds } => {
                        buf.push(msg::CONSTRAIN_LIFETIME);
                        buf.deref_mut().write_u32::<BigEndian>(seconds)?
                    }
                    Constraint::Confirm => buf.push(msg::CONSTRAIN_CONFIRM),
                    Constraint::Extensions {
                        ref name,
                        ref details,
                    } => {
                        buf.push(msg::CONSTRAIN_EXTENSION);
                        buf.extend_ssh_string(name);
                        buf.extend_ssh_string(details);
                    }
                }
            }
        }
        buf.write_len();
        self.stream.request(&buf)?;

        Ok(())
    }

    /// Add a smart card to the agent, with a (possibly empty) set of
    /// constraints to apply when signing.
    pub fn add_smartcard_key(
        &mut self,
        id: &str,
        pin: &[u8],
        constraints: &[Constraint],
    ) -> Result<(), Error> {
        let mut buf = Buffer::default();

        buf.resize(4, 0);

        if constraints.is_empty() {
            buf.push(msg::ADD_SMARTCARD_KEY)
        } else {
            buf.push(msg::ADD_SMARTCARD_KEY_CONSTRAINED)
        }
        buf.extend_ssh_string(id.as_bytes());
        buf.extend_ssh_string(pin);

        if !constraints.is_empty() {
            buf.deref_mut()
                .write_u32::<BigEndian>(constraints.len() as u32)?;
            for cons in constraints {
                match *cons {
                    Constraint::KeyLifetime { seconds } => {
                        buf.push(msg::CONSTRAIN_LIFETIME);
                        buf.deref_mut().write_u32::<BigEndian>(seconds)?;
                    }
                    Constraint::Confirm => buf.push(msg::CONSTRAIN_CONFIRM),
                    Constraint::Extensions {
                        ref name,
                        ref details,
                    } => {
                        buf.push(msg::CONSTRAIN_EXTENSION);
                        buf.extend_ssh_string(name);
                        buf.extend_ssh_string(details);
                    }
                }
            }
        }
        buf.write_len();
        self.stream.request(&buf)?;

        Ok(())
    }

    /// Lock the agent, making it refuse to sign until unlocked.
    pub fn lock(&mut self, passphrase: &[u8]) -> Result<(), Error> {
        let mut buf = Buffer::default();

        buf.resize(4, 0);
        buf.push(msg::LOCK);
        buf.extend_ssh_string(passphrase);
        buf.write_len();

        self.stream.request(&buf)?;

        Ok(())
    }

    /// Unlock the agent, allowing it to sign again.
    pub fn unlock(&mut self, passphrase: &[u8]) -> Result<(), Error> {
        let mut buf = Buffer::default();
        buf.resize(4, 0);
        buf.push(msg::UNLOCK);
        buf.extend_ssh_string(passphrase);
        buf.write_len();

        self.stream.request(&buf)?;

        Ok(())
    }

    /// Ask the agent for a list of the currently registered secret
    /// keys.
    pub fn request_identities<K>(&mut self) -> Result<Vec<K>, Error>
    where
        K: Encodable,
        K::Error: std::error::Error + Send + Sync + 'static,
    {
        let mut buf = Buffer::default();
        buf.resize(4, 0);
        buf.push(msg::REQUEST_IDENTITIES);
        buf.write_len();

        let mut keys = Vec::new();
        let resp = self.stream.request(&buf)?;

        if resp[0] == msg::IDENTITIES_ANSWER {
            let mut r = resp.reader(1);
            let n = r.read_u32()?;

            for _ in 0..n {
                let key = r.read_string()?;
                let _ = r.read_string()?;
                let mut r = key.reader(0);

                if let Ok(pk) = K::read(&mut r) {
                    keys.push(pk);
                }
            }
        }

        Ok(keys)
    }

    /// Ask the agent to sign the supplied piece of data.
    pub fn sign<K>(&mut self, public: &K, data: &[u8]) -> Result<Signature, Error>
    where
        K: Encodable + fmt::Debug,
    {
        let req = self.prepare_sign_request(public, data);
        let resp = self.stream.request(&req)?;

        if !resp.is_empty() && resp[0] == msg::SIGN_RESPONSE {
            self.read_signature(&resp)
        } else if !resp.is_empty() && resp[0] == msg::FAILURE {
            Err(Error::AgentFailure)
        } else {
            Err(Error::AgentProtocolError)
        }
    }

    fn prepare_sign_request<K>(&self, public: &K, data: &[u8]) -> Buffer
    where
        K: Encodable + fmt::Debug,
    {
        // byte                    SSH_AGENTC_SIGN_REQUEST
        // string                  key blob
        // string                  data
        // uint32                  flags

        let mut pk = Buffer::default();
        public.write(&mut pk);

        let total = 1 + pk.len() + 4 + data.len() + 4;

        let mut buf = Buffer::default();
        buf.write_u32::<BigEndian>(total as u32)
            .expect("Writing to a vector never fails");
        buf.push(msg::SIGN_REQUEST);
        buf.extend_from_slice(&pk);
        buf.extend_ssh_string(data);

        // Signature flags should be zero for ed25519.
        buf.write_u32::<BigEndian>(0).unwrap();
        buf
    }

    fn read_signature(&self, sig: &[u8]) -> Result<Signature, Error> {
        let mut r = sig.reader(1);
        let mut resp = r.read_string()?.reader(0);
        let _t = resp.read_string()?;
        let sig = resp.read_string()?;

        let mut out = [0; 64];
        out.copy_from_slice(sig);

        Ok(out)
    }

    /// Ask the agent to remove a key from its memory.
    pub fn remove_identity<K>(&mut self, public: &K) -> Result<(), Error>
    where
        K: Encodable,
    {
        let mut pk: Buffer = Vec::new().into();
        public.write(&mut pk);

        let total = 1 + pk.len();

        let mut buf = Buffer::default();
        buf.write_u32::<BigEndian>(total as u32)?;
        buf.push(msg::REMOVE_IDENTITY);
        buf.extend_from_slice(&pk);

        self.stream.request(&buf)?;

        Ok(())
    }

    /// Ask the agent to remove a smartcard from its memory.
    pub fn remove_smartcard_key(&mut self, id: &str, pin: &[u8]) -> Result<(), Error> {
        let mut buf = Buffer::default();
        buf.resize(4, 0);
        buf.push(msg::REMOVE_SMARTCARD_KEY);
        buf.extend_ssh_string(id.as_bytes());
        buf.extend_ssh_string(pin);
        buf.write_len();

        self.stream.request(&buf)?;

        Ok(())
    }

    /// Ask the agent to forget all known keys.
    pub fn remove_all_identities(&mut self) -> Result<(), Error> {
        let mut buf = Buffer::default();
        buf.resize(4, 0);
        buf.push(msg::REMOVE_ALL_IDENTITIES);
        buf.write_len();

        self.stream.request(&buf)?;

        Ok(())
    }

    /// Send a custom message to the agent.
    pub fn extension(&mut self, typ: &[u8], ext: &[u8]) -> Result<(), Error> {
        let mut buf = Buffer::default();

        buf.resize(4, 0);
        buf.push(msg::EXTENSION);
        buf.extend_ssh_string(typ);
        buf.extend_ssh_string(ext);
        buf.write_len();

        self.stream.request(&buf)?;

        Ok(())
    }

    /// Ask the agent about supported extensions.
    pub fn query_extension(&mut self, typ: &[u8], mut ext: Buffer) -> Result<bool, Error> {
        let mut req = Buffer::default();

        req.resize(4, 0);
        req.push(msg::EXTENSION);
        req.extend_ssh_string(typ);
        req.write_len();

        let resp = self.stream.request(&req)?;
        let mut r = resp.reader(1);
        ext.extend(r.read_string()?);

        Ok(!resp.is_empty() && resp[0] == msg::SUCCESS)
    }
}

#[cfg(not(unix))]
impl ClientStream for TcpStream {
    fn connect_uds<P>(_: P) -> Result<AgentClient<Self>, Error>
    where
        P: AsRef<Path> + Send,
    {
        Err(Error::AgentFailure)
    }

    fn read_response(&mut self, _: &mut Buffer) -> Result<(), Error> {
        Err(Error::AgentFailure)
    }

    fn connect_env() -> Result<AgentClient<Self>, Error> {
        Err(Error::AgentFailure)
    }
}

#[cfg(unix)]
impl ClientStream for UnixStream {
    fn connect<P>(path: P) -> Result<AgentClient<Self>, Error>
    where
        P: AsRef<Path> + Send,
    {
        let stream = UnixStream::connect(path)?;

        Ok(AgentClient { stream })
    }

    fn request(&mut self, msg: &[u8]) -> Result<Buffer, Error> {
        let mut resp = Buffer::default();

        // Write the message
        self.write_all(msg)?;
        self.flush()?;

        // Read the length
        resp.resize(4, 0);
        self.read_exact(&mut resp)?;

        // Read the rest of the buffer
        let len = BigEndian::read_u32(&resp) as usize;
        resp.zeroize();
        resp.resize(len, 0);
        self.read_exact(&mut resp)?;

        Ok(resp)
    }
}

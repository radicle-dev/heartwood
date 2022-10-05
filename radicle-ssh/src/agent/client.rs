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
use crate::encoding;
use crate::encoding::{Buffer, Encoding, Reader};
use crate::key::{Private, Public};

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
    buf: Buffer,
}

// https://tools.ietf.org/html/draft-miller-ssh-agent-00#section-4.1
impl<S> AgentClient<S> {
    /// Connect to an SSH agent via the provided stream (on Unix, usually a Unix-domain socket).
    pub fn connect(stream: S) -> Self {
        AgentClient {
            stream,
            buf: Vec::new().into(),
        }
    }
}

pub trait ClientStream: Sized + Send + Sync {
    /// How to read the response from the stream
    fn read_response(&mut self, buf: &mut Buffer) -> Result<(), Error>;

    /// How to connect the streaming socket
    fn connect_socket<P>(path: P) -> Result<AgentClient<Self>, Error>
    where
        P: AsRef<Path> + Send;

    fn connect_env() -> Result<AgentClient<Self>, Error> {
        let var = if let Ok(var) = std::env::var("SSH_AUTH_SOCK") {
            var
        } else {
            return Err(Error::EnvVar("SSH_AUTH_SOCK"));
        };
        match Self::connect_socket(var) {
            Err(Error::Io(io_err)) if io_err.kind() == std::io::ErrorKind::NotFound => {
                Err(Error::BadAuthSock)
            }
            owise => owise,
        }
    }
}

impl<S: ClientStream> AgentClient<S> {
    /// Send a key to the agent, with a (possibly empty) slice of constraints
    /// to apply when using the key to sign.
    pub fn add_identity<K>(&mut self, key: &K, constraints: &[Constraint]) -> Result<(), Error>
    where
        K: Private,
        K::Error: std::error::Error + Send + Sync + 'static,
    {
        self.buf.zeroize();
        self.buf.resize(4, 0);

        if constraints.is_empty() {
            self.buf.push(msg::ADD_IDENTITY)
        } else {
            self.buf.push(msg::ADD_ID_CONSTRAINED)
        }
        key.write(&mut self.buf)
            .map_err(|err| Error::Private(Box::new(err)))?;

        if !constraints.is_empty() {
            for cons in constraints {
                match *cons {
                    Constraint::KeyLifetime { seconds } => {
                        self.buf.push(msg::CONSTRAIN_LIFETIME);
                        self.buf.deref_mut().write_u32::<BigEndian>(seconds)?
                    }
                    Constraint::Confirm => self.buf.push(msg::CONSTRAIN_CONFIRM),
                    Constraint::Extensions {
                        ref name,
                        ref details,
                    } => {
                        self.buf.push(msg::CONSTRAIN_EXTENSION);
                        self.buf.extend_ssh_string(name);
                        self.buf.extend_ssh_string(details);
                    }
                }
            }
        }
        self.buf.write_len();
        self.stream.read_response(&mut self.buf)?;

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
        self.buf.zeroize();
        self.buf.resize(4, 0);

        if constraints.is_empty() {
            self.buf.push(msg::ADD_SMARTCARD_KEY)
        } else {
            self.buf.push(msg::ADD_SMARTCARD_KEY_CONSTRAINED)
        }
        self.buf.extend_ssh_string(id.as_bytes());
        self.buf.extend_ssh_string(pin);

        if !constraints.is_empty() {
            self.buf
                .deref_mut()
                .write_u32::<BigEndian>(constraints.len() as u32)?;
            for cons in constraints {
                match *cons {
                    Constraint::KeyLifetime { seconds } => {
                        self.buf.push(msg::CONSTRAIN_LIFETIME);
                        self.buf.deref_mut().write_u32::<BigEndian>(seconds)?;
                    }
                    Constraint::Confirm => self.buf.push(msg::CONSTRAIN_CONFIRM),
                    Constraint::Extensions {
                        ref name,
                        ref details,
                    } => {
                        self.buf.push(msg::CONSTRAIN_EXTENSION);
                        self.buf.extend_ssh_string(name);
                        self.buf.extend_ssh_string(details);
                    }
                }
            }
        }
        self.buf.write_len();
        self.stream.read_response(&mut self.buf)?;

        Ok(())
    }

    /// Lock the agent, making it refuse to sign until unlocked.
    pub fn lock(&mut self, passphrase: &[u8]) -> Result<(), Error> {
        self.buf.zeroize();
        self.buf.resize(4, 0);
        self.buf.push(msg::LOCK);
        self.buf.extend_ssh_string(passphrase);
        self.buf.write_len();

        self.stream.read_response(&mut self.buf)?;

        Ok(())
    }

    /// Unlock the agent, allowing it to sign again.
    pub fn unlock(&mut self, passphrase: &[u8]) -> Result<(), Error> {
        self.buf.zeroize();
        self.buf.resize(4, 0);
        self.buf.push(msg::UNLOCK);
        self.buf.extend_ssh_string(passphrase);
        self.buf.write_len();

        self.stream.read_response(&mut self.buf)?;

        Ok(())
    }

    /// Ask the agent for a list of the currently registered secret
    /// keys.
    pub fn request_identities<K>(&mut self) -> Result<Vec<K>, Error>
    where
        K: Public,
        K::Error: std::error::Error + Send + Sync + 'static,
    {
        self.buf.zeroize();
        self.buf.resize(4, 0);
        self.buf.push(msg::REQUEST_IDENTITIES);
        self.buf.write_len();

        self.stream.read_response(&mut self.buf)?;
        debug!("identities: {:?}", &self.buf[..]);

        let mut keys = Vec::new();
        if self.buf[0] == msg::IDENTITIES_ANSWER {
            let mut r = self.buf.reader(1);
            let n = r.read_u32()?;

            for _ in 0..n {
                let key = r.read_string()?;
                let _ = r.read_string()?;
                let mut r = key.reader(0);

                if let Some(pk) = K::read(&mut r).map_err(|err| Error::Public(Box::new(err)))? {
                    keys.push(pk);
                }
            }
        }

        Ok(keys)
    }

    /// Ask the agent to sign the supplied piece of data.
    pub fn sign_request<K>(&mut self, public: &K, data: Buffer) -> Result<Signature, Error>
    where
        K: Public + fmt::Debug,
    {
        self.prepare_sign_request(public, &data);
        self.stream.read_response(&mut self.buf)?;

        if !self.buf.is_empty() && self.buf[0] == msg::SIGN_RESPONSE {
            let mut signature: Signature = [0; 64];
            self.write_signature(&mut signature)?;

            Ok(signature)
        } else if self.buf[0] == msg::FAILURE {
            Err(Error::AgentFailure)
        } else {
            Err(Error::AgentProtocolError)
        }
    }

    fn prepare_sign_request<K>(&mut self, public: &K, data: &[u8])
    where
        K: Public + fmt::Debug,
    {
        // byte                    SSH_AGENTC_SIGN_REQUEST
        // string                  key blob
        // string                  data
        // uint32                  flags

        let mut pk = Buffer::default();
        let n = public.write(&mut pk);
        let total = 1 + n + 4 + data.len() + 4;

        debug_assert_eq!(n, pk.len());

        self.buf.zeroize();
        self.buf
            .write_u32::<BigEndian>(total as u32)
            .expect("Writing to a vector never fails");
        self.buf.push(msg::SIGN_REQUEST);
        self.buf.extend_from_slice(&pk);
        self.buf.extend_ssh_string(data);

        // Signature flags should be zero for ed25519.
        self.buf.write_u32::<BigEndian>(0).unwrap();
    }

    fn write_signature(&self, data: &mut [u8]) -> Result<(), Error> {
        let mut r = self.buf.reader(1);
        let mut resp = r.read_string()?.reader(0);
        let _t = resp.read_string()?;
        let sig = resp.read_string()?;

        data.copy_from_slice(sig);

        Ok(())
    }

    /// Ask the agent to remove a key from its memory.
    pub fn remove_identity<K>(&mut self, public: &K) -> Result<(), Error>
    where
        K: Public,
    {
        let mut pk: Buffer = Vec::new().into();
        let n = public.write(&mut pk);
        let total = 1 + n;

        debug_assert_eq!(n, pk.len());

        self.buf.zeroize();
        self.buf.write_u32::<BigEndian>(total as u32)?;
        self.buf.push(msg::REMOVE_IDENTITY);
        self.buf.extend_from_slice(&pk);

        self.stream.read_response(&mut self.buf)?;

        Ok(())
    }

    /// Ask the agent to remove a smartcard from its memory.
    pub fn remove_smartcard_key(&mut self, id: &str, pin: &[u8]) -> Result<(), Error> {
        self.buf.zeroize();
        self.buf.resize(4, 0);
        self.buf.push(msg::REMOVE_SMARTCARD_KEY);
        self.buf.extend_ssh_string(id.as_bytes());
        self.buf.extend_ssh_string(pin);
        self.buf.write_len();

        self.stream.read_response(&mut self.buf)?;

        Ok(())
    }

    /// Ask the agent to forget all known keys.
    pub fn remove_all_identities(&mut self) -> Result<(), Error> {
        self.buf.zeroize();
        self.buf.resize(4, 0);
        self.buf.push(msg::REMOVE_ALL_IDENTITIES);
        self.buf.write_len();

        self.stream.read_response(&mut self.buf)?;

        Ok(())
    }

    /// Send a custom message to the agent.
    pub fn extension(&mut self, typ: &[u8], ext: &[u8]) -> Result<(), Error> {
        self.buf.zeroize();
        self.buf.resize(4, 0);
        self.buf.push(msg::EXTENSION);
        self.buf.extend_ssh_string(typ);
        self.buf.extend_ssh_string(ext);
        self.buf.write_len();

        self.stream.read_response(&mut self.buf)?;

        Ok(())
    }

    /// Ask the agent what extensions about supported extensions.
    pub fn query_extension(&mut self, typ: &[u8], mut ext: Buffer) -> Result<bool, Error> {
        self.buf.zeroize();
        self.buf.resize(4, 0);
        self.buf.push(msg::EXTENSION);
        self.buf.extend_ssh_string(typ);
        self.buf.write_len();

        self.stream.read_response(&mut self.buf)?;

        let mut r = self.buf.reader(1);
        ext.extend(r.read_string()?);

        Ok(!self.buf.is_empty() && self.buf[0] == msg::SUCCESS)
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
    fn connect_socket<P>(path: P) -> Result<AgentClient<Self>, Error>
    where
        P: AsRef<Path> + Send,
    {
        let stream = UnixStream::connect(path)?;
        Ok(AgentClient {
            stream,
            buf: Vec::new().into(),
        })
    }

    fn read_response(&mut self, buf: &mut Buffer) -> Result<(), Error> {
        // Write the message
        self.write_all(buf)?;
        self.flush()?;

        // Read the length
        buf.zeroize();
        buf.resize(4, 0);
        self.read_exact(buf)?;

        // Read the rest of the buffer
        let len = BigEndian::read_u32(buf) as usize;
        buf.zeroize();
        buf.resize(len, 0);
        self.read_exact(buf)?;

        Ok(())
    }
}

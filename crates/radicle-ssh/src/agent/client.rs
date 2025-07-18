use std::fmt;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

#[cfg(unix)]
pub use std::os::unix::net::UnixStream as Stream;

#[cfg(windows)]
pub use winpipe::WinStream as Stream;

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
    #[error("SSH agent replied with unexpected data, violating the SSH agent protocol.")]
    AgentProtocolError,
    #[error(
        "SSH agent replied with failure (protocol message number 5), which could not be handled."
    )]
    AgentFailure,
    #[error("Unable to connect to SSH agent because '{path}' was not found: {source}")]
    BadAuthSock {
        path: String,
        source: std::io::Error,
    },
    #[error("Encoding error while communicating with SSH agent: {0}")]
    Encoding(#[from] encoding::Error),
    #[error("Unable to read environment variable '{var}': {source}")]
    EnvVar {
        var: String,
        source: std::env::VarError,
    },
    #[error("Unable to connect SSH agent using the path '{path}': {source}")]
    Connect {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("I/O error while communicating with SSH agent: {0}")]
    Io(#[from] std::io::Error),
}

impl Error {
    pub fn is_not_running(&self) -> bool {
        matches!(self, Self::EnvVar { .. } | Self::BadAuthSock { .. })
    }
}

/// SSH agent client.
pub struct AgentClient<S = Stream> {
    /// The path that was originally used to connect to the agent.
    path: Option<PathBuf>,

    /// The underlying stream to the SSH agent.
    stream: S,
}

impl<S> AgentClient<S> {
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }
}

impl AgentClient<Stream> {
    /// Connect to an SSH agent at the provided path.
    pub fn connect<P>(path: P) -> Result<Self, Error>
    where
        P: AsRef<Path>,
    {
        let path = path.as_ref().to_owned();

        let stream = match Stream::connect(&path) {
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Err(Error::BadAuthSock {
                    path: path.display().to_string(),
                    source: err,
                })
            }
            Err(err) => {
                return Err(Error::Connect {
                    path: path.display().to_string(),
                    source: err,
                })
            }
            Ok(stream) => stream,
        };

        Ok(Self {
            path: Some(path),
            stream,
        })
    }

    pub fn connect_env() -> Result<Self, Error> {
        const SSH_AUTH_SOCK: &str = "SSH_AUTH_SOCK";

        let path = match std::env::var(SSH_AUTH_SOCK) {
            Ok(var) => var,
            Err(err) => {
                if cfg!(windows) {
                    // Windows uses a named pipe for the SSH agent, which
                    // we fall back to in case reading the environment
                    // variable fails.
                    "\\\\.\\pipe\\openssh-ssh-agent".to_string()
                } else {
                    return Err(Error::EnvVar {
                        var: SSH_AUTH_SOCK.to_string(),
                        source: err,
                    });
                }
            }
        };

        Self::connect(path)
    }
}

impl<Stream: ClientStream> AgentClient<Stream> {
    pub fn new(path: Option<PathBuf>, stream: Stream) -> Self {
        Self { path, stream }
    }

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
                        buf.extend_u32(seconds);
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
            buf.extend_usize(constraints.len());
            for cons in constraints {
                match *cons {
                    Constraint::KeyLifetime { seconds } => {
                        buf.push(msg::CONSTRAIN_LIFETIME);
                        buf.extend_u32(seconds);
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
        buf.extend_usize(total);
        buf.push(msg::SIGN_REQUEST);
        buf.extend_from_slice(&pk);
        buf.extend_ssh_string(data);

        // Signature flags should be zero for ed25519.
        buf.extend_u32(0);
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
        buf.extend_usize(total);
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

pub trait ClientStream: Sized + Send + Sync {
    fn request(&mut self, msg: &[u8]) -> Result<Buffer, Error>;
}

impl<S: Read + Write + Sized + Send + Sync> ClientStream for S {
    fn request(&mut self, msg: &[u8]) -> Result<Buffer, Error> {
        let mut resp = Buffer::default();

        // Write the message
        self.write_all(msg)?;
        self.flush()?;

        // Read the length
        resp.resize(4, 0);
        self.read_exact(&mut resp)?;

        // Read the rest of the buffer
        let len = u32::from_be_bytes(resp.as_slice().try_into().unwrap()) as usize;

        resp.zeroize();
        resp.resize(len, 0);
        self.read_exact(&mut resp)?;

        Ok(resp)
    }
}

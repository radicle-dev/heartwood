use std::ops::{Deref, DerefMut};
use std::sync::Mutex;

pub use radicle_ssh::agent::client::AgentClient;
pub use radicle_ssh::agent::client::Error;
pub use radicle_ssh::{self as ssh, agent::client::ClientStream};

use crate::{PublicKey, SecretKey, Signature, Signer, SignerError};

#[cfg(not(unix))]
pub use std::net::TcpStream as Stream;
#[cfg(unix)]
pub use std::os::unix::net::UnixStream as Stream;

pub struct Agent {
    client: AgentClient<Stream>,
}

impl Agent {
    /// Connect to a running SSH agent.
    pub fn connect() -> Result<Self, ssh::agent::client::Error> {
        Stream::connect_env().map(|client| Self { client })
    }

    /// Register a key with the agent.
    pub fn register(&mut self, key: &SecretKey) -> Result<(), ssh::Error> {
        self.client.add_identity(key, &[])
    }

    /// Get a signer from this agent, given the public key.
    pub fn signer(self, key: PublicKey) -> AgentSigner {
        AgentSigner::new(self, key)
    }
}

impl Deref for Agent {
    type Target = AgentClient<Stream>;

    fn deref(&self) -> &Self::Target {
        &self.client
    }
}

impl DerefMut for Agent {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.client
    }
}

/// A [`Signer`] that uses `ssh-agent`.
pub struct AgentSigner {
    agent: Mutex<Agent>,
    public: PublicKey,
}

impl AgentSigner {
    pub fn new(agent: Agent, public: PublicKey) -> Self {
        let agent = Mutex::new(agent);

        Self { agent, public }
    }

    pub fn is_ready(&self) -> Result<bool, Error> {
        let ids = self.agent.lock().unwrap().request_identities()?;

        Ok(ids.contains(&self.public))
    }

    /// Box this signer into a [`Signer`].
    pub fn boxed(self) -> Box<dyn Signer> {
        Box::new(self)
    }
}

impl Signer for AgentSigner {
    fn public_key(&self) -> &PublicKey {
        &self.public
    }

    fn sign(&self, msg: &[u8]) -> Signature {
        self.try_sign(msg).unwrap()
    }

    fn try_sign(&self, msg: &[u8]) -> Result<Signature, SignerError> {
        let sig = self
            .agent
            .lock()
            // We'll take our chances here; the worse that can happen is the agent returns an error.
            .unwrap_or_else(|e| e.into_inner())
            .sign(&self.public, msg)
            .map_err(SignerError::new)?;

        Ok(Signature::from(sig))
    }
}

impl signature::Signer<Signature> for AgentSigner {
    fn try_sign(&self, msg: &[u8]) -> Result<Signature, signature::Error> {
        let sig = self
            .agent
            .lock()
            // We'll take our chances here; the worse that can happen is the agent returns an error.
            .unwrap_or_else(|e| e.into_inner())
            .sign(&self.public, msg)
            .map_err(signature::Error::from_source)?;

        Ok(Signature::from(sig))
    }
}

use std::cell::RefCell;

pub use radicle_ssh::agent::client::AgentClient;
pub use radicle_ssh::agent::client::Error;
pub use radicle_ssh::{self as ssh, agent::client::ClientStream};

use crate::{PublicKey, SecretKey, Signature, Signer, SignerError};

#[cfg(not(unix))]
pub use std::net::TcpStream as Stream;
#[cfg(unix)]
pub use std::os::unix::net::UnixStream as Stream;

use super::ExtendedSignature;

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

    pub fn unregister(&mut self, key: &PublicKey) -> Result<(), ssh::Error> {
        self.client.remove_identity(key)
    }

    pub fn unregister_all(&mut self) -> Result<(), ssh::Error> {
        self.client.remove_all_identities()
    }

    pub fn sign(&mut self, key: &PublicKey, data: &[u8]) -> Result<[u8; 64], ssh::Error> {
        self.client.sign(key, data)
    }

    /// Get a signer from this agent, given the public key.
    pub fn signer(self, key: PublicKey) -> AgentSigner {
        AgentSigner::new(self, key)
    }

    pub fn pid(&self) -> Option<u32> {
        self.client.pid()
    }

    pub fn request_identities(&mut self) -> Result<Vec<PublicKey>, ssh::agent::client::Error> {
        self.client.request_identities()
    }
}

/// A [`Signer`] that uses `ssh-agent`.
pub struct AgentSigner {
    agent: RefCell<Agent>,
    public: PublicKey,
}

impl signature::Signer<Signature> for AgentSigner {
    fn try_sign(&self, msg: &[u8]) -> Result<Signature, signature::Error> {
        Signer::try_sign(self, msg).map_err(signature::Error::from_source)
    }
}

impl signature::Signer<ExtendedSignature> for AgentSigner {
    fn try_sign(&self, msg: &[u8]) -> Result<ExtendedSignature, signature::Error> {
        Ok(ExtendedSignature {
            key: self.public,
            sig: Signer::try_sign(self, msg).map_err(signature::Error::from_source)?,
        })
    }
}

impl AgentSigner {
    pub fn new(agent: Agent, public: PublicKey) -> Self {
        let agent = RefCell::new(agent);

        Self { agent, public }
    }

    pub fn is_ready(&self) -> Result<bool, Error> {
        let ids = self.agent.borrow_mut().request_identities()?;

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
            .borrow_mut()
            .sign(&self.public, msg)
            .map_err(SignerError::new)?;

        Ok(Signature::from(sig))
    }
}

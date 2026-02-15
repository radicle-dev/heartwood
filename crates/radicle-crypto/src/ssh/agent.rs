use std::cell::RefCell;
use std::env::VarError;
use std::path::Path;
use std::path::PathBuf;

use proto::Credential;
use ssh_agent_lib::blocking::Client;
pub use ssh_agent_lib::error::AgentError;
use ssh_agent_lib::proto;
use ssh_key::public::{Ed25519PublicKey, KeyData};
use thiserror::Error;

use crate::{PublicKey, SecretKey, Signature, Signer};

use super::ExtendedSignature;

#[cfg(unix)]
use std::os::unix::net::UnixStream as Stream;

#[cfg(windows)]
use winpipe::WinStream as Stream;

#[derive(Debug, Error)]
pub enum ConnectError {
    #[error(transparent)]
    Agent(#[from] AgentError),
    #[error("Unable to read environment variable '{var}': {source}")]
    EnvVar { var: String, source: VarError },
}

impl ConnectError {
    pub fn is_not_running(&self) -> bool {
        use std::io::ErrorKind::*;
        match self {
            Self::EnvVar {
                source: VarError::NotPresent,
                ..
            } => true,
            Self::Agent(AgentError::IO(source)) if source.kind() == NotFound => true,
            #[cfg(windows)]
            Self::Agent(AgentError::IO(source)) if source.kind() == ConnectionRefused => {
                // On Windows, a named pipe might be used, and if no
                // agent is running, we might get a "connection refused"
                // error, even though the `SSH_AUTH_SOCK` environment variable
                // is set and a named pipe exists.
                true
            }
            _ => false,
        }
    }
}

pub struct Agent {
    path: PathBuf,
    client: Client<Stream>,
}

impl Agent {
    /// Connect to a running SSH agent.
    pub fn connect() -> Result<Self, ConnectError> {
        const SSH_AUTH_SOCK: &str = "SSH_AUTH_SOCK";

        let path =
            PathBuf::from(
                std::env::var(SSH_AUTH_SOCK).map_err(|err| ConnectError::EnvVar {
                    var: SSH_AUTH_SOCK.to_string(),
                    source: err,
                })?,
            );

        let client = Client::new(
            Stream::connect(&path).map_err(|err| ConnectError::Agent(AgentError::IO(err)))?,
        );

        Ok(Self { path, client })
    }

    /// Register a key with the agent.
    pub fn register(&mut self, key: &SecretKey) -> Result<(), AgentError> {
        use ssh_key::private::{Ed25519Keypair, KeypairData};
        self.client.add_identity(proto::AddIdentity {
            credential: Credential::Key {
                privkey: KeypairData::Ed25519(Ed25519Keypair::from_bytes(key).unwrap()),
                comment: "".into(),
            },
        })
    }

    pub fn unregister(&mut self, key: &PublicKey) -> Result<(), AgentError> {
        self.client.remove_identity(proto::RemoveIdentity {
            pubkey: Self::key_data(key),
        })
    }

    pub fn unregister_all(&mut self) -> Result<(), AgentError> {
        self.client.remove_all_identities()
    }

    pub fn sign(&mut self, key: &PublicKey, data: &[u8]) -> Result<[u8; 64], AgentError> {
        let sig = self.client.sign(proto::SignRequest {
            pubkey: Self::key_data(key),
            data: data.to_vec(),
            flags: 0,
        })?;

        Ok(sig.as_bytes().to_owned().try_into().unwrap())
    }

    /// Get a signer from this agent, given the public key.
    pub fn signer(self, key: PublicKey) -> AgentSigner {
        AgentSigner::new(self, key)
    }

    pub fn path(&self) -> &Path {
        self.path.as_ref()
    }

    pub fn request_identities(&mut self) -> Result<Vec<PublicKey>, AgentError> {
        Ok(self
            .client
            .request_identities()?
            .into_iter()
            .filter_map(|identity| identity.pubkey.ed25519().map(|key| PublicKey::from(key.0)))
            .collect())
    }

    fn key_data(key: &PublicKey) -> KeyData {
        KeyData::Ed25519(Ed25519PublicKey(key.to_byte_array()))
    }
}

/// A [`Signer`] that uses `ssh-agent`.
pub struct AgentSigner {
    agent: RefCell<Agent>,
    public: PublicKey,
}

impl signature::Signer<Signature> for AgentSigner {
    fn try_sign(&self, msg: &[u8]) -> Result<Signature, signature::Error> {
        let sig = self
            .agent
            .borrow_mut()
            .sign(&self.public, msg)
            .map_err(signature::Error::from_source)?;
        Ok(Signature::from(sig))
    }
}

impl signature::Signer<ExtendedSignature> for AgentSigner {
    fn try_sign(&self, msg: &[u8]) -> Result<ExtendedSignature, signature::Error> {
        use signature::Keypair as _;
        Ok(ExtendedSignature {
            key: self.verifying_key(),
            sig: self.try_sign(msg)?,
        })
    }
}

impl AsRef<PublicKey> for AgentSigner {
    fn as_ref(&self) -> &PublicKey {
        &self.public
    }
}

impl signature::KeypairRef for AgentSigner {
    type VerifyingKey = PublicKey;
}

impl AgentSigner {
    pub fn new(agent: Agent, public: PublicKey) -> Self {
        let agent = RefCell::new(agent);

        Self { agent, public }
    }

    pub fn is_ready(&self) -> Result<bool, AgentError> {
        let ids = self.agent.borrow_mut().request_identities()?;

        Ok(ids.contains(&self.public))
    }

    /// Box this signer into a [`Signer`].
    pub fn boxed(self) -> Box<dyn Signer> {
        Box::new(self)
    }
}

#[cfg(test)]
mod test {
    use crate::PublicKey;
    use ssh_agent_lib::blocking::Client;
    use ssh_agent_lib::proto::SignRequest;
    use ssh_agent_lib::ssh_key::public::{Ed25519PublicKey, KeyData};

    #[test]
    fn test_agent_encoding_remove() {
        use std::str::FromStr;

        let pk = PublicKey::from_str("z6MktWkM9vcfysWFq1c2aaLjJ6j4PYYg93TLPswR4qtuoAeT").unwrap();
        let expected = [
            0, 0, 0, 56, // Message length
            18, // Message type (remove identity)
            0, 0, 0, 51, // Key blob length
            0, 0, 0, 11, // Key type length
            115, 115, 104, 45, 101, 100, 50, 53, 53, 49, 57, // Key type
            0, 0, 0, 32, // Key length
            208, 232, 92, 138, 225, 114, 116, 99, 156, 177, 148, 93, 65, 93, 198, 25, 46, 203, 79,
            37, 145, 51, 176, 174, 61, 136, 160, 107, 4, 95, 175, 144, // Key
        ];

        let mut client = Client::new(std::io::Cursor::new(vec![]));

        // We expect this to fail with an unexpected EOF, since the client will
        // attempt to read a response from the stream, but the stream is empty,
        // since we are not actually connected to SSH agent.
        assert!(
            matches!(client.remove_identity(ssh_agent_lib::proto::RemoveIdentity {
                pubkey: KeyData::Ed25519(Ed25519PublicKey(pk.to_byte_array())),
            }),
                Err(
                    super::AgentError::Proto(ssh_agent_lib::proto::ProtoError::IO(err)),
                ) if err.kind() == std::io::ErrorKind::UnexpectedEof
            )
        );

        assert_eq!(client.into_inner().into_inner(), expected.as_slice());
    }

    #[test]
    fn test_agent_encoding_sign() {
        use std::str::FromStr;

        let pk = PublicKey::from_str("z6MktWkM9vcfysWFq1c2aaLjJ6j4PYYg93TLPswR4qtuoAeT").unwrap();
        let expected = [
            0, 0, 0, 73, // Message length
            13, // Message type (sign request)
            0, 0, 0, 51, // Key blob length
            0, 0, 0, 11, // Key type length
            115, 115, 104, 45, 101, 100, 50, 53, 53, 49, 57, // Key type
            0, 0, 0, 32, // Public key
            208, 232, 92, 138, 225, 114, 116, 99, 156, 177, 148, 93, 65, 93, 198, 25, 46, 203, 79,
            37, 145, 51, 176, 174, 61, 136, 160, 107, 4, 95, 175, 144, // Key
            0, 0, 0, 9, // Length of data to sign
            1, 2, 3, 4, 5, 6, 7, 8, 9, // Data to sign
            0, 0, 0, 0, // Signature flags
        ];

        let mut client = Client::new(std::io::Cursor::new(vec![]));
        let data: Vec<u8> = vec![1, 2, 3, 4, 5, 6, 7, 8, 9];

        client
            .sign(SignRequest {
                pubkey: KeyData::Ed25519(Ed25519PublicKey(pk.to_byte_array())),
                data,
                flags: 0,
            })
            .ok();

        assert_eq!(client.into_inner().into_inner(), expected);
    }
}

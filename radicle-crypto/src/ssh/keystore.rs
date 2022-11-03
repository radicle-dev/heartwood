use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::{fs, io};

use thiserror::Error;

use crate::{KeyPair, PublicKey, SecretKey, Signature, Signer, SignerError};

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("ssh keygen: {0}")]
    Ssh(#[from] ssh_key::Error),
    #[error("invalid key type, expected ed25519 key")]
    InvalidKeyType,
    #[error("keystore already initialized")]
    AlreadyInitialized,
}

/// Stores keys on disk, in OpenSSH format.
#[derive(Debug)]
pub struct Keystore {
    path: PathBuf,
}

impl Keystore {
    /// Create a new keystore pointing to the given path. Use [`Keystore::init`] to initialize.
    pub fn new<P: AsRef<Path>>(path: &P) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }

    /// Get the path to the keystore.
    pub fn path(&self) -> &Path {
        self.path.as_path()
    }

    /// Initialize a keystore by generating a key pair and storing the secret and public key
    /// at the given path.
    ///
    /// The `comment` is associated with the private key.
    /// The `passphrase` is used to encrypt the private key.
    pub fn init(&self, comment: &str, passphrase: &str) -> Result<PublicKey, Error> {
        let pair = KeyPair::generate();
        let ssh_pair = ssh_key::private::Ed25519Keypair::from_bytes(&*pair)?;
        let ssh_pair = ssh_key::private::KeypairData::Ed25519(ssh_pair);
        let secret = ssh_key::PrivateKey::new(ssh_pair, comment)?;
        let secret = secret.encrypt(ssh_key::rand_core::OsRng, passphrase)?;
        let public = secret.public_key();
        let path = self.path.join("radicle");

        if path.exists() {
            return Err(Error::AlreadyInitialized);
        }

        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&self.path)?;

        secret.write_openssh_file(&path, ssh_key::LineEnding::default())?;
        public.write_openssh_file(&path.with_extension("pub"))?;

        Ok(pair.pk.into())
    }

    /// Load the public key from the store. Returns `None` if it wasn't found.
    pub fn public_key(&self) -> Result<Option<PublicKey>, Error> {
        let path = self.path.join("radicle.pub");
        if !path.exists() {
            return Ok(None);
        }

        let public = ssh_key::PublicKey::read_openssh_file(&path)?;
        match public.try_into() {
            Ok(public) => Ok(Some(public)),
            _ => Err(Error::InvalidKeyType),
        }
    }

    /// Load the secret key from the store, decrypting it with the given passphrase.
    /// Returns `None` if it wasn't found.
    pub fn secret_key(&self, passphrase: &str) -> Result<Option<SecretKey>, Error> {
        let path = self.path.join("radicle");
        if !path.exists() {
            return Ok(None);
        }

        let encrypted = ssh_key::PrivateKey::read_openssh_file(&path)?;
        let secret = encrypted.decrypt(passphrase)?;

        match secret.key_data() {
            ssh_key::private::KeypairData::Ed25519(pair) => {
                Ok(Some(SecretKey::new(pair.to_bytes())))
            }
            _ => Err(Error::InvalidKeyType),
        }
    }
}

#[derive(Debug, Error)]
pub enum MemorySignerError {
    #[error(transparent)]
    Keystore(#[from] Error),
    #[error("key not found in '{0}'")]
    NotFound(PathBuf),
}

/// An in-memory signer that keeps its secret key internally
/// so that signing never fails.
///
/// Can be created from a [`Keystore`] with the [`MemorySigner::load`] function.
#[derive(Debug)]
pub struct MemorySigner {
    public: PublicKey,
    secret: SecretKey,
}

impl Signer for MemorySigner {
    fn public_key(&self) -> &PublicKey {
        &self.public
    }

    fn sign(&self, msg: &[u8]) -> Signature {
        Signature(self.secret.sign(msg, None))
    }

    fn try_sign(&self, msg: &[u8]) -> Result<Signature, SignerError> {
        Ok(self.sign(msg))
    }
}

impl MemorySigner {
    /// Load this signer from a keystore, given a secret key passphrase.
    pub fn load(keystore: &Keystore, passphrase: &str) -> Result<Self, MemorySignerError> {
        let public = keystore
            .public_key()?
            .ok_or_else(|| MemorySignerError::NotFound(keystore.path().to_path_buf()))?;
        let secret = keystore
            .secret_key(passphrase)?
            .ok_or_else(|| MemorySignerError::NotFound(keystore.path().to_path_buf()))?;

        Ok(Self { public, secret })
    }

    /// Box this signer into a trait object.
    pub fn boxed(self) -> Box<dyn Signer> {
        Box::new(self)
    }
}

impl TryFrom<ssh_key::PublicKey> for PublicKey {
    type Error = Error;

    fn try_from(public: ssh_key::PublicKey) -> Result<Self, Self::Error> {
        match public.key_data() {
            ssh_key::public::KeyData::Ed25519(ssh_key::public::Ed25519PublicKey(data)) => {
                Ok(Self::from(*data))
            }
            _ => Err(Error::InvalidKeyType),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Keystore::new(&tmp.path());

        let public = store.init("test", "hunter").unwrap();
        assert_eq!(public, store.public_key().unwrap().unwrap());

        let secret = store.secret_key("hunter").unwrap().unwrap();
        assert_eq!(PublicKey::from(secret.public_key()), public);

        store.secret_key("blunder").unwrap_err(); // Wrong passphrase.
    }

    #[test]
    fn test_signer() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Keystore::new(&tmp.path());

        let public = store.init("test", "hunter").unwrap();
        let signer = MemorySigner::load(&store, "hunter").unwrap();

        assert_eq!(public, *signer.public_key());
    }
}

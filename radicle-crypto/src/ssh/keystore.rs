use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::{KeyPair, PublicKey, SecretKey};

#[derive(Debug, Error)]
pub enum Error {
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

    /// Initialize a keystore by generate a key pair and storing the secret and public keys
    /// at the given path.
    ///
    /// The comment is associated with the private key.
    /// The passphrase is used to encrypt the private key.
    ///
    pub fn init(self, comment: &str, passphrase: &str) -> Result<Self, Error> {
        let pair = KeyPair::generate();
        let pair = ssh_key::private::Ed25519Keypair::from_bytes(&*pair)?;
        let pair = ssh_key::private::KeypairData::Ed25519(pair);
        let secret = ssh_key::PrivateKey::new(pair, comment)?;
        let secret = secret.encrypt(ssh_key::rand_core::OsRng, passphrase)?;
        let public = secret.public_key();
        let path = self.path.join("radicle");

        if path.exists() {
            return Err(Error::AlreadyInitialized);
        }

        secret.write_openssh_file(&path, ssh_key::LineEnding::default())?;
        public.write_openssh_file(&path.with_extension("pub"))?;

        Ok(self)
    }

    /// Load the public key from the store. Returns `None` if it wasn't found.
    pub fn public_key(&self) -> Result<Option<PublicKey>, Error> {
        let path = self.path.join("radicle.pub");
        if !path.exists() {
            return Ok(None);
        }

        let public = ssh_key::PublicKey::read_openssh_file(&path)?;
        match public.key_data() {
            ssh_key::public::KeyData::Ed25519(ssh_key::public::Ed25519PublicKey(data)) => {
                Ok(Some(PublicKey::from(*data)))
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Keystore::new(&tmp.path());

        let store = store.init("test", "hunter").unwrap();
        let public = store.public_key().unwrap().unwrap();
        let secret = store.secret_key("hunter").unwrap().unwrap();

        assert_eq!(PublicKey::from(secret.public_key()), public);
        store.secret_key("blunder").unwrap_err(); // Wrong passphrase.
    }
}

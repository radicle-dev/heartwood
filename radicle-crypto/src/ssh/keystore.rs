use std::ops::Deref;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::{fs, io};

#[cfg(feature = "cyphernet")]
use cyphernet::{EcSk, EcSkInvalid, Ecdh};
use thiserror::Error;
use zeroize::Zeroizing;

use crate::{keypair, KeyPair, PublicKey, SecretKey, Signature, Signer, SignerError};

/// A secret key passphrase.
pub type Passphrase = Zeroizing<String>;

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
    #[error("keystore is encrypted; a passphrase is required")]
    PassphraseMissing,
}

impl Error {
    /// Check if it's a decryption error.
    pub fn is_crypto_err(&self) -> bool {
        matches!(self, Self::Ssh(ssh_key::Error::Crypto))
    }
}

/// Stores keys on disk, in OpenSSH format.
#[derive(Debug, Clone)]
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
    ///
    /// If `passphrase` is `None`, the key is not encrypted.
    pub fn init(&self, comment: &str, passphrase: Option<Passphrase>) -> Result<PublicKey, Error> {
        self.store(keypair::generate(), comment, passphrase)
    }

    /// Store a keypair on disk. Returns an error if the key already exists.
    pub fn store(
        &self,
        keypair: KeyPair,
        comment: &str,
        passphrase: Option<Passphrase>,
    ) -> Result<PublicKey, Error> {
        let ssh_pair = ssh_key::private::Ed25519Keypair::from_bytes(&keypair)?;
        let ssh_pair = ssh_key::private::KeypairData::Ed25519(ssh_pair);
        let secret = ssh_key::PrivateKey::new(ssh_pair, comment)?;
        let secret = if let Some(p) = passphrase {
            secret.encrypt(&mut ssh_key::rand_core::OsRng, p)?
        } else {
            secret
        };
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

        Ok(keypair.pk.into())
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
    pub fn secret_key(
        &self,
        passphrase: Option<Passphrase>,
    ) -> Result<Option<Zeroizing<SecretKey>>, Error> {
        let path = self.path.join("radicle");
        if !path.exists() {
            return Ok(None);
        }

        let secret = ssh_key::PrivateKey::read_openssh_file(&path)?;
        let secret = if let Some(p) = passphrase {
            secret.decrypt(p)?
        } else if secret.is_encrypted() {
            return Err(Error::PassphraseMissing);
        } else {
            secret
        };
        match secret.key_data() {
            ssh_key::private::KeypairData::Ed25519(pair) => {
                Ok(Some(SecretKey::from(pair.to_bytes()).into()))
            }
            _ => Err(Error::InvalidKeyType),
        }
    }

    /// Check that the passphrase is valid.
    pub fn is_valid_passphrase(&self, passphrase: &Passphrase) -> Result<bool, Error> {
        let path = self.path.join("radicle");
        if !path.exists() {
            return Err(Error::Io(io::ErrorKind::NotFound.into()));
        }

        let secret = ssh_key::PrivateKey::read_openssh_file(&path)?;
        let valid = secret.decrypt(passphrase).is_ok();

        Ok(valid)
    }

    /// Check whether the secret key is encrypted.
    pub fn is_encrypted(&self) -> Result<bool, Error> {
        let path = self.path.join("radicle");
        let secret = ssh_key::PrivateKey::read_openssh_file(&path)?;

        Ok(secret.is_encrypted())
    }
}

#[derive(Debug, Error)]
pub enum MemorySignerError {
    #[error(transparent)]
    Keystore(#[from] Error),
    #[error("key not found in '{0}'")]
    NotFound(PathBuf),
    #[error("invalid passphrase")]
    InvalidPassphrase,
}

/// An in-memory signer that keeps its secret key internally
/// so that signing never fails.
///
/// Can be created from a [`Keystore`] with the [`MemorySigner::load`] function.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct MemorySigner {
    public: PublicKey,
    secret: Zeroizing<SecretKey>,
}

impl Signer for MemorySigner {
    fn public_key(&self) -> &PublicKey {
        &self.public
    }

    fn sign(&self, msg: &[u8]) -> Signature {
        Signature(self.secret.deref().deref().sign(msg, None))
    }

    fn try_sign(&self, msg: &[u8]) -> Result<Signature, SignerError> {
        Ok(Signer::sign(self, msg))
    }
}

#[cfg(feature = "cyphernet")]
impl EcSk for MemorySigner {
    type Pk = PublicKey;

    fn generate_keypair() -> (Self, Self::Pk)
    where
        Self: Sized,
    {
        let ms = Self::gen();
        let pk = ms.public;

        (ms, pk)
    }

    fn to_pk(&self) -> Result<Self::Pk, EcSkInvalid> {
        Ok(self.public)
    }
}

#[cfg(feature = "cyphernet")]
impl Ecdh for MemorySigner {
    type SharedSecret = [u8; 32];

    fn ecdh(&self, pk: &Self::Pk) -> Result<Self::SharedSecret, cyphernet::EcdhError> {
        self.secret.ecdh(pk).map_err(cyphernet::EcdhError::from)
    }
}

impl MemorySigner {
    /// Load this signer from a keystore, given a secret key passphrase.
    pub fn load(
        keystore: &Keystore,
        passphrase: Option<Passphrase>,
    ) -> Result<Self, MemorySignerError> {
        let public = keystore
            .public_key()?
            .ok_or_else(|| MemorySignerError::NotFound(keystore.path().to_path_buf()))?;
        let secret = keystore
            .secret_key(passphrase)
            .map_err(|e| {
                if e.is_crypto_err() {
                    MemorySignerError::InvalidPassphrase
                } else {
                    e.into()
                }
            })?
            .ok_or_else(|| MemorySignerError::NotFound(keystore.path().to_path_buf()))?;

        Ok(Self { public, secret })
    }

    /// Box this signer into a trait object.
    pub fn boxed(self) -> Box<dyn Signer> {
        Box::new(self)
    }

    /// Generate a new memory signer.
    pub fn gen() -> Self {
        let keypair = KeyPair::generate();
        let sk = keypair.sk;

        Self {
            public: sk.public_key().into(),
            secret: Zeroizing::new(sk.into()),
        }
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
    fn test_init_passphrase() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Keystore::new(&tmp.path());

        let public = store
            .init("test", Some("hunter".to_owned().into()))
            .unwrap();
        assert_eq!(public, store.public_key().unwrap().unwrap());
        assert!(store.is_encrypted().unwrap());

        let secret = store
            .secret_key(Some("hunter".to_owned().into()))
            .unwrap()
            .unwrap();
        assert_eq!(PublicKey::from(secret.public_key()), public);

        store
            .secret_key(Some("blunder".to_owned().into()))
            .unwrap_err(); // Wrong passphrase.
    }

    #[test]
    fn test_init_no_passphrase() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Keystore::new(&tmp.path());

        let public = store.init("test", None).unwrap();
        assert_eq!(public, store.public_key().unwrap().unwrap());
        assert!(!store.is_encrypted().unwrap());

        let secret = store.secret_key(None).unwrap().unwrap();
        assert_eq!(PublicKey::from(secret.public_key()), public);
    }

    #[test]
    fn test_signer() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Keystore::new(&tmp.path());

        let public = store
            .init("test", Some("hunter".to_owned().into()))
            .unwrap();
        let signer = MemorySigner::load(&store, Some("hunter".to_owned().into())).unwrap();

        assert_eq!(public, *signer.public_key());
    }
}

use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::{fs, io};

#[cfg(feature = "cyphernet")]
use cyphernet::{EcSk, EcSkInvalid, Ecdh};
use thiserror::Error;
use zeroize::Zeroizing;

use crate::{KeyPair, PublicKey, SecretKey, Signature, Signer, SignerError};

use super::ExtendedSignature;

/// A secret key passphrase.
pub type Passphrase = Zeroizing<String>;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("ssh keygen: {0}")]
    Ssh(#[from] ssh_key::Error),
    #[error("invalid key type, expected ed25519 key")]
    InvalidKeyType,
    #[error("keystore already initialized, file '{exists}' exists")]
    AlreadyInitialized { exists: PathBuf },
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
    path_secret: PathBuf,
    path_public: Option<PathBuf>,
}

impl Keystore {
    /// Create a new keystore pointing to the given path.
    ///
    /// Use [`Keystore::init`] to initialize.
    pub fn new<P: AsRef<Path>>(path: &P) -> Self {
        const DEFAULT_SECRET_KEY_FILE_NAME: &str = "radicle";
        const DEFAULT_PUBLIC_KEY_FILE_NAME: &str = "radicle.pub";

        let keys = path.as_ref().to_path_buf();

        Self {
            path_secret: keys.join(DEFAULT_SECRET_KEY_FILE_NAME),
            path_public: Some(keys.join(DEFAULT_PUBLIC_KEY_FILE_NAME)),
        }
    }

    /// Create a new keystore pointing to the given paths.
    ///
    /// Use [`Keystore::init`] to initialize.
    pub fn from_secret_path<P: AsRef<Path>>(secret: &P) -> Self {
        Self {
            path_secret: secret.as_ref().to_path_buf(),
            path_public: None,
        }
    }

    /// Get the path to the secret key backing the keystore.
    pub fn secret_key_path(&self) -> &Path {
        self.path_secret.as_path()
    }

    /// Get the path to the public key backing the keystore, if present.
    pub fn public_key_path(&self) -> Option<&Path> {
        self.path_public.as_deref()
    }

    /// Initialize a keystore by generating a key pair and storing the secret
    /// and public key at the given path.
    ///
    /// The `comment` is associated with the private key. The `passphrase` is
    /// used to encrypt the private key. The `seed` is used to derive the
    /// private key and should almost always be generated.
    ///
    /// If `passphrase` is `None`, the key is not encrypted.
    pub fn init(
        &self,
        comment: &str,
        passphrase: Option<Passphrase>,
        seed: ec25519::Seed,
    ) -> Result<PublicKey, Error> {
        self.store(KeyPair::from_seed(seed), comment, passphrase)
    }

    /// Store a keypair on disk. Returns an error if any of the two key files already exist.
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

        if self.path_secret.exists() {
            return Err(Error::AlreadyInitialized {
                exists: self.path_secret.to_path_buf(),
            });
        }

        if let Some(path_public) = &self.path_public {
            if path_public.exists() {
                return Err(Error::AlreadyInitialized {
                    exists: path_public.to_path_buf(),
                });
            }
        }

        // NOTE: If [`PathBuf::parent`] returns `None`,
        // then the path is at root or empty, so don't
        // attempt to create any parents.
        self.path_secret.parent().map_or(Ok(()), |parent| {
            let mut builder = fs::DirBuilder::new();
            builder.recursive(true);

            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt as _;
                builder.mode(0o700);
            }

            builder.create(parent)
        })?;
        secret.write_openssh_file(&self.path_secret, ssh_key::LineEnding::default())?;

        if let Some(path_public) = &self.path_public {
            path_public.parent().map_or(Ok(()), |parent| {
                let mut builder = fs::DirBuilder::new();
                builder.recursive(true);

                #[cfg(unix)]
                {
                    use std::os::unix::fs::DirBuilderExt as _;
                    builder.mode(0o700);
                }

                builder.create(parent)
            })?;
            public.write_openssh_file(path_public)?;
        }

        Ok(keypair.pk.into())
    }

    /// Load the public key from the store. Returns `None` if it wasn't found.
    pub fn public_key(&self) -> Result<Option<PublicKey>, Error> {
        let Some(path_public) = &self.path_public else {
            return Ok(None);
        };

        if !path_public.exists() {
            return Ok(None);
        }

        let public = ssh_key::PublicKey::read_openssh_file(path_public)?;
        PublicKey::try_from(public)
            .map(Some)
            .map_err(|_| Error::InvalidKeyType)
    }

    /// Load the secret key from the store, decrypting it with the given passphrase.
    /// Returns `None` if it wasn't found.
    pub fn secret_key(
        &self,
        passphrase: Option<Passphrase>,
    ) -> Result<Option<Zeroizing<SecretKey>>, Error> {
        let path = &self.path_secret;
        if !path.exists() {
            return Ok(None);
        }

        let secret = ssh_key::PrivateKey::read_openssh_file(path)?;

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
        if !self.path_secret.exists() {
            return Err(Error::Io(io::ErrorKind::NotFound.into()));
        }

        let secret = ssh_key::PrivateKey::read_openssh_file(&self.path_secret)?;
        let valid = secret.decrypt(passphrase).is_ok();

        Ok(valid)
    }

    /// Check whether the secret key is encrypted.
    pub fn is_encrypted(&self) -> Result<bool, Error> {
        let secret = ssh_key::PrivateKey::read_openssh_file(&self.path_secret)?;

        Ok(secret.is_encrypted())
    }
}

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum MemorySignerError {
    #[error(transparent)]
    Keystore(#[from] Error),
    #[error("key not found in '{0}'")]
    NotFound(PathBuf),
    #[error("invalid passphrase")]
    InvalidPassphrase,
    #[error("secret key '{secret}' and public key '{public}' do not match")]
    KeyMismatch { secret: PathBuf, public: PathBuf },
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

impl signature::Signer<Signature> for MemorySigner {
    fn try_sign(&self, msg: &[u8]) -> Result<Signature, signature::Error> {
        Ok(Signer::sign(self, msg))
    }
}

impl signature::Signer<ExtendedSignature> for MemorySigner {
    fn try_sign(&self, msg: &[u8]) -> Result<ExtendedSignature, signature::Error> {
        Ok(ExtendedSignature {
            key: self.public,
            sig: Signer::sign(self, msg),
        })
    }
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
        let secret = keystore
            .secret_key(passphrase)
            .map_err(|e| {
                if e.is_crypto_err() {
                    MemorySignerError::InvalidPassphrase
                } else {
                    e.into()
                }
            })?
            .ok_or_else(|| MemorySignerError::NotFound(keystore.secret_key_path().to_path_buf()))?;

        let Some(public_path) = keystore.public_key_path() else {
            // There is no public key in the key store, so there's nothing
            // to validate. Derive it from the secret key.
            return Ok(Self::from_secret(secret));
        };

        let public = keystore
            .public_key()?
            .ok_or_else(|| MemorySignerError::NotFound(public_path.to_path_buf()))?;

        secret
            .validate_public_key(&public)
            .map_err(|_| MemorySignerError::KeyMismatch {
                secret: keystore.secret_key_path().to_path_buf(),
                public: public_path.to_path_buf(),
            })?;

        Ok(Self { public, secret })
    }

    /// Create a new memory signer from the given secret key, deriving
    /// the public key from the secret key.
    pub fn from_secret(secret: Zeroizing<SecretKey>) -> Self {
        Self {
            public: PublicKey(secret.public_key()),
            secret,
        }
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
        let store = Keystore::new(&tmp);

        let public = store
            .init(
                "test",
                Some("hunter".to_owned().into()),
                ec25519::Seed::default(),
            )
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
        let store = Keystore::new(&tmp);

        let public = store.init("test", None, ec25519::Seed::default()).unwrap();
        assert_eq!(public, store.public_key().unwrap().unwrap());
        assert!(!store.is_encrypted().unwrap());

        let secret = store.secret_key(None).unwrap().unwrap();
        assert_eq!(PublicKey::from(secret.public_key()), public);
    }

    #[test]
    fn test_signer() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Keystore::new(&tmp);

        let public = store
            .init(
                "test",
                Some("hunter".to_owned().into()),
                ec25519::Seed::default(),
            )
            .unwrap();
        let signer = MemorySigner::load(&store, Some("hunter".to_owned().into())).unwrap();

        assert_eq!(public, *signer.public_key());
    }
}

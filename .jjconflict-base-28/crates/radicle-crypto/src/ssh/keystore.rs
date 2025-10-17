extern crate std;

use std::path::{Path, PathBuf};
use std::string::String;
use std::{fs, io};

use thiserror::Error;
use zeroize::Zeroizing;

use crate::{PublicKey, Seed, SigningKey};

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
    #[error(transparent)]
    Signature(#[from] crate::signature::Error),
    #[error("invalid key")]
    Invalid,
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
        seed: Seed,
    ) -> Result<PublicKey, Error> {
        let signing_key = SigningKey::from_seed(seed);
        self.store(&signing_key, comment, passphrase)?;
        Ok(*signing_key.public_key())
    }

    /// Store a keypair on disk. Returns an error if any of the two key files already exist.
    pub fn store(
        &self,
        keypair: &SigningKey,
        comment: &str,
        passphrase: Option<Passphrase>,
    ) -> Result<(), Error> {
        let keypair_bytes = keypair.to_keypair_bytes();
        let ssh_pair = ssh_key::private::Ed25519Keypair::from_bytes(&keypair_bytes)?;
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

        if let Some(path_public) = &self.path_public
            && path_public.exists()
        {
            return Err(Error::AlreadyInitialized {
                exists: path_public.to_path_buf(),
            });
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

        Ok(())
    }

    /// Load the public key from the store. Returns `None` if it wasn't found.
    pub fn public_key(&self) -> Result<Option<PublicKey>, Error> {
        use KeyData::*;
        use ssh_key::{PublicKey as SshPublicKey, public::KeyData};

        let Some(path_public) = &self.path_public else {
            return Ok(None);
        };

        if !path_public.exists() {
            return Ok(None);
        }

        match KeyData::from(SshPublicKey::read_openssh_file(path_public)?) {
            Ed25519(key) => Ok(Some(PublicKey::from(key))),
            _ => Err(Error::InvalidKeyType),
        }
    }

    /// Load the secret key from the store, decrypting it with the given passphrase.
    /// Returns `None` if it wasn't found.
    pub fn secret_key(&self, passphrase: Option<Passphrase>) -> Result<Option<SigningKey>, Error> {
        use KeypairData::*;
        use ssh_key::{PrivateKey, private::KeypairData};

        let path = &self.path_secret;
        if !path.exists() {
            return Ok(None);
        }

        let secret = PrivateKey::read_openssh_file(path)?;

        let secret = if let Some(p) = passphrase {
            secret.decrypt(p)?
        } else if secret.is_encrypted() {
            return Err(Error::PassphraseMissing);
        } else {
            secret
        };
        match secret.key_data() {
            Ed25519(pair) => Ok(Some(SigningKey::try_from(pair.to_bytes())?)),
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

#[cfg(test)]
mod tests {
    use super::*;

    use std::borrow::ToOwned as _;

    #[test]
    fn test_init_passphrase() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Keystore::new(&tmp);

        let public = store
            .init("test", Some("hunter".to_owned().into()), Seed::mock(1))
            .unwrap();
        assert_eq!(public, store.public_key().unwrap().unwrap());
        assert!(store.is_encrypted().unwrap());

        let secret = store
            .secret_key(Some("hunter".to_owned().into()))
            .unwrap()
            .unwrap();

        let secret_public = secret.public_key();

        assert_eq!(secret_public, &public);

        store
            .secret_key(Some("blunder".to_owned().into()))
            .unwrap_err(); // Wrong passphrase.
    }

    #[test]
    fn test_init_no_passphrase() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Keystore::new(&tmp);

        let public = store.init("test", None, Seed::mock(1)).unwrap();
        assert_eq!(public, store.public_key().unwrap().unwrap());
        assert!(!store.is_encrypted().unwrap());

        let secret = store.secret_key(None).unwrap().unwrap();
        let secret_public = secret.public_key();
        assert_eq!(secret_public, &public);
    }

    #[test]
    fn test_signer() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Keystore::new(&tmp);

        let public = store
            .init("test", Some("hunter".to_owned().into()), Seed::mock(1))
            .unwrap();
        let signer = SigningKey::load(&store, Some("hunter".to_owned().into())).unwrap();

        assert_eq!(&public, signer.public_key());
    }
}

pub mod agent;
pub mod keystore;

use std::io;

use thiserror::Error;

use radicle_ssh::encoding;
use radicle_ssh::encoding::Encodable;
use radicle_ssh::encoding::Encoding;
use radicle_ssh::encoding::Reader;

use crate as crypto;
use crate::PublicKey;

pub use keystore::{Keystore, Passphrase};

#[derive(Debug, Error)]
pub enum ExtendedSignatureError {
    #[error(transparent)]
    Ssh(#[from] ssh_key::Error),
    #[error(transparent)]
    Crypto(#[from] crypto::Error),
    #[error("unsupported signature algorithm")]
    UnsupportedAlgorithm,
}

/// Signature with public key, used for SSH signing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtendedSignature {
    pub key: crypto::PublicKey,
    pub sig: crypto::Signature,
}

impl From<ExtendedSignature> for crypto::Signature {
    fn from(ExtendedSignature { sig, .. }: ExtendedSignature) -> Self {
        sig
    }
}

impl ExtendedSignature {
    /// Create a new extended signature.
    pub fn new(public_key: crypto::PublicKey, signature: crypto::Signature) -> Self {
        Self {
            key: public_key,
            sig: signature,
        }
    }

    /// Convert to OpenSSH standard PEM format.
    pub fn to_pem(&self) -> Result<String, ExtendedSignatureError> {
        ssh_key::SshSig::new(
            ssh_key::public::KeyData::from(ssh_key::public::Ed25519PublicKey(**self.key)),
            String::from("radicle"),
            ssh_key::HashAlg::Sha256,
            ssh_key::Signature::new(ssh_key::Algorithm::Ed25519, **self.sig)?,
        )?
        .to_pem(ssh_key::LineEnding::default())
        .map_err(ExtendedSignatureError::from)
    }

    /// Create from OpenSSH PEM format.
    pub fn from_pem(pem: impl AsRef<[u8]>) -> Result<Self, ExtendedSignatureError> {
        let sig = ssh_key::SshSig::from_pem(pem)?;

        Ok(Self {
            key: crypto::PublicKey::from(
                sig.public_key()
                    .ed25519()
                    .ok_or(ExtendedSignatureError::UnsupportedAlgorithm)?
                    .0,
            ),
            sig: crypto::Signature::try_from(sig.signature().as_bytes())?,
        })
    }

    /// Verify the signature for a given payload.
    pub fn verify(&self, payload: &[u8]) -> bool {
        self.key.verify(payload, &self.sig).is_ok()
    }
}

pub mod fmt {
    use crate::PublicKey;

    /// Get the SSH long key from a public key.
    /// This is the output of `ssh-add -L`.
    pub fn key(key: &PublicKey) -> String {
        ssh_key::PublicKey::from(*key).to_string()
    }

    /// Get the SSH key fingerprint from a public key.
    /// This is the output of `ssh-add -l`.
    pub fn fingerprint(key: &PublicKey) -> String {
        ssh_key::PublicKey::from(*key)
            .fingerprint(Default::default())
            .to_string()
    }

    #[cfg(test)]
    mod test {
        use std::str::FromStr;

        use super::*;
        use crate::PublicKey;

        #[test]
        fn test_key() {
            let pk =
                PublicKey::from_str("z6MktWkM9vcfysWFq1c2aaLjJ6j4PYYg93TLPswR4qtuoAeT").unwrap();

            assert_eq!(
                key(&pk),
                "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAINDoXIrhcnRjnLGUXUFdxhkuy08lkTOwrj2IoGsEX6+Q"
            );
        }

        #[test]
        fn test_fingerprint() {
            let pk =
                PublicKey::from_str("z6MktWkM9vcfysWFq1c2aaLjJ6j4PYYg93TLPswR4qtuoAeT").unwrap();
            assert_eq!(
                fingerprint(&pk),
                "SHA256:gE/Ty4fuXzww49lcnNe9/GI0L7xSEQdFp/v9tOjFwB4"
            );
        }
    }
}

#[derive(Debug, Error)]
pub enum SignatureError {
    #[error(transparent)]
    Invalid(#[from] crypto::Error),
    #[error(transparent)]
    Encoding(#[from] encoding::Error),
    #[error("unknown algorithm '{0}'")]
    UnknownAlgorithm(String),
}

impl Encodable for crypto::Signature {
    type Error = SignatureError;

    fn read(r: &mut encoding::Cursor) -> Result<Self, Self::Error> {
        let buf = r.read_string()?;
        let mut inner_strs = buf.reader(0);

        let sig_type = inner_strs.read_string()?;
        if sig_type != b"ssh-ed25519" {
            return Err(SignatureError::UnknownAlgorithm(
                String::from_utf8_lossy(sig_type).to_string(),
            ));
        }
        let sig = crypto::Signature::try_from(inner_strs.read_string()?)?;

        Ok(sig)
    }

    fn write<E: Encoding>(&self, buf: &mut E) {
        let mut inner_strs = Vec::new();
        inner_strs.extend_ssh_string(b"ssh-ed25519");
        inner_strs.extend_ssh_string(self.as_ref());
        buf.extend_ssh_string(&inner_strs);
    }
}

#[derive(Debug, Error)]
pub enum PublicKeyError {
    #[error(transparent)]
    Invalid(#[from] crypto::Error),
    #[error(transparent)]
    Encoding(#[from] encoding::Error),
    #[error("unknown algorithm '{0}'")]
    UnknownAlgorithm(String),
}

impl Encodable for PublicKey {
    type Error = PublicKeyError;

    fn read(r: &mut encoding::Cursor) -> Result<Self, Self::Error> {
        match r.read_string()? {
            b"ssh-ed25519" => {
                let s = r.read_string()?;
                let p = PublicKey::try_from(s)?;

                Ok(p)
            }
            v => Err(PublicKeyError::UnknownAlgorithm(
                String::from_utf8_lossy(v).to_string(),
            )),
        }
    }

    fn write<E: Encoding>(&self, w: &mut E) {
        let mut str_w: Vec<u8> = Vec::<u8>::new();
        str_w.extend_ssh_string(b"ssh-ed25519");
        str_w.extend_ssh_string(&self[..]);
        w.extend_ssh_string(&str_w)
    }
}

#[derive(Debug, Error)]
pub enum SecretKeyError {
    #[error(transparent)]
    Encoding(#[from] encoding::Error),
    #[error(transparent)]
    Crypto(#[from] crypto::Error),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("unknown algorithm '{0}'")]
    UnknownAlgorithm(String),
    #[error("public key does not match secret key")]
    Mismatch,
}

impl Encodable for crypto::SecretKey {
    type Error = SecretKeyError;

    fn read(r: &mut encoding::Cursor) -> Result<Self, Self::Error> {
        match r.read_string()? {
            b"ssh-ed25519" => {
                let public = r.read_string()?;
                let pair = r.read_string()?;
                let _comment = r.read_string()?;
                let key = crypto::SecretKey::try_from(pair)?;

                if public != key.public_key().as_ref() {
                    return Err(SecretKeyError::Mismatch);
                }
                Ok(key)
            }
            s => Err(SecretKeyError::UnknownAlgorithm(
                String::from_utf8_lossy(s).to_string(),
            )),
        }
    }

    fn write<E: Encoding>(&self, buf: &mut E) {
        let public = self.0.public_key();

        buf.extend_ssh_string(b"ssh-ed25519");
        buf.extend_ssh_string(public.as_ref());
        buf.extend_ssh_string(self.0.as_ref());
        buf.extend_ssh_string(b"radicle");
    }
}

#[cfg(test)]
mod test {
    use std::sync::{Arc, Mutex};

    use qcheck_macros::quickcheck;

    use crate as crypto;
    use crate::{PublicKey, SecretKey};
    use radicle_ssh::agent::client::{AgentClient, ClientStream, Error};
    use radicle_ssh::encoding::*;

    #[derive(Clone, Default)]
    struct DummyStream {
        incoming: Arc<Mutex<Vec<u8>>>,
    }

    impl ClientStream for DummyStream {
        fn connect<P>(_path: P) -> Result<AgentClient<Self>, Error>
        where
            P: AsRef<std::path::Path> + Send,
        {
            panic!("This function should never be called!")
        }

        fn request(&mut self, buf: &[u8]) -> Result<Buffer, Error> {
            *self.incoming.lock().unwrap() = buf.to_vec();

            Ok(Buffer::default())
        }
    }

    #[quickcheck]
    fn prop_encode_decode_sk(input: [u8; 64]) {
        let mut buf = Buffer::default();
        let sk = crypto::SecretKey::from(input);
        sk.write(&mut buf);

        let mut cursor = buf.reader(0);
        let output = SecretKey::read(&mut cursor).unwrap();

        assert_eq!(sk, output);
    }

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

        let stream = DummyStream::default();
        let mut agent = AgentClient::connect(stream.clone());

        agent.remove_identity(&pk).unwrap();

        assert_eq!(
            stream.incoming.lock().unwrap().as_slice(),
            expected.as_slice()
        );
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

        let stream = DummyStream::default();
        let mut agent = AgentClient::connect(stream.clone());
        let data: Vec<u8> = vec![1, 2, 3, 4, 5, 6, 7, 8, 9];

        agent.sign(&pk, &data).ok();

        assert_eq!(
            stream.incoming.lock().unwrap().as_slice(),
            expected.as_slice()
        );
    }
}

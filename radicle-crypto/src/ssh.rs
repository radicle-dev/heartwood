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

pub mod fmt {
    use radicle_ssh::encoding::Encoding as _;

    use crate::PublicKey;

    /// Get the SSH long key from a public key.
    /// This is the output of `ssh-add -L`.
    pub fn key(key: &PublicKey) -> String {
        let mut buf = Vec::new();

        buf.extend_ssh_string(b"ssh-ed25519");
        buf.extend_ssh_string(key.as_ref());

        base64::encode_config(buf, base64::STANDARD_NO_PAD)
    }

    /// Get the SSH key fingerprint from a public key.
    /// This is the output of `ssh-add -l`.
    pub fn fingerprint(key: &PublicKey) -> String {
        use sha2::Digest;

        let mut buf = Vec::new();

        buf.extend_ssh_string(b"ssh-ed25519");
        buf.extend_ssh_string(key.as_ref());

        let sha = sha2::Sha256::digest(&buf).to_vec();
        let encoded = base64::encode_config(sha, base64::STANDARD_NO_PAD);

        format!("SHA256:{encoded}")
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
                "AAAAC3NzaC1lZDI1NTE5AAAAINDoXIrhcnRjnLGUXUFdxhkuy08lkTOwrj2IoGsEX6+Q"
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

#[derive(Debug, Error)]
pub enum ExtendedSignatureError {
    #[error(transparent)]
    Base64Encoding(#[from] base64::DecodeError),
    #[error("wrong preamble")]
    MagicPreamble([u8; 6]),
    #[error("missing armored footer")]
    MissingFooter,
    #[error("missing armored header")]
    MissingHeader,
    #[error(transparent)]
    Encoding(#[from] encoding::Error),
    #[error(transparent)]
    PublicKey(#[from] PublicKeyError),
    #[error(transparent)]
    SignatureError(#[from] SignatureError),
    #[error("unsupported version '{0}'")]
    UnsupportedVersion(u32),
}

/// An SSH signature's decoded format.
///
/// See <https://github.com/openssh/openssh-portable/blob/master/PROTOCOL.sshsig>
#[derive(Clone, Debug)]
pub struct ExtendedSignature {
    version: u32,
    public_key: crypto::PublicKey,
    /// Unambigious interpretation domain to prevent cross-protocol attacks.
    namespace: Vec<u8>,
    reserved: Vec<u8>,
    /// Hash used for signature. For example 'sha256'.
    hash_algorithm: Vec<u8>,
    signature: crypto::Signature,
}

impl From<ExtendedSignature> for (crypto::PublicKey, crypto::Signature) {
    fn from(ex: ExtendedSignature) -> Self {
        (ex.public_key, ex.signature)
    }
}

impl Encodable for ExtendedSignature {
    type Error = ExtendedSignatureError;

    fn read(r: &mut encoding::Cursor) -> Result<Self, Self::Error> {
        let sig_version = r.read_u32()?;
        if sig_version > 1 {
            return Err(ExtendedSignatureError::UnsupportedVersion(sig_version));
        }
        let mut pk = r.read_string()?.reader(0);

        Ok(ExtendedSignature {
            version: sig_version,
            public_key: PublicKey::read(&mut pk)?,
            namespace: r.read_string()?.into(),
            reserved: r.read_string()?.into(),
            hash_algorithm: r.read_string()?.into(),
            signature: crypto::Signature::read(r)?,
        })
    }

    fn write<E: Encoding>(&self, buf: &mut E) {
        buf.extend_u32(self.version);
        let _ = &self.public_key.write(buf);
        buf.extend_ssh_string(&self.namespace);
        buf.extend_ssh_string(&self.reserved);
        buf.extend_ssh_string(&self.hash_algorithm);
        let _ = &self.signature.write(buf);
    }
}

impl ExtendedSignature {
    const ARMORED_HEADER: &[u8] = b"-----BEGIN SSH SIGNATURE-----";
    const ARMORED_FOOTER: &[u8] = b"-----END SSH SIGNATURE-----";
    const ARMORED_WIDTH: usize = 70;
    const MAGIC_PREAMBLE: &[u8] = b"SSHSIG";

    pub fn new(public_key: crypto::PublicKey, signature: crypto::Signature) -> Self {
        Self {
            version: 1,
            public_key,
            namespace: b"radicle".to_vec(),
            reserved: b"".to_vec(),
            hash_algorithm: b"sha256".to_vec(),
            signature,
        }
    }

    pub fn from_armored(s: &[u8]) -> Result<Self, ExtendedSignatureError> {
        let s = s
            .strip_prefix(Self::ARMORED_HEADER)
            .ok_or(ExtendedSignatureError::MissingHeader)?;
        let s = s
            .strip_suffix(Self::ARMORED_FOOTER)
            .ok_or(ExtendedSignatureError::MissingFooter)?;
        let s: Vec<u8> = s.iter().filter(|b| *b != &b'\n').copied().collect();

        let buf = base64::decode(s)?;
        let mut reader = buf.reader(0);

        let preamble: [u8; 6] = reader.read_bytes()?;
        if preamble != Self::MAGIC_PREAMBLE {
            return Err(ExtendedSignatureError::MagicPreamble(preamble));
        }

        ExtendedSignature::read(&mut reader)
    }

    pub fn to_armored(&self) -> Vec<u8> {
        let mut buf = encoding::Buffer::from(Self::MAGIC_PREAMBLE.to_vec());
        self.write(&mut buf);

        let mut armored = Self::ARMORED_HEADER.to_vec();
        armored.push(b'\n');

        let body = base64::encode(buf);
        for line in body.as_bytes().chunks(Self::ARMORED_WIDTH) {
            armored.extend(line);
            armored.push(b'\n');
        }

        armored.extend(Self::ARMORED_FOOTER);
        armored
    }
}

#[cfg(test)]
mod test {
    use std::sync::{Arc, Mutex};

    use qcheck_macros::quickcheck;

    use super::{fmt, ExtendedSignature};
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

    #[test]
    fn test_signature_encode_decode() {
        let armored: &[u8] = b"-----BEGIN SSH SIGNATURE-----
U1NIU0lHAAAAAQAAADMAAAALc3NoLWVkMjU1MTkAAAAgvjrQogRxxLjzzWns8+mKJAGzEX
4fm2ALoN7pyvD2ttQAAAADZ2l0AAAAAAAAAAZzaGE1MTIAAABTAAAAC3NzaC1lZDI1NTE5
AAAAQI84aPZsXxlQigpy1/Y/iJSmHSS//CIgvqvUMQIb/TM2vhCKruduH0cK02k9G8wOI+
EUMf2bSDyxbJyZThOEiAs=
-----END SSH SIGNATURE-----";

        let public_key = "AAAAC3NzaC1lZDI1NTE5AAAAIL460KIEccS4881p7PPpiiQBsxF+H5tgC6De6crw9rbU";
        let signature = ExtendedSignature::from_armored(armored).unwrap();

        assert_eq!(signature.version, 1);
        assert_eq!(fmt::key(&signature.public_key), public_key);
        assert_eq!(
            String::from_utf8(armored.to_vec()),
            String::from_utf8(signature.to_armored()),
            "signature should remain unaltered after decoding"
        );
    }

    #[test]
    fn test_signature_verify() {
        let seed = crypto::Seed::new([1; 32]);
        let pair = crypto::KeyPair::from_seed(seed);
        let message = &[0xff];
        let sig = pair.sk.sign(message, None);
        let esig = ExtendedSignature {
            version: 1,
            public_key: pair.pk.into(),
            signature: sig.into(),
            hash_algorithm: vec![],
            namespace: vec![],
            reserved: vec![],
        };

        let armored = esig.to_armored();
        let unarmored = ExtendedSignature::from_armored(&armored).unwrap();

        unarmored
            .public_key
            .verify(message, &unarmored.signature)
            .unwrap();
    }
}

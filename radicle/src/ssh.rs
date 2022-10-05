pub mod agent;

use std::io;
use std::ops::DerefMut;

use byteorder::{BigEndian, WriteBytesExt};
use thiserror::Error;
use zeroize::Zeroizing;

use radicle_ssh::encoding;
use radicle_ssh::encoding::Encodable;
use radicle_ssh::encoding::Encoding;
use radicle_ssh::encoding::Reader;
use radicle_ssh::key::Public;

use crate::crypto;
use crate::crypto::PublicKey;

pub mod fmt {
    use radicle_ssh::encoding::Encoding as _;

    use crate::crypto::PublicKey;

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

        format!("SHA256:{}", encoded)
    }

    #[cfg(test)]
    mod test {
        use std::str::FromStr;

        use super::*;
        use crate::crypto::PublicKey;

        #[test]
        fn test_key() {
            let pk = PublicKey::from_str("zF4VJZgNEeL1niWmKu1NtT1B4ZyGpjACyhs2VEZvtsws5").unwrap();

            assert_eq!(
                key(&pk),
                "AAAAC3NzaC1lZDI1NTE5AAAAINDoXIrhcnRjnLGUXUFdxhkuy08lkTOwrj2IoGsEX6+Q"
            );
        }

        #[test]
        fn test_fingerprint() {
            let pk = PublicKey::from_str("zF4VJZgNEeL1niWmKu1NtT1B4ZyGpjACyhs2VEZvtsws5").unwrap();

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

    fn read_ssh(r: &mut encoding::Cursor) -> Result<Self, Self::Error> {
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

    fn write_ssh<E: Encoding>(&self, buf: &mut E) {
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

    fn read_ssh(r: &mut encoding::Cursor) -> Result<Self, Self::Error> {
        let buf = r.read_string()?;
        let mut str_r = buf.reader(0);
        match str_r.read_string()? {
            b"ssh-ed25519" => {
                let s = str_r.read_string()?;
                let p = PublicKey::try_from(s)?;

                Ok(p)
            }
            v => Err(PublicKeyError::UnknownAlgorithm(
                String::from_utf8_lossy(v).to_string(),
            )),
        }
    }

    fn write_ssh<E: Encoding>(&self, w: &mut E) {
        _ = self.write(w);
    }
}

impl Public for PublicKey {
    type Error = PublicKeyError;

    fn read(r: &mut encoding::Cursor) -> Result<Option<Self>, Self::Error> {
        match r.read_string()? {
            b"ssh-ed25519" => {
                let s = r.read_string()?;
                let p = PublicKey::try_from(s)?;

                Ok(Some(p))
            }
            _ => Ok(None),
        }
    }

    fn write<E: Encoding>(&self, buf: &mut E) -> usize {
        let mut str_w: Vec<u8> = Vec::<u8>::new();
        str_w.extend_ssh_string(b"ssh-ed25519");
        str_w.extend_ssh_string(&self[..]);
        buf.extend_ssh_string(&str_w)
    }
}

// FIXME: Should zeroize, or we should be creating our own type
// in `crypto`.
struct SecretKey(crypto::SecretKey);

impl From<crypto::SecretKey> for SecretKey {
    fn from(other: crypto::SecretKey) -> Self {
        Self(other)
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
    #[error("public key does not match secret key")]
    Mismatch,
}

impl radicle_ssh::key::Private for SecretKey {
    type Error = SecretKeyError;

    fn read(r: &mut encoding::Cursor) -> Result<Option<Self>, Self::Error> {
        match r.read_string()? {
            b"ssh-ed25519" => {
                let public = r.read_string()?;
                let pair = r.read_string()?;
                let _comment = r.read_string()?;
                let key = crypto::SecretKey::from_slice(pair)?;

                if public != key.public_key().as_ref() {
                    return Err(SecretKeyError::Mismatch);
                }
                Ok(Some(SecretKey(key)))
            }
            _ => Ok(None),
        }
    }

    fn write(&self, buf: &mut Zeroizing<Vec<u8>>) -> Result<(), Self::Error> {
        let public = self.0.public_key();

        buf.extend_ssh_string(b"ssh-ed25519");
        buf.extend_ssh_string(public.as_ref());
        buf.deref_mut().write_u32::<BigEndian>(64)?;
        buf.extend(&*self.0);
        buf.extend_ssh_string(b"radicle");

        Ok(())
    }

    fn write_signature<Bytes: AsRef<[u8]>>(
        &self,
        data: Bytes,
        buf: &mut Zeroizing<Vec<u8>>,
    ) -> Result<(), Self::Error> {
        let name = "ssh-ed25519";
        let signature: [u8; 64] = *self.0.sign(data.as_ref(), None);

        buf.deref_mut()
            .write_u32::<BigEndian>((name.len() + signature.len() + 8) as u32)?;
        buf.extend_ssh_string(name.as_bytes());
        buf.extend_ssh_string(&signature);

        Ok(())
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
    #[error("public key encoding")]
    PublicKeyEncoding,
    #[error(transparent)]
    PublicKeyError(#[from] PublicKeyError),
    #[error("signature encoding")]
    SignatureEncoding,
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
    sig_version: u32,
    publickey: crypto::PublicKey,
    /// Unambigious interpretation domain to prevent cross-protocol attacks.
    namespace: Vec<u8>,
    reserved: Vec<u8>,
    /// Hash used for signature. For example 'sha256'.
    hash_algorithm: Vec<u8>,
    signature: crypto::Signature,
}

impl Encodable for ExtendedSignature {
    type Error = ExtendedSignatureError;

    fn read_ssh(r: &mut encoding::Cursor) -> Result<Self, Self::Error> {
        let sig_version = r.read_u32()?;
        if sig_version > 1 {
            return Err(ExtendedSignatureError::UnsupportedVersion(sig_version));
        }

        Ok(ExtendedSignature {
            sig_version,
            publickey: PublicKey::read_ssh(r)
                .map_err(|_| ExtendedSignatureError::PublicKeyEncoding)?,
            namespace: r.read_string()?.into(),
            reserved: r.read_string()?.into(),
            hash_algorithm: r.read_string()?.into(),
            signature: crypto::Signature::read_ssh(r)
                .map_err(|_| ExtendedSignatureError::PublicKeyEncoding)?,
        })
    }

    fn write_ssh<E: Encoding>(&self, buf: &mut E) {
        buf.extend_u32(self.sig_version);
        let _ = &self.publickey.write_ssh(buf);
        buf.extend_ssh_string(&self.namespace);
        buf.extend_ssh_string(&self.reserved);
        buf.extend_ssh_string(&self.hash_algorithm);
        let _ = &self.signature.write_ssh(buf);
    }
}

impl ExtendedSignature {
    const ARMORED_HEADER: &[u8] = b"-----BEGIN SSH SIGNATURE-----";
    const ARMORED_FOOTER: &[u8] = b"-----END SSH SIGNATURE-----";
    const ARMORED_WIDTH: usize = 70;
    const MAGIC_PREAMBLE: &[u8] = b"SSHSIG";

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

        let sig = ExtendedSignature::read_ssh(&mut reader)?;
        Ok(sig)
    }

    pub fn to_armored(&self) -> Vec<u8> {
        let mut v = encoding::Buffer::default();

        v.extend(Self::MAGIC_PREAMBLE);
        self.write_ssh(&mut v);

        let mut armored = Self::ARMORED_HEADER.to_vec();
        armored.push(b'\n');

        let body: Vec<u8> = base64::encode(v).into();
        for line in body.chunks(Self::ARMORED_WIDTH) {
            armored.extend(line.to_vec());
            armored.push(b'\n');
        }

        armored.extend(Self::ARMORED_FOOTER);

        armored
    }
}

#[cfg(test)]
mod test {
    use std::sync::{Arc, Mutex};

    use quickcheck_macros::quickcheck;
    use zeroize::Zeroizing;

    use super::{ExtendedSignature, SecretKey};
    use crate::crypto;
    use crate::crypto::PublicKey;
    use crate::test::arbitrary::ByteArray;
    use radicle_ssh::agent::client::{AgentClient, ClientStream, Error};
    use radicle_ssh::encoding::Reader;
    use radicle_ssh::key::Private as _;

    #[derive(Clone, Default)]
    struct DummyStream {
        incoming: Arc<Mutex<Zeroizing<Vec<u8>>>>,
    }

    impl ClientStream for DummyStream {
        fn connect_socket<P>(_path: P) -> Result<AgentClient<Self>, Error>
        where
            P: AsRef<std::path::Path> + Send,
        {
            panic!("This function should never be called!")
        }

        fn read_response(&mut self, buf: &mut Zeroizing<Vec<u8>>) -> Result<(), Error> {
            *self.incoming.lock().unwrap() = buf.clone();

            Ok(())
        }
    }

    #[quickcheck]
    fn prop_encode_decode_sk(input: ByteArray<64>) {
        let mut buf = Vec::new().into();
        let sk = crypto::SecretKey::new(input.into_inner());
        SecretKey(sk).write(&mut buf).unwrap();

        let mut cursor = buf.reader(0);
        let output = SecretKey::read(&mut cursor).unwrap().unwrap();

        assert_eq!(sk, output.0);
    }

    #[test]
    fn test_agent_encoding_remove() {
        use std::str::FromStr;

        let pk = PublicKey::from_str("zF4VJZgNEeL1niWmKu1NtT1B4ZyGpjACyhs2VEZvtsws5").unwrap();
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

        let pk = PublicKey::from_str("zF4VJZgNEeL1niWmKu1NtT1B4ZyGpjACyhs2VEZvtsws5").unwrap();
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
        let data: Zeroizing<Vec<u8>> = vec![1, 2, 3, 4, 5, 6, 7, 8, 9].into();

        agent.sign_request(&pk, data).ok();

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
AAAAQJ759x+pFz0z2yM13S/sqeOOSgTE3fhoJG54dotNTk17dQEPKnH4S4N5jjA+pxM1mb
oejZ0WJ0cQtBjWZ7JEBQM=
-----END SSH SIGNATURE-----";

        let signature = ExtendedSignature::from_armored(armored).unwrap();
        assert_eq!(1, signature.sig_version);
        assert_eq!(
            String::from_utf8(armored.to_vec()),
            String::from_utf8(signature.to_armored()),
            "signature should remain unaltered after decoding"
        );
    }
}

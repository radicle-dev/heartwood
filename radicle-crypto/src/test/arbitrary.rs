use quickcheck::Arbitrary;

use crate::{hash, test::signer::MockSigner, KeyPair, PublicKey, SecretKey, Seed};

impl Arbitrary for MockSigner {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        let bytes: ByteArray<32> = Arbitrary::arbitrary(g);
        let seed = Seed::new(bytes.into_inner());
        let sk = KeyPair::from_seed(seed).sk;

        MockSigner::from(SecretKey::from(sk))
    }
}

impl Arbitrary for PublicKey {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        let bytes: ByteArray<32> = Arbitrary::arbitrary(g);
        let seed = Seed::new(bytes.into_inner());
        let keypair = KeyPair::from_seed(seed);

        PublicKey(keypair.pk)
    }
}

impl Arbitrary for hash::Digest {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        let bytes: Vec<u8> = Arbitrary::arbitrary(g);
        hash::Digest::new(&bytes)
    }
}

#[derive(Clone, Debug)]
pub struct ByteArray<const N: usize>([u8; N]);

impl<const N: usize> ByteArray<N> {
    pub fn into_inner(self) -> [u8; N] {
        self.0
    }

    pub fn as_slice(&self) -> &[u8] {
        self.0.as_slice()
    }
}

impl<const N: usize> Arbitrary for ByteArray<N> {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        let mut bytes: [u8; N] = [0; N];
        for byte in &mut bytes {
            *byte = u8::arbitrary(g);
        }
        Self(bytes)
    }
}

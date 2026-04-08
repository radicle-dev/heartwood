use qcheck::Arbitrary;

use crate::{KeyPair, PublicKey, SecretKey, Seed, test::signer::MockSigner};

impl Arbitrary for MockSigner {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
        let bytes: [u8; 32] = Arbitrary::arbitrary(g);
        let seed = Seed::new(bytes);
        let sk = KeyPair::from_seed(seed).sk;

        MockSigner::from(SecretKey::from(sk))
    }
}

impl Arbitrary for PublicKey {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
        let bytes: [u8; 32] = Arbitrary::arbitrary(g);
        let seed = Seed::new(bytes);
        let keypair = KeyPair::from_seed(seed);

        PublicKey(keypair.pk)
    }
}

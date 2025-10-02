use zeroize::Zeroizing;

#[derive(Clone, Debug)]
pub struct Seed(Zeroizing<[u8; Self::BYTES]>);

impl Seed {
    pub const BYTES: usize = 32;

    pub fn new(seed: [u8; Self::BYTES]) -> Self {
        Seed(Zeroizing::from(seed))
    }

    #[cfg(any(test, all(feature = "test", feature = "alloc")))]
    pub fn mock(bytes: usize) -> Self {
        use alloc::vec::Vec;

        let bytes = core::iter::repeat_n(bytes, Self::BYTES / core::mem::size_of::<usize>())
            .flat_map(|bytes| bytes.to_be_bytes())
            .collect::<Vec<_>>();

        Self::new(bytes.try_into().expect("length matches"))
    }
}

impl AsRef<[u8; Self::BYTES]> for Seed {
    fn as_ref(&self) -> &[u8; Self::BYTES] {
        &self.0
    }
}

#[cfg(feature = "qcheck")]
impl qcheck::Arbitrary for Seed {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
        Self::new(qcheck::Arbitrary::arbitrary(g))
    }
}

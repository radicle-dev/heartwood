//! Stores carry [`Access`] to indicate allowed access modes. In particular
//! whether writes to the store are allowed.

pub use seal::Access;

/// [`ReadOnly`] is used for read-only [`Access`].
pub struct ReadOnly;

/// [`WriteAs`] is used for write [`Access`].
pub struct WriteAs<'a, Signer: crypto::Signer> {
    pub(super) signer: &'a Signer,
}

impl<'a, Signer: crypto::Signer> WriteAs<'a, Signer> {
    pub fn new(signer: &'a Signer) -> Self {
        Self { signer }
    }
}

// See <https://predr.ag/blog/definitive-guide-to-sealed-traits-in-rust/#sealing-traits-via-method-signatures>.
#[allow(private_interfaces)]
mod seal {
    enum Seal {}

    /// Marker trait for COB store access modes.
    pub trait Access {
        fn seal(&self, _: Seal);
    }

    impl Access for super::ReadOnly {
        fn seal(&self, _: Seal) {}
    }

    impl<Signer: crypto::Signer> Access for super::WriteAs<'_, Signer> {
        fn seal(&self, _: Seal) {}
    }
}

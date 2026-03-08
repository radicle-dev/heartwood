use std::fmt;
use std::ops::Deref;

use crypto::{
    Signature,
    signature::{Keypair, KeypairRef, Signer, Verifier},
    ssh::ExtendedSignature,
};

use crate::crypto;

use super::NodeId;

/// A `Device` identifies the local node through its [`NodeId`], and carries its
/// signing mechanism.
///
/// The signing mechanism is for node specific cryptography, e.g. signing
/// `rad/sigrefs`, COB commits, node messages, etc.
///
/// Note that a `Device` can create [`Signature`]s and [`ExtendedSignature`]s.
/// It can achieve this as long as `S` implements `Signer<Signature>`.
#[derive(Clone)]
pub struct Device<S> {
    node: NodeId,
    signer: S,
}

impl<S> fmt::Debug for Device<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Device")
            .field("node", &self.node.to_human())
            .finish()
    }
}

impl<S: crypto::Signer + Default> Default for Device<S> {
    fn default() -> Self {
        Self::from(S::default())
    }
}

impl<S> Device<S> {
    /// Construct a new `Device`.
    pub fn new(node: NodeId, signer: S) -> Self {
        Self { node, signer }
    }

    /// Return the [`NodeId`] of the `Device.`
    pub fn node_id(&self) -> &NodeId {
        &self.node
    }

    /// Return the [`crypto::PublicKey`] of the `Device.`
    pub fn public_key(&self) -> &crypto::PublicKey {
        &self.node
    }

    /// Convert the `Device` into its signer.
    ///
    /// This consumes the `Device`.
    pub fn into_inner(self) -> S {
        self.signer
    }
}

#[cfg(any(test, feature = "test"))]
impl Device<crypto::test::signer::MockSigner> {
    /// Construct a `Device` using a default `MockSigner` as the device signer.
    pub fn mock() -> Self {
        Device::from(crypto::test::signer::MockSigner::default())
    }

    /// Construct a `Device`, constructing an RNG'd `MockSigner` for the signer.
    pub fn mock_rng(rng: &mut fastrand::Rng) -> Self {
        Device::from(crypto::test::signer::MockSigner::new(rng))
    }

    /// Construct a `Device`, constructing a seeded `MockSigner` for the signer.
    pub fn mock_from_seed(seed: [u8; 32]) -> Self {
        Device::from(crypto::test::signer::MockSigner::from_seed(seed))
    }
}

impl<S: BoxableSigner + 'static> Device<S> {
    /// Construct a [`BoxedDevice`] from a given `Device`.
    pub fn boxed(self) -> BoxedDevice {
        BoxedDevice(Device {
            node: self.node,
            signer: BoxedSigner(Box::new(self.signer)),
        })
    }
}

impl<S> AsRef<NodeId> for Device<S> {
    fn as_ref(&self) -> &NodeId {
        &self.node
    }
}

impl<S> KeypairRef for Device<S> {
    type VerifyingKey = NodeId;
}

impl<S> Verifier<Signature> for Device<S> {
    fn verify(&self, msg: &[u8], signature: &Signature) -> Result<(), crypto::signature::Error> {
        self.node
            .verify(msg, signature)
            .map_err(crypto::signature::Error::from_source)
    }
}

impl<S: crypto::Signer> From<S> for Device<S> {
    fn from(signer: S) -> Self {
        Self {
            node: *signer.public_key(),
            signer,
        }
    }
}

impl<S: crypto::Signer + Clone> From<&S> for Device<S> {
    fn from(signer: &S) -> Self {
        Self::from(signer.clone())
    }
}

impl<S: Signer<Signature>> Signer<Signature> for Device<S> {
    fn try_sign(&self, msg: &[u8]) -> Result<Signature, crypto::signature::Error> {
        self.signer.try_sign(msg)
    }
}

impl<S: Signer<Signature>> Signer<ExtendedSignature> for Device<S> {
    fn try_sign(&self, msg: &[u8]) -> Result<ExtendedSignature, crypto::signature::Error> {
        Ok(ExtendedSignature {
            key: *self.public_key(),
            sig: self.signer.try_sign(msg)?,
        })
    }
}

pub trait BoxableSigner: Signer<Signature> + Keypair<VerifyingKey = crypto::PublicKey> {}

impl<S: Signer<Signature> + Keypair<VerifyingKey = crypto::PublicKey>> BoxableSigner for S {}

/// A `Signer<Signature>` that is packed in a [`Box`] for dynamic dispatch.
pub struct BoxedSigner(Box<dyn BoxableSigner + 'static>);

impl Signer<Signature> for BoxedSigner {
    fn try_sign(&self, msg: &[u8]) -> Result<Signature, crypto::signature::Error> {
        self.0.try_sign(msg)
    }
}

impl Keypair for BoxedSigner {
    type VerifyingKey = crypto::PublicKey;

    fn verifying_key(&self) -> Self::VerifyingKey {
        self.0.verifying_key()
    }
}

/// A `Device` where the signer is a dynamic `Signer<Signature>`, in the form of
/// a [`BoxedSigner`].
///
/// This can be constructed via [`Device::boxed`].
pub struct BoxedDevice(Device<BoxedSigner>);

impl AsRef<Device<BoxedSigner>> for BoxedDevice {
    fn as_ref(&self) -> &Device<BoxedSigner> {
        &self.0
    }
}

impl Deref for BoxedDevice {
    type Target = Device<BoxedSigner>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Signer<Signature> for BoxedDevice {
    fn try_sign(&self, msg: &[u8]) -> Result<Signature, crypto::signature::Error> {
        self.0.signer.try_sign(msg)
    }
}

impl Signer<ExtendedSignature> for BoxedDevice {
    fn try_sign(&self, msg: &[u8]) -> Result<ExtendedSignature, crypto::signature::Error> {
        Ok(ExtendedSignature {
            key: *self.0.public_key(),
            sig: self.0.signer.try_sign(msg)?,
        })
    }
}

impl AsRef<crypto::PublicKey> for BoxedDevice {
    fn as_ref(&self) -> &crypto::PublicKey {
        &self.0.node
    }
}

impl KeypairRef for BoxedDevice {
    type VerifyingKey = crypto::PublicKey;
}

impl Verifier<Signature> for BoxedDevice {
    fn verify(&self, msg: &[u8], signature: &Signature) -> Result<(), crypto::signature::Error> {
        self.0
            .node
            .verify(msg, signature)
            .map_err(crypto::signature::Error::from_source)
    }
}

use bstr::{BString, ByteSlice};
use either::Either;
use radicle::crypto::PublicKey;
use radicle::git::{self, Component, Namespaced, Oid, Qualified};
use radicle::node::NodeId;
use thiserror::Error;

pub use radicle::git::refs::storage::Special;

use crate::git::refs::{Policy, Update};

pub(crate) use radicle::git::refs::storage::IDENTITY_BRANCH as REFS_RAD_ID;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    #[error("non-namespaced ref name '{0}' is not 'refs/rad/id'")]
    NotCanonicalRadID(Qualified<'static>),
    #[error("invalid remote peer id")]
    PublicKey(#[from] radicle::crypto::PublicKeyError),
    #[error(transparent)]
    Ref(#[from] radicle::git::RefError),
    #[error(transparent)]
    Utf8(#[from] bstr::Utf8Error),
}

pub(crate) fn unpack_ref<'a>(
    r: gix_protocol::handshake::Ref,
) -> Result<(ReceivedRefname<'a>, Oid), Error> {
    use crate::git::oid;
    use gix_protocol::handshake::Ref;

    match r {
        Ref::Peeled {
            full_ref_name,
            object,
            ..
        }
        | Ref::Direct {
            full_ref_name,
            object,
        }
        | Ref::Symbolic {
            full_ref_name,
            object,
            ..
        } => ReceivedRefname::try_from(full_ref_name).map(|name| (name, oid::to_oid(object))),
        Ref::Unborn { full_ref_name, .. } => {
            unreachable!("BUG: unborn ref {}", full_ref_name)
        }
    }
}

/// A reference name received during an exchange with another peer. The
/// expected references are either namespaced references in the form
/// of [`RemoteRef`] or the canonical `rad/id` reference.
#[derive(Debug)]
pub(crate) enum ReceivedRefname<'a> {
    /// A reference name under a `remote` namespace.
    ///
    /// # Examples
    ///
    ///   * `refs/namespaces/<remote>/refs/rad/id`
    ///   * `refs/namespaces/<remote>/refs/rad/sigrefs`
    ///   * `refs/namespaces/<remote>/refs/heads/main`
    ///   * `refs/namespaces/<remote>/refs/cobs/issue.rad.xyz`
    Namespaced {
        /// The namespace of the remote.
        remote: NodeId,
        /// The reference is expected to either be a [`Special`] reference
        /// or a generic reference name.
        suffix: Either<Special, Qualified<'a>>,
    },
    /// The canonical `refs/rad/id` reference
    RadId,
}

impl<'a> ReceivedRefname<'a> {
    pub fn remote(remote: NodeId, suffix: Qualified<'a>) -> Self {
        Self::Namespaced {
            remote,
            suffix: Either::Right(suffix),
        }
    }

    pub fn to_qualified<'b>(&self) -> Qualified<'b> {
        match &self {
            Self::Namespaced { remote, suffix } => match suffix {
                Either::Left(s) => Qualified::from(*s)
                    .with_namespace(Component::from(remote))
                    .into(),
                Either::Right(name) => {
                    Qualified::from(name.with_namespace(Component::from(remote))).to_owned()
                }
            },
            Self::RadId => REFS_RAD_ID.clone(),
        }
    }

    pub fn to_namespaced<'b>(&self) -> Option<Namespaced<'b>> {
        match self {
            Self::Namespaced { remote, suffix } => Some(match suffix {
                Either::Left(special) => special.namespaced(remote),
                Either::Right(refname) => {
                    refname.with_namespace(Component::from(remote)).to_owned()
                }
            }),
            Self::RadId => None,
        }
    }
}

impl TryFrom<BString> for ReceivedRefname<'_> {
    type Error = Error;

    fn try_from(value: BString) -> Result<Self, Self::Error> {
        match git::parse_ref::<NodeId>(value.to_str()?)? {
            (None, name) => (name == *REFS_RAD_ID)
                .then_some(ReceivedRefname::RadId)
                .ok_or_else(|| Error::NotCanonicalRadID(name.to_owned())),
            (Some(remote), name) => Ok(ReceivedRefname::Namespaced {
                remote,
                suffix: match Special::from_qualified(&name) {
                    None => Either::Right(name.to_owned()),
                    Some(special) => Either::Left(special),
                },
            }),
        }
    }
}

/// A reference name and the associated tip received during an
/// exchange with another peer.
#[derive(Debug)]
pub(crate) struct ReceivedRef {
    pub tip: Oid,
    pub name: ReceivedRefname<'static>,
}

impl ReceivedRef {
    pub fn new(tip: Oid, name: ReceivedRefname<'static>) -> Self {
        Self { tip, name }
    }

    pub fn to_qualified(&self) -> Qualified<'static> {
        self.name.to_qualified()
    }

    pub fn as_special_ref_update<F>(&self, is_delegate: F) -> Option<(NodeId, Update<'static>)>
    where
        F: Fn(&NodeId) -> bool,
    {
        match &self.name {
            ReceivedRefname::RadId => None,
            ReceivedRefname::Namespaced { remote, suffix } => {
                special_update(remote, suffix, self.tip, is_delegate).map(|up| (*remote, up))
            }
        }
    }
}

pub(crate) fn special_update<F>(
    remote: &NodeId,
    suffix: &Either<Special, Qualified>,
    tip: Oid,
    is_delegate: F,
) -> Option<Update<'static>>
where
    F: Fn(&NodeId) -> bool,
{
    suffix.as_ref().left().map(|special| Update::Direct {
        name: special.namespaced(remote).to_owned(),
        target: tip,
        // N.b. reject any updates if the remote is not a delegate,
        // since this is not fatal.
        no_ff: if is_delegate(remote) {
            Policy::Abort
        } else {
            Policy::Reject
        },
    })
}

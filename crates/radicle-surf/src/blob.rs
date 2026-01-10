//! Represents git object type 'blob', i.e. actual file contents.
//! See git [doc](https://git-scm.com/book/en/v2/Git-Internals-Git-Objects) for more details.

use std::ops::Deref;

use radicle_git_ext::Oid;

#[cfg(feature = "serde")]
use serde::{
    ser::{SerializeStruct as _, Serializer},
    Serialize,
};

use crate::Commit;

/// Represents a git blob object.
///
/// The type parameter `T` can be fulfilled by [`BlobRef`] or a
/// [`Vec`] of bytes.
pub struct Blob<T> {
    id: Oid,
    is_binary: bool,
    commit: Commit,
    content: T,
}

impl<T> Blob<T> {
    pub fn object_id(&self) -> Oid {
        self.id
    }

    pub fn is_binary(&self) -> bool {
        self.is_binary
    }

    /// Returns the commit that created this blob.
    pub fn commit(&self) -> &Commit {
        &self.commit
    }

    pub fn content(&self) -> &[u8]
    where
        T: AsRef<[u8]>,
    {
        self.content.as_ref()
    }

    pub fn size(&self) -> usize
    where
        T: AsRef<[u8]>,
    {
        self.content.as_ref().len()
    }
}

impl<'a> Blob<BlobRef<'a>> {
    /// Returns the [`Blob`] wrapping around an underlying [`git2::Blob`].
    pub(crate) fn new(id: Oid, git2_blob: git2::Blob<'a>, commit: Commit) -> Self {
        let is_binary = git2_blob.is_binary();
        let content = BlobRef { inner: git2_blob };
        Self {
            id,
            is_binary,
            content,
            commit,
        }
    }

    /// Converts into a `Blob` with owned content bytes.
    pub fn to_owned(&self) -> Blob<Vec<u8>> {
        Blob {
            id: self.id,
            content: self.content.to_vec(),
            commit: self.commit.clone(),
            is_binary: self.is_binary,
        }
    }
}

/// Represents a blob with borrowed content bytes.
pub struct BlobRef<'a> {
    pub(crate) inner: git2::Blob<'a>,
}

impl BlobRef<'_> {
    pub fn id(&self) -> Oid {
        self.inner.id().into()
    }
}

impl AsRef<[u8]> for BlobRef<'_> {
    fn as_ref(&self) -> &[u8] {
        self.inner.content()
    }
}

impl Deref for BlobRef<'_> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.inner.content()
    }
}

#[cfg(feature = "serde")]
impl<T> Serialize for Blob<T>
where
    T: AsRef<[u8]>,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use base64::Engine as _;

        const FIELDS: usize = 4;
        let mut state = serializer.serialize_struct("Blob", FIELDS)?;
        state.serialize_field("id", &self.id)?;
        state.serialize_field("binary", &self.is_binary())?;

        let bytes = self.content.as_ref();
        match std::str::from_utf8(bytes) {
            Ok(s) => state.serialize_field("content", s)?,
            Err(_) => {
                let encoded = base64::prelude::BASE64_STANDARD.encode(bytes);
                state.serialize_field("content", &encoded)?
            }
        };
        state.serialize_field("lastCommit", &self.commit)?;
        state.end()
    }
}

#[cfg(feature = "serde")]
impl Serialize for BlobRef<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use base64::Engine as _;

        const FIELDS: usize = 3;
        let mut state = serializer.serialize_struct("BlobRef", FIELDS)?;
        state.serialize_field("id", &self.id())?;
        state.serialize_field("binary", &self.inner.is_binary())?;

        let bytes = self.as_ref();
        match std::str::from_utf8(bytes) {
            Ok(s) => state.serialize_field("content", s)?,
            Err(_) => {
                let encoded = base64::prelude::BASE64_STANDARD.encode(bytes);
                state.serialize_field("content", &encoded)?
            }
        };
        state.end()
    }
}

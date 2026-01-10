use std::{convert::TryFrom, str};

use git_ext::{
    ref_format::{component, lit, Qualified, RefStr, RefString},
    Oid,
};

use crate::{refs::refstr_join, Author};

/// The metadata of a [`Git tag`][git-tag].
///
/// [git-tag]: https://git-scm.com/book/en/v2/Git-Basics-Tagging
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tag {
    /// A light-weight git tag.
    Light {
        /// The Object ID for the `Tag`, i.e the SHA1 digest.
        id: Oid,
        /// The reference name for this `Tag`.
        name: RefString,
    },
    /// An annotated git tag.
    Annotated {
        /// The Object ID for the `Tag`, i.e the SHA1 digest.
        id: Oid,
        /// The Object ID for the object that is tagged.
        target: Oid,
        /// The reference name for this `Tag`.
        name: RefString,
        /// The named author of this `Tag`, if the `Tag` was annotated.
        tagger: Option<Author>,
        /// The message with this `Tag`, if the `Tag` was annotated.
        message: Option<String>,
    },
}

impl Tag {
    /// Get the `Oid` of the tag, regardless of its type.
    pub fn id(&self) -> Oid {
        match self {
            Self::Light { id, .. } => *id,
            Self::Annotated { id, .. } => *id,
        }
    }

    /// Return the short `Tag` refname,
    /// e.g. `release/v1`.
    pub fn short_name(&self) -> &RefString {
        match &self {
            Tag::Light { name, .. } => name,
            Tag::Annotated { name, .. } => name,
        }
    }

    /// Return the fully qualified `Tag` refname,
    /// e.g. `refs/tags/release/v1`.
    pub fn refname<'a>(&'a self) -> Qualified<'a> {
        lit::refs_tags(self.short_name()).into()
    }
}

pub mod error {
    use std::str;

    use radicle_git_ext::ref_format::{self, RefString};
    use thiserror::Error;

    #[derive(Debug, Error)]
    pub enum FromTag {
        #[error(transparent)]
        RefFormat(#[from] ref_format::Error),
        #[error(transparent)]
        Utf8(#[from] str::Utf8Error),
    }

    #[derive(Debug, Error)]
    pub enum FromReference {
        #[error(transparent)]
        FromTag(#[from] FromTag),
        #[error(transparent)]
        Git(#[from] git2::Error),
        #[error("the refname '{0}' did not begin with 'refs/tags'")]
        NotQualified(String),
        #[error("the refname '{0}' did not begin with 'refs/tags'")]
        NotTag(RefString),
        #[error(transparent)]
        RefFormat(#[from] ref_format::Error),
        #[error(transparent)]
        Utf8(#[from] str::Utf8Error),
    }
}

impl TryFrom<&git2::Tag<'_>> for Tag {
    type Error = error::FromTag;

    fn try_from(tag: &git2::Tag) -> Result<Self, Self::Error> {
        let id = tag.id().into();
        let target = tag.target_id().into();
        let name = {
            let name = str::from_utf8(tag.name_bytes())?;
            RefStr::try_from_str(name)?.to_ref_string()
        };
        let tagger = tag.tagger().map(Author::try_from).transpose()?;
        let message = tag
            .message_bytes()
            .map(str::from_utf8)
            .transpose()?
            .map(|message| message.into());

        Ok(Tag::Annotated {
            id,
            target,
            name,
            tagger,
            message,
        })
    }
}

impl TryFrom<&git2::Reference<'_>> for Tag {
    type Error = error::FromReference;

    fn try_from(reference: &git2::Reference) -> Result<Self, Self::Error> {
        let name = reference_name(reference)?;
        match reference.peel_to_tag() {
            Ok(tag) => Tag::try_from(&tag).map_err(error::FromReference::from),
            // If we get an error peeling to a tag _BUT_ we also have confirmed the
            // reference is a tag, that means we have a lightweight tag,
            // i.e. a commit SHA and name.
            Err(err)
                if err.class() == git2::ErrorClass::Object
                    && err.code() == git2::ErrorCode::InvalidSpec =>
            {
                let commit = reference.peel_to_commit()?;
                Ok(Tag::Light {
                    id: commit.id().into(),
                    name,
                })
            }
            Err(err) => Err(err.into()),
        }
    }
}

pub(crate) fn reference_name(
    reference: &git2::Reference,
) -> Result<RefString, error::FromReference> {
    let name = str::from_utf8(reference.name_bytes())?;
    let name = RefStr::try_from_str(name)?
        .qualified()
        .ok_or_else(|| error::FromReference::NotQualified(name.to_string()))?;

    let (_refs, tags, c, cs) = name.non_empty_components();

    if tags == component::TAGS {
        Ok(refstr_join(c, cs))
    } else {
        Err(error::FromReference::NotTag(name.into()))
    }
}

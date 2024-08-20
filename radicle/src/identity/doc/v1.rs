//! We keep track of the previous versions of the identity documents for testing
//! and migrating.
//!
//! This [`Doc`] is the Version 1 of the identity document.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::git;
use crate::git::canonical::rules;
use crate::git::canonical::rules::{RawRules, Rule, Rules, ValidationError};
use crate::prelude::{Did, Project};

use super::{
    version::VersionOne, CanonicalRefs, Delegates, DocError, Payload, PayloadError, PayloadId,
    Threshold, Visibility,
};

/// `RawDoc` is similar to the [`Doc`] type, however, it can be edited and may
/// not be valid.
///
/// It is expected that any changes to a [`Doc`] are made via [`RawDoc`], and
/// then verified by using [`RawDoc::verified`].
///
/// Note that `RawDoc` only implements [`Deserialize`]. This prevents us from
/// serializing an unverified document, while also making sure that any document
/// that is deserialized is verified.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RawDoc {
    #[serde(default)]
    pub version: VersionOne,
    /// The payload section.
    pub payload: BTreeMap<PayloadId, Payload>,
    /// The delegates section.
    pub delegates: Vec<Did>,
    /// The signature threshold.
    pub threshold: usize,
    /// Repository visibility.
    #[serde(default)]
    pub visibility: Visibility,
}

impl RawDoc {
    /// Verify the `RawDoc`'s values, converting it into a valid [`Doc`].
    ///
    /// The verifications are as follows:
    ///
    ///  - [`RawDoc::delegates`]: any duplicates are removed, and for the
    ///    remaining set ensure that it is non-empty and does not exceed a
    ///    length of [`MAX_DELEGATES`].
    ///  - [`RawDoc::threshold`]: ensure that it is in the range `[1, delegates.len()]`.
    pub fn verified(self) -> Result<Doc, DocError> {
        let RawDoc {
            payload,
            delegates,
            threshold,
            visibility,
            version,
        } = self;
        let delegates = Delegates::new(delegates)?;
        let threshold = Threshold::new(threshold, &delegates)?;
        Ok(Doc {
            payload,
            delegates,
            threshold,
            visibility,
            version,
        })
    }
}

/// `Doc` is a valid identity document.
///
/// To ensure that only valid documents are used, this type is restricted to be
/// read-only. For mutating the document use [`Doc::edit`].
///
/// A valid `Doc` can be constructed in four ways:
///
///   1. [`Doc::initial`]: a safe way to construct the initial document for an identity.
///   2. [`RawDoc::verified`]: validates a [`RawDoc`]'s fields and converts it
///      into a `Doc`
///   3. [`Doc::from_blob`]: construct a `Doc` from a Git blob by deserializing
///      its contents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Doc {
    #[serde(skip_serializing)]
    version: VersionOne,
    payload: BTreeMap<PayloadId, Payload>,
    delegates: Delegates,
    threshold: Threshold,
    #[serde(default, skip_serializing_if = "Visibility::is_public")]
    visibility: Visibility,
}

impl TryFrom<RawDoc> for Doc {
    type Error = DocError;

    fn try_from(doc: RawDoc) -> Result<Self, Self::Error> {
        doc.verified()
    }
}

#[derive(Debug, Error)]
pub enum MigrationError {
    #[error(transparent)]
    Pattern(#[from] rules::PatternError),
    #[error(transparent)]
    Payload(#[from] PayloadError),
    #[error(transparent)]
    ValidationError(#[from] ValidationError),
}

impl Doc {
    /// Automatically migrate the `v1` `Doc` to the latest [`super::Doc`] version.
    ///
    /// This can be used to get the latest version in a verified state for
    /// proposing as identity document change.
    ///
    /// This migrations handles:
    ///
    ///  * v1 -> v2:
    ///    - Add a canonical reference rule for the default branch.
    ///    - Add the [`Version`] field
    ///    - Removes the `threshold`, but uses it in the above rule.
    pub fn migrate(self) -> Result<super::Doc, MigrationError> {
        let project = self.project()?;
        let Doc {
            version: _version,
            payload,
            delegates,
            threshold,
            visibility,
        } = self;

        let rules = match project {
            Some(project) => {
                let refname = git::refspec::QualifiedPattern::from(git::refs::branch(
                    project.default_branch(),
                ))
                .to_owned();
                let rules = [(
                    rules::Pattern::try_from(refname)?,
                    Rule::new(rules::Allowed::Delegates, threshold.into()),
                )]
                .into_iter()
                .collect::<RawRules>();
                // N.b. we always return the `delegates`, since we know we're
                // using the `Identity` marker.
                Rules::from_raw(rules, &mut |_| Ok(Delegates::new(delegates.clone())?))?
            }
            None => Rules::default(),
        };

        Ok(super::Doc {
            version: super::VersionTwo,
            payload,
            delegates,
            canonical_refs: CanonicalRefs::new(rules),
            visibility,
        })
    }

    /// Get the project payload, if it exists and is valid, out of this document.
    fn project(&self) -> Result<Option<Project>, PayloadError> {
        match self.payload.get(&PayloadId::project()) {
            Some(value) => serde_json::from_value((**value).clone())
                .map_err(PayloadError::from)
                .map(Some),
            None => Ok(None),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod test {
    use crate::identity::VersionedRawDoc;

    use super::*;

    use nonempty::NonEmpty;
    use serde_json::json;

    #[test]
    fn test_parse_version() {
        // Harcoded version 1 of the identity document. We expect that parsing
        // this will include the version.
        let v1 = json!(
            {
                "payload": {
                    "xyz.radicle.project": {
                        "defaultBranch": "master",
                        "description": "Radicle Heartwood Protocol & Stack",
                        "name": "heartwood"
                    }
                },
                "delegates": [
                    "did:key:z6MksFqXN3Yhqk8pTJdUGLwATkRfQvwZXPqR2qMEhbS9wzpT",
                    "did:key:z6MktaNvN1KVFMkSRAiN4qK5yvX1zuEEaseeX5sffhzPZRZW",
                    "did:key:z6MkireRatUThvd3qzfKht1S44wpm4FEWSSa4PRMTSQZ3voM"
                ],
                "threshold": 1
            }
        );

        // Deserializing the v1 document to the current version of the mutable
        // document should not fail.
        let doc = serde_json::from_str::<RawDoc>(&v1.to_string()).unwrap();
        let payload = [(
            PayloadId::project(),
            Payload {
                value: json!({
                    "name": "heartwood",
                    "description": "Radicle Heartwood Protocol & Stack",
                    "defaultBranch": "master",
                }),
            },
        )]
        .into_iter()
        .collect::<BTreeMap<_, _>>();
        let delegates = vec![
            "did:key:z6MksFqXN3Yhqk8pTJdUGLwATkRfQvwZXPqR2qMEhbS9wzpT"
                .parse::<Did>()
                .unwrap(),
            "did:key:z6MktaNvN1KVFMkSRAiN4qK5yvX1zuEEaseeX5sffhzPZRZW"
                .parse::<Did>()
                .unwrap(),
            "did:key:z6MkireRatUThvd3qzfKht1S44wpm4FEWSSa4PRMTSQZ3voM"
                .parse::<Did>()
                .unwrap(),
        ];

        let expected_doc = RawDoc {
            version: VersionOne,
            payload: payload.clone(),
            delegates: delegates.clone(),
            threshold: 1,
            visibility: Visibility::Public,
        };

        // And this is the expected outcome of the deserialization
        assert_eq!(doc, expected_doc);

        // Deserializing into the verified document should also succeed.
        let doc = serde_json::from_str::<RawDoc>(&v1.to_string()).unwrap();
        let verified = doc.verified().unwrap();
        let delegates = Delegates(NonEmpty::from_vec(delegates).unwrap());
        assert_eq!(
            verified,
            Doc {
                version: VersionOne,
                threshold: Threshold::new(1, &delegates).unwrap(),
                payload: payload.clone(),
                delegates,
                visibility: Visibility::Public,
            }
        );

        let versioned = serde_json::from_str::<VersionedRawDoc>(&v1.to_string()).unwrap();
        assert_eq!(versioned, VersionedRawDoc::V1(expected_doc));
    }
}

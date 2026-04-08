//! Representation of a [`patch::Review`] for deserializing.
//!
//! The [`Review`] type contains fields that are:
//!
//!   - Shared within previous versions, e.g. V1 fields that are shared with V2
//!   - Introduced at a later version, but can use a [`Default`] instance
//!   - Can be migrated from one version to another
//!
//! [`patch::Review`]: crate::cob::patch::Review

use nonempty::NonEmpty;
use serde::Deserialize;
use serde_untagged::UntaggedEnumVisitor;

use crate::cob::patch;
use crate::cob::patch::{
    Author, CodeLocation, Comment, Edit, Label, Reactions, ReviewId, Thread, Timestamp, Verdict,
};

/// The encoding for a `patch::Review` that can be deserialized and migrated.
///
/// To maintain backwards-compatibility, [`Review`] must implement:
/// ```rust, ignore
/// From<Review> for patch::Review
/// ```
#[derive(Deserialize)]
pub(in crate::cob::patch) struct Review {
    // V1 fields
    id: ReviewId,
    author: Author,
    verdict: Option<Verdict>,
    comments: Thread<Comment<CodeLocation>>,
    labels: Vec<Label>,
    timestamp: Timestamp,

    // V2 fields
    #[serde(default)]
    reactions: Reactions,

    // V1 -> V2 conversion
    #[serde(default)]
    summary: Summary,
}

/// The [`Summary`] type represents the different versions of the `summary`
/// field of a [`Review`].
///
/// The `V1` variant holds an `Option<String>`, which can be converted into an
/// `Edit` given the `ActorId` and `Timestamp` – supplying an empty `Vec` of
/// `Embed`s.
///
/// The `V2` variant holds the current type `NonEmpty<Edit>`.
///
/// Using [`Summary::into_edits`], we can get the latest representation of a
/// [`patch::Review::summary`].
///
/// [`patch::Review::summary`]: crate::cob::patch::Review::summary
#[derive(Debug, PartialEq, Eq)]
pub(in crate::cob::patch) enum Summary {
    V2(NonEmpty<Edit>),
    V1(Option<String>),
}

impl<'de> Deserialize<'de> for Summary {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        UntaggedEnumVisitor::new()
            .unit(|| Ok(Self::V1(None)))
            .string(|body| Ok(Self::V1(Some(body.to_owned()))))
            .seq(|edits| edits.deserialize().map(Self::V2))
            .deserialize(deserializer)
    }
}

impl Default for Summary {
    fn default() -> Self {
        Self::V1(None)
    }
}

impl Summary {
    fn into_edits(self, author: &Author, timestamp: &Timestamp) -> NonEmpty<Edit> {
        match self {
            Summary::V1(summary) => NonEmpty::new(Edit::new(
                *author.public_key(),
                summary.unwrap_or_default(),
                *timestamp,
                vec![],
            )),
            Summary::V2(edits) => edits,
        }
    }
}

impl From<Review> for patch::Review {
    fn from(review: Review) -> Self {
        let Review {
            id,
            author,
            verdict,
            comments,
            labels,
            timestamp,
            reactions,
            summary,
        } = review;
        let summary = summary.into_edits(&author, &timestamp);
        Self {
            id,
            author,
            verdict,
            summary,
            comments,
            labels,
            reactions,
            timestamp,
        }
    }
}

#[allow(clippy::unwrap_used)]
#[cfg(test)]
mod test {
    use nonempty::nonempty;
    use serde_json::json;

    use crate::{
        cob::{Timestamp, thread::Edit},
        patch,
    };

    use super::{Review, Summary};

    #[test]
    fn test_review_summary() {
        let summary_null = json!(null);
        let summary_string = json!("lgtm");
        let summary_edits = json!([{
            "author": "z6MkwPUeUS2fJMfc2HZN1RQTQcTTuhw4HhPySB8JeUg2mVvx",
            "timestamp": 1710947885000_i64,
            "body": "lgtm",
            "embeds": [],
        }]);
        assert_eq!(
            serde_json::from_value::<Summary>(summary_null).unwrap(),
            Summary::V1(None)
        );
        assert_eq!(
            serde_json::from_value::<Summary>(summary_string).unwrap(),
            Summary::V1(Some("lgtm".to_string()))
        );
        assert_eq!(
            serde_json::from_value::<Summary>(summary_edits).unwrap(),
            Summary::V2(nonempty![Edit::new(
                "z6MkwPUeUS2fJMfc2HZN1RQTQcTTuhw4HhPySB8JeUg2mVvx"
                    .parse()
                    .unwrap(),
                "lgtm".to_string(),
                Timestamp::from_secs(1710947885),
                vec![]
            )])
        );
    }

    #[test]
    fn test_review_deserialize_summary_migration_null_summary() {
        let review = json!({
            "id": "89d45fb371eb2622ba88188d474347cc526d80bb",
            "author": { "id": "did:key:z6MkwPUeUS2fJMfc2HZN1RQTQcTTuhw4HhPySB8JeUg2mVvx" },
            "verdict": "accept",
            "summary": null,
            "comments": {
                "comments": {},
                "timeline": []
            },
            "labels": [],
            "timestamp": 1710947885000_i64
        });
        let v1 = serde_json::from_value::<Review>(review.clone()).unwrap();
        assert_eq!(
            serde_json::from_value::<patch::Review>(review).unwrap(),
            v1.into()
        );
    }

    #[test]
    fn test_review_deserialize_summary_migration_without_summary() {
        let review = json!({
            "id": "89d45fb371eb2622ba88188d474347cc526d80bb",
            "author": { "id": "did:key:z6MkwPUeUS2fJMfc2HZN1RQTQcTTuhw4HhPySB8JeUg2mVvx" },
            "verdict": "accept",
            "comments": {
                "comments": {},
                "timeline": []
            },
            "labels": [],
            "timestamp": 1710947885000_i64
        });
        let v1 = serde_json::from_value::<Review>(review.clone()).unwrap();
        assert_eq!(
            serde_json::from_value::<patch::Review>(review).unwrap(),
            v1.into()
        );
    }

    #[test]
    fn test_review_deserialize_summary_migration_with_summary() {
        let review = json!({
            "id": "89d45fb371eb2622ba88188d474347cc526d80bb",
            "author": { "id": "did:key:z6MkwPUeUS2fJMfc2HZN1RQTQcTTuhw4HhPySB8JeUg2mVvx" },
            "verdict": "accept",
            "summary": "lgtm",
            "comments": {
                "comments": {},
                "timeline": []
            },
            "labels": [],
            "timestamp": 1710947885000_i64
        });
        let v1 = serde_json::from_value::<Review>(review.clone()).unwrap();
        assert_eq!(
            serde_json::from_value::<patch::Review>(review).unwrap(),
            v1.into()
        );
    }

    #[test]
    fn test_review_deserialize_summary_v2() {
        let review = json!({
            "id": "89d45fb371eb2622ba88188d474347cc526d80bb",
            "author": { "id": "did:key:z6MkwPUeUS2fJMfc2HZN1RQTQcTTuhw4HhPySB8JeUg2mVvx" },
            "verdict": "accept",
            "summary": [{
                "author": "z6MkwPUeUS2fJMfc2HZN1RQTQcTTuhw4HhPySB8JeUg2mVvx",
                "timestamp": 1710947885000_i64,
                "body": "lgtm",
                "embeds": [],
            }],
            "comments": {
                "comments": {},
                "timeline": []
            },
            "labels": [],
            "timestamp": 1710947885000_i64
        });
        let v2 = serde_json::from_value::<Review>(review.clone()).unwrap();
        assert_eq!(
            serde_json::from_value::<patch::Review>(review).unwrap(),
            v2.into()
        );
    }
}

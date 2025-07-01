//! Keep track of the patch [`Action`] versions, to ensure compatibility where
//! possible.
//!
//! [`Action`]: super::Action

use serde::{Deserialize, Serialize};

use crate::cob::{thread::Edit, ActorId, Embed, Label, Timestamp, Uri};

use super::{lookup, Error, Patch, ReviewId, Verdict};

/// A review edit that keeps track of the different versions of actions.
///
/// [`ReviewEdit::new`] will create the latest version of the action.
///
/// [`ReviewEdit::run`] will apply the action to the given [`Patch`].
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ReviewEdit {
    /// The initial version of editing a review.
    ///
    /// This allowed editing the `summary`, `verdict`, and `labels` of a
    /// [`Patch`], where the `summary` value was optional.
    #[serde(rename = "review.edit")]
    V1(ReviewEditV1),
    /// The latest version of editing a review.
    ///
    /// This allows editing the `summary`, `verdict`, `labels` of [`Patch`], and
    /// introduces `embeds` to the review [`summary`].
    ///
    /// The `summary` of a [`Review`] is now an edit-history.
    #[serde(rename = "review.edit.v2")]
    V2(ReviewEditV2),
}

impl ReviewEdit {
    /// Create the latest version of [`ReviewEdit`].
    pub fn new(
        review: ReviewId,
        summary: String,
        verdict: Option<Verdict>,
        labels: Vec<Label>,
        embeds: Vec<Embed<Uri>>,
    ) -> Self {
        Self::V2(ReviewEditV2 {
            review,
            summary,
            verdict,
            labels,
            embeds,
        })
    }

    /// Get the [`ReviewId`] that this edit is applying to.
    pub fn review_id(&self) -> &ReviewId {
        match self {
            ReviewEdit::V1(ReviewEditV1 { review, .. }) => review,
            ReviewEdit::V2(ReviewEditV2 { review, .. }) => review,
        }
    }

    /// Apply the action to the given [`Patch`].
    pub fn run(
        self,
        author: ActorId,
        timestamp: Timestamp,
        patch: &mut Patch,
    ) -> Result<(), Error> {
        match self {
            ReviewEdit::V1(ReviewEditV1 {
                review,
                summary,
                verdict,
                labels,
            }) => {
                if summary.is_none() && verdict.is_none() {
                    return Err(Error::EmptyReview);
                }
                let Some(review) = lookup::review_mut(patch, &review)? else {
                    return Ok(());
                };

                if let Some(body) = summary {
                    review
                        .summary
                        .push(Edit::new(author, body, timestamp, vec![]));
                }
                review.verdict = verdict;
                review.labels = labels;
                Ok(())
            }
            ReviewEdit::V2(ReviewEditV2 {
                review,
                summary,
                verdict,
                labels,
                embeds,
            }) => {
                if summary.is_empty() && verdict.is_none() {
                    return Err(Error::EmptyReview);
                }
                let Some(review) = lookup::review_mut(patch, &review)? else {
                    return Ok(());
                };

                review
                    .summary
                    .push(Edit::new(author, summary, timestamp, embeds));
                review.verdict = verdict;
                review.labels = labels;
                Ok(())
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewEditV2 {
    review: ReviewId,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    verdict: Option<Verdict>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    labels: Vec<Label>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    embeds: Vec<Embed<Uri>>,
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewEditV1 {
    review: ReviewId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    verdict: Option<Verdict>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    labels: Vec<Label>,
}

#[allow(clippy::unwrap_used)]
#[cfg(test)]
mod test {
    use serde_json::json;

    use crate::patch;

    use super::ReviewEdit;

    #[test]
    fn test_review_edit() {
        let v1 = json!({
            "type": "review.edit",
            "review": "89d45fb371eb2622ba88188d474347cc526d80bb",
            "summary": "lgtm",
            "verdict": "accept",
            "labels": [],
        });
        let v2 = json!({
            "type": "review.edit.v2",
            "review": "89d45fb371eb2622ba88188d474347cc526d80bb",
            "summary": "lgtm",
            "verdict": "accept",
            "labels": [],
            "embeds": [],
        });
        serde_json::from_value::<ReviewEdit>(v1.clone()).unwrap();
        serde_json::from_value::<ReviewEdit>(v2.clone()).unwrap();
        assert!(matches!(
            serde_json::from_value::<patch::Action>(v1).unwrap(),
            patch::Action::ReviewEdit { .. }
        ));
        assert!(matches!(
            serde_json::from_value::<patch::Action>(v2).unwrap(),
            patch::Action::ReviewEdit { .. }
        ));
    }
}

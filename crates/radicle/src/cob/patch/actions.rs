//! Keep track of the patch [`Action`] versions, to ensure compatibility where
//! possible.
//!
//! [`Action`]: super::Action

use radicle_cob::EntryId;
use serde::{Deserialize, Serialize};

use crate::cob::{
    thread,
    thread::{CommentId, Edit},
    ActorId, DiffLocation, Embed, Label, PartialLocation, Reaction, Timestamp, Uri,
};

use super::{encoding::Location, lookup, Error, Patch, ReviewId, RevisionId, Verdict};

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
    /// introduces `embeds` to the review summary.
    ///
    /// The `summary` of a [`super::Review`] is now an edit-history.
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

    /// Get the summary of the [`ReviewEdit`].
    ///
    /// The summary was optional in the first version, so it may be `None`.
    pub fn summary(&self) -> Option<&String> {
        match self {
            ReviewEdit::V1(ReviewEditV1 { summary, .. }) => summary.as_ref(),
            ReviewEdit::V2(ReviewEditV2 { summary, .. }) => Some(summary),
        }
    }

    /// Get the [`Verdict`] of the [`ReviewEdit`].
    pub fn verdict(&self) -> Option<&Verdict> {
        match self {
            ReviewEdit::V1(ReviewEditV1 { verdict, .. }) => verdict.as_ref(),
            ReviewEdit::V2(ReviewEditV2 { verdict, .. }) => verdict.as_ref(),
        }
    }

    /// Get the list of [`Label`]s of the [`ReviewEdit`].
    pub fn labels(&self) -> &[Label] {
        match self {
            ReviewEdit::V1(ReviewEditV1 { labels, .. }) => labels,
            ReviewEdit::V2(ReviewEditV2 { labels, .. }) => labels,
        }
    }

    /// Get the [`Embed`]s of the [`ReviewEdit`].
    ///
    /// [`Embed`]s were introduced in the second version of edits. For this
    /// reason, an [`Option`] is returned instead of a [`Vec`] – this allows to
    /// avoid an unnecessary clone of the [`Vec`] when it is present.
    pub fn embeds(&self) -> Option<&Vec<Embed<Uri>>> {
        match self {
            ReviewEdit::V1(_) => None,
            ReviewEdit::V2(ReviewEditV2 { embeds, .. }) => Some(embeds),
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

/// A review comment that keeps track of the different versions of actions.
///
/// [`ReviewComment::new`] will create the latest version of the action.
///
/// [`ReviewComment::run`] will apply the action to the given [`Patch`].
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ReviewComment {
    #[serde(rename = "review.comment")]
    V1(ReviewCommentV1),
    #[serde(rename = "review.comment.v2")]
    V2(ReviewCommentV2),
}

impl ReviewComment {
    /// Create the latest version of [`ReviewComment`].
    pub fn new(
        review: ReviewId,
        body: String,
        location: Option<DiffLocation>,
        reply_to: Option<CommentId>,
        embeds: Vec<Embed<Uri>>,
    ) -> Self {
        Self::V2(ReviewCommentV2 {
            review,
            body,
            location,
            reply_to,
            embeds,
        })
    }

    /// Get the [`ReviewId`] that this comment is applying to.
    pub fn review_id(&self) -> &ReviewId {
        match self {
            ReviewComment::V1(v1) => &v1.review,
            ReviewComment::V2(v2) => &v2.review,
        }
    }

    /// Get the body of the comment.
    pub fn body(&self) -> &String {
        match self {
            ReviewComment::V1(v1) => &v1.body,
            ReviewComment::V2(v2) => &v2.body,
        }
    }

    /// Get the [`CommentId`] this comment is replying to, if any.
    pub fn reply_to(&self) -> Option<&CommentId> {
        match self {
            ReviewComment::V1(v1) => v1.reply_to.as_ref(),
            ReviewComment::V2(v2) => v2.reply_to.as_ref(),
        }
    }

    /// Get the [`Embed`]s for this comment.
    pub fn embeds(&self) -> &[Embed<Uri>] {
        match self {
            ReviewComment::V1(v1) => &v1.embeds,
            ReviewComment::V2(v2) => &v2.embeds,
        }
    }

    /// Get the [`DiffLocation`] this comment is referring to, if any.
    pub fn location(&self) -> Option<&DiffLocation> {
        match self {
            ReviewComment::V1(_) => None,
            ReviewComment::V2(v2) => v2.location.as_ref(),
        }
    }

    /// Apply the action to the given [`Patch`].
    pub fn run(
        self,
        entry: EntryId,
        author: ActorId,
        timestamp: Timestamp,
        patch: &mut Patch,
    ) -> Result<(), Error> {
        match self {
            ReviewComment::V1(ReviewCommentV1 {
                review,
                body,
                location,
                reply_to,
                embeds,
            }) => {
                let context = patch
                    .revisions()
                    .find(|(_, rev)| rev.reviews().any(|(_, r)| r.id == review))
                    .map(|(_, rev)| rev.base)
                    // TODO
                    .expect("BUG: no revision for review");
                if let Some(review) = lookup::review_mut(patch, &review)? {
                    let location =
                        location.map(|loc| Location::V1(loc).into_diff_location(context));
                    thread::comment(
                        &mut review.comments,
                        entry,
                        author,
                        timestamp,
                        body,
                        reply_to,
                        location,
                        embeds,
                    )?;
                }
            }
            ReviewComment::V2(ReviewCommentV2 {
                review,
                body,
                location,
                reply_to,
                embeds,
            }) => {
                if let Some(review) = lookup::review_mut(patch, &review)? {
                    thread::comment(
                        &mut review.comments,
                        entry,
                        author,
                        timestamp,
                        body,
                        reply_to,
                        location,
                        embeds,
                    )?;
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewCommentV2 {
    review: ReviewId,
    body: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    location: Option<DiffLocation>,
    /// Comment this is a reply to.
    /// Should be [`None`] if it's the first comment.
    /// Should be [`Some`] otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reply_to: Option<CommentId>,
    /// Embeded content.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    embeds: Vec<Embed<Uri>>,
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewCommentV1 {
    review: ReviewId,
    body: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    location: Option<PartialLocation>,
    /// Comment this is a reply to.
    /// Should be [`None`] if it's the first comment.
    /// Should be [`Some`] otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reply_to: Option<CommentId>,
    /// Embeded content.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    embeds: Vec<Embed<Uri>>,
}

/// A revision comment that keeps track of the different versions of actions.
///
/// [`RevisionComment::new`] will create the latest version of the action.
///
/// [`RevisionComment::run`] will apply the action to the given [`Patch`].
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum RevisionComment {
    #[serde(rename = "revision.comment")]
    V1(RevisionCommentV1),
    #[serde(rename = "revision.comment.v2")]
    V2(RevisionCommentV2),
}

impl RevisionComment {
    /// Create the latest version of [`RevisionComment`].
    pub fn new(
        revision: RevisionId,
        location: Option<DiffLocation>,
        body: String,
        reply_to: Option<CommentId>,
        embeds: Vec<Embed<Uri>>,
    ) -> Self {
        Self::V2(RevisionCommentV2 {
            revision,
            location,
            body,
            reply_to,
            embeds,
        })
    }

    /// Get the [`RevisionId`] that this comment is applying to.
    pub fn revision(&self) -> &RevisionId {
        match self {
            Self::V1(v1) => &v1.revision,
            Self::V2(v2) => &v2.revision,
        }
    }

    /// Get the body of the comment.
    pub fn body(&self) -> &String {
        match self {
            Self::V1(v1) => &v1.body,
            Self::V2(v2) => &v2.body,
        }
    }

    /// Get the [`CommentId`] this comment is replying to, if any.
    pub fn reply_to(&self) -> Option<&CommentId> {
        match self {
            Self::V1(v1) => v1.reply_to.as_ref(),
            Self::V2(v2) => v2.reply_to.as_ref(),
        }
    }

    /// Get the [`Embed`]s for this comment.
    pub fn embeds(&self) -> &[Embed<Uri>] {
        match self {
            Self::V1(v1) => &v1.embeds,
            Self::V2(v2) => &v2.embeds,
        }
    }

    /// Get the [`DiffLocation`] this comment is referring to, if any.
    pub fn location(&self) -> Option<&DiffLocation> {
        match self {
            Self::V1(_) => None,
            Self::V2(v2) => v2.location.as_ref(),
        }
    }

    /// Apply the action to the given [`Patch`].
    pub fn run(
        self,
        entry: EntryId,
        author: ActorId,
        timestamp: Timestamp,
        patch: &mut Patch,
    ) -> Result<(), Error> {
        match self {
            Self::V1(RevisionCommentV1 {
                revision,
                location,
                body,
                reply_to,
                embeds,
            }) => {
                if let Some(revision) = lookup::revision_mut(patch, &revision)? {
                    let location =
                        location.map(|loc| Location::V1(loc).into_diff_location(revision.base));
                    thread::comment(
                        &mut revision.discussion,
                        entry,
                        author,
                        timestamp,
                        body,
                        reply_to,
                        location,
                        embeds,
                    )?;
                }
            }
            Self::V2(RevisionCommentV2 {
                revision,
                body,
                location,
                reply_to,
                embeds,
            }) => {
                if let Some(revision) = lookup::revision_mut(patch, &revision)? {
                    thread::comment(
                        &mut revision.discussion,
                        entry,
                        author,
                        timestamp,
                        body,
                        reply_to,
                        location,
                        embeds,
                    )?;
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RevisionCommentV2 {
    /// The revision to comment on.
    revision: RevisionId,
    /// For comments on the revision code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    location: Option<DiffLocation>,
    /// Comment body.
    body: String,
    /// Comment this is a reply to.
    /// Should be [`None`] if it's the top-level comment.
    /// Should be the root [`CommentId`] if it's a top-level comment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reply_to: Option<CommentId>,
    /// Embeded content.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    embeds: Vec<Embed<Uri>>,
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RevisionCommentV1 {
    /// The revision to comment on.
    revision: RevisionId,
    /// For comments on the revision code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    location: Option<PartialLocation>,
    /// Comment body.
    body: String,
    /// Comment this is a reply to.
    /// Should be [`None`] if it's the top-level comment.
    /// Should be the root [`CommentId`] if it's a top-level comment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reply_to: Option<CommentId>,
    /// Embeded content.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    embeds: Vec<Embed<Uri>>,
}

/// A revision reaction that keeps track of the different versions of actions.
///
/// [`RevisionReact::new`] will create the latest version of the action.
///
/// [`RevisionReact::run`] will apply the action to the given [`Patch`].
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum RevisionReact {
    #[serde(rename = "revision.react")]
    V1(RevisionReactV1),
    #[serde(rename = "revision.react.v2")]
    V2(RevisionReactV2),
}

impl RevisionReact {
    /// Create the latest version of [`RevisionReact`].
    pub fn new(
        revision: RevisionId,
        location: Option<DiffLocation>,
        reaction: Reaction,
        active: bool,
    ) -> Self {
        Self::V2(RevisionReactV2 {
            revision,
            location,
            reaction,
            active,
        })
    }

    /// Get the [`RevisionId`] that this reaction is applying to.
    pub fn revision(&self) -> &RevisionId {
        match self {
            Self::V1(v1) => &v1.revision,
            Self::V2(v2) => &v2.revision,
        }
    }

    /// The [`Reaction`] to the revision.
    pub fn reaction(&self) -> &Reaction {
        match self {
            Self::V1(v1) => &v1.reaction,
            Self::V2(v2) => &v2.reaction,
        }
    }

    /// Whether the [`Reaction`] is active.
    pub fn active(&self) -> bool {
        match self {
            Self::V1(v1) => v1.active,
            Self::V2(v2) => v2.active,
        }
    }

    /// Get the [`DiffLocation`] this reaction is referring to, if any.
    pub fn location(&self) -> Option<&DiffLocation> {
        match self {
            Self::V1(_) => None,
            Self::V2(v2) => v2.location.as_ref(),
        }
    }

    /// Apply the action to the given [`Patch`].
    pub fn run(self, author: ActorId, patch: &mut Patch) -> Result<(), Error> {
        match self {
            Self::V1(RevisionReactV1 {
                revision,
                location,
                reaction,
                active,
            }) => {
                if let Some(revision) = lookup::revision_mut(patch, &revision)? {
                    let key = (author, reaction);
                    let location =
                        location.map(|loc| Location::V1(loc).into_diff_location(revision.base));
                    let reactions = revision.reactions.entry(location).or_default();

                    if active {
                        reactions.insert(key);
                    } else {
                        reactions.remove(&key);
                    }
                }
            }
            Self::V2(RevisionReactV2 {
                revision,
                location,
                reaction,
                active,
            }) => {
                if let Some(revision) = lookup::revision_mut(patch, &revision)? {
                    let key = (author, reaction);
                    let reactions = revision.reactions.entry(location).or_default();

                    if active {
                        reactions.insert(key);
                    } else {
                        reactions.remove(&key);
                    }
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RevisionReactV2 {
    revision: RevisionId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    location: Option<DiffLocation>,
    reaction: Reaction,
    active: bool,
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RevisionReactV1 {
    revision: RevisionId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    location: Option<PartialLocation>,
    reaction: Reaction,
    active: bool,
}

#[allow(clippy::unwrap_used)]
#[cfg(test)]
mod test {
    use serde_json::json;

    use crate::patch;

    use super::*;

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

    #[test]
    fn test_review_comment() {
        let v1 = json!({
            "type": "review.comment",
            "review": "89d45fb371eb2622ba88188d474347cc526d80bb",
            "body": "This looks good to me",
            "location": {
                "commit": "b455c819807cd7a7543d03215570c72b7cb452d7",
                "path": "src/main.rs",
                "old": {
                    "type": "lines",
                    "range": {
                        "start": 10,
                        "end": 12
                    }
                },
                "new": {
                    "type": "lines",
                    "range": {
                        "start": 12,
                        "end": 14
                    }
                }
            },
            "replyTo": "92d77f9ec373261d91a65e68c95baaaa9fc9b95e",
            "embeds": []
        });
        let v2 = json!({
            "type": "review.comment.v2",
            "review": "89d45fb371eb2622ba88188d474347cc526d80bb",
            "body": "This looks good to me",
            "location": {
                "base": "b455c819807cd7a7543d03215570c72b7cb452d7",
                "head": "e67a8f4d32c830c24ed68ea21707923480830511",
                "path": "src/main.rs",
                "selection": {
                    "hunk": 0,
                    "range": {
                        "type": "lines",
                        "range": {
                            "start": 2,
                            "end": 4
                        }
                    }
                }
            },
            "replyTo": "92d77f9ec373261d91a65e68c95baaaa9fc9b95e",
            "embeds": []
        });
        serde_json::from_value::<ReviewComment>(v1.clone()).unwrap();
        serde_json::from_value::<ReviewComment>(v2.clone()).unwrap();
        assert!(matches!(
            serde_json::from_value::<patch::Action>(v1).unwrap(),
            patch::Action::ReviewComment { .. }
        ));
        assert!(matches!(
            serde_json::from_value::<patch::Action>(v2).unwrap(),
            patch::Action::ReviewComment { .. }
        ));
    }

    #[test]
    fn test_revision_comment() {
        let v1 = json!({
            "type": "revision.comment",
            "revision": "b455c819807cd7a7543d03215570c72b7cb452d7",
            "body": "Nice changes here",
            "location": {
                "commit": "e67a8f4d32c830c24ed68ea21707923480830511",
                "path": "src/lib.rs",
                "old": {
                    "type": "lines",
                    "range": {
                        "start": 25,
                        "end": 27
                    }
                },
                "new": {
                    "type": "lines",
                    "range": {
                        "start": 30,
                        "end": 32
                    }
                }
            },
            "replyTo": "05368e84fabb0fa5d2995741de87374812ae501c",
            "embeds": []
        });
        let v2 = json!({
            "type": "revision.comment.v2",
            "revision": "b455c819807cd7a7543d03215570c72b7cb452d7",
            "body": "Nice changes here",
            "location": {
                "base": "e67a8f4d32c830c24ed68ea21707923480830511",
                "head": "2a47bc0c7dd6238ce3416e19d2ba57f3a7626f64",
                "path": "src/lib.rs",
                "selection": {
                    "hunk": 1,
                    "range": {
                        "type": "lines",
                        "range": {
                            "start": 0,
                            "end": 3
                        }
                    }
                }
            },
            "replyTo": "05368e84fabb0fa5d2995741de87374812ae501c",
            "embeds": []
        });
        serde_json::from_value::<RevisionComment>(v1.clone()).unwrap();
        serde_json::from_value::<RevisionComment>(v2.clone()).unwrap();
        assert!(matches!(
            serde_json::from_value::<patch::Action>(v1).unwrap(),
            patch::Action::RevisionComment { .. }
        ));
        assert!(matches!(
            serde_json::from_value::<patch::Action>(v2).unwrap(),
            patch::Action::RevisionComment { .. }
        ));
    }

    #[test]
    fn test_revision_react() {
        let v1 = json!({
            "type": "revision.react",
            "revision": "e67a8f4d32c830c24ed68ea21707923480830511",
            "reaction": "👍",
            "active": true,
            "location": {
                "commit": "2a47bc0c7dd6238ce3416e19d2ba57f3a7626f64",
                "path": "src/utils.rs",
                "old": {
                    "type": "lines",
                    "range": {
                        "start": 15,
                        "end": 17
                    }
                },
                "new": {
                    "type": "lines",
                    "range": {
                        "start": 18,
                        "end": 20
                    }
                }
            }
        });
        let v2 = json!({
            "type": "revision.react.v2",
            "revision": "e67a8f4d32c830c24ed68ea21707923480830511",
            "reaction": "👍",
            "active": true,
            "location": {
                "base": "2a47bc0c7dd6238ce3416e19d2ba57f3a7626f64",
                "head": "a998ce691d6962ea75d861b25532f4c042c36f1a",
                "path": "src/utils.rs",
                "selection": {
                    "hunk": 2,
                    "range": {
                        "type": "lines",
                        "range": {
                            "start": 5,
                            "end": 8
                        }
                    }
                }
            }
        });
        serde_json::from_value::<RevisionReact>(v1.clone()).unwrap();
        serde_json::from_value::<RevisionReact>(v2.clone()).unwrap();
        assert!(matches!(
            serde_json::from_value::<patch::Action>(v1).unwrap(),
            patch::Action::RevisionReact { .. }
        ));
        assert!(matches!(
            serde_json::from_value::<patch::Action>(v2).unwrap(),
            patch::Action::RevisionReact { .. }
        ));
    }
}

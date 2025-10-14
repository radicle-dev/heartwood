pub mod error;

use std::{collections::BTreeSet, str::FromStr};

use serde_json as json;

use crate::{
    git,
    identity::crefs::GetCanonicalRefs as _,
    prelude::Did,
    storage::{refs, ReadRepository, RepositoryError},
};

use super::{Doc, PayloadError, PayloadId, RawDoc, Visibility};

/// [`EditVisibility`] allows the visibility of a [`RawDoc`] to be edited using
/// the [`visibility`] function.
///
/// Note that this differs from [`Visibility`] since the
/// [`EditVisibility::Private`] variant does not hold the allowed set of
/// [`Did`]s.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EditVisibility {
    #[default]
    Public,
    Private,
}

impl FromStr for EditVisibility {
    type Err = error::ParseEditVisibility;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "public" => Ok(EditVisibility::Public),
            "private" => Ok(EditVisibility::Private),
            _ => Err(error::ParseEditVisibility(s.to_owned())),
        }
    }
}

/// Change the visibility of the [`RawDoc`], using the provided
/// [`EditVisibility`].
pub fn visibility(mut raw: RawDoc, edit: EditVisibility) -> RawDoc {
    match (&mut raw.visibility, edit) {
        (Visibility::Public, EditVisibility::Public) => raw,
        (Visibility::Private { .. }, EditVisibility::Private) => raw,
        (Visibility::Public, EditVisibility::Private) => {
            raw.visibility = Visibility::private([]);
            raw
        }
        (Visibility::Private { .. }, EditVisibility::Public) => {
            raw.visibility = Visibility::Public;
            raw
        }
    }
}

/// Change the `allow` set of a document if the visibility is set to
/// [`Visibility::Private`].
///
/// All `Did`s in the `allow` set are added to the set, while all `Did`s in the
/// `disallow` set are removed from the set.
///
/// # Errors
///
/// This will fail when `allow` and `disallow` are not disjoint, i.e. they
/// contain at least share one `Did`.
///
/// This will fail when the [`Visibility`] of the document is
/// [`Visibility::Public`].
pub fn privacy_allow_list(
    mut raw: RawDoc,
    allow: BTreeSet<Did>,
    disallow: BTreeSet<Did>,
) -> Result<RawDoc, error::PrivacyAllowList> {
    if allow.is_empty() && disallow.is_empty() {
        return Ok(raw);
    }

    if !allow.is_disjoint(&disallow) {
        let overlap = allow
            .intersection(&disallow)
            .map(Did::to_string)
            .collect::<Vec<_>>();
        return Err(error::PrivacyAllowList::Overlapping(overlap));
    }

    match &mut raw.visibility {
        Visibility::Public => Err(error::PrivacyAllowList::PublicVisibility),
        Visibility::Private { allow: existing } => {
            for did in allow {
                existing.insert(did);
            }
            for did in disallow {
                existing.remove(&did);
            }
            Ok(raw)
        }
    }
}

/// Change the delegates of the document and perform some verification based on
/// the new set of delegates.
///
/// The set of `additions` are added to the delegates, while the set to
/// `removals` are removed from the delegates. Note that `removals` will take
/// precedence over the additions, i.e. if an addition and removal overlap, then
/// the [`Did`] will not be in the final set.
///
/// The result is either the updated [`RawDoc`] or a set of
/// [`error::DelegateVerification`] errors – which may be reported by the caller
/// to provide better error messaging.
///
/// # Errors
///
/// This will fail if an operation using the repository fails.
pub fn delegates<S>(
    mut raw: RawDoc,
    additions: Vec<Did>,
    removals: Vec<Did>,
    repo: &S,
) -> Result<Result<RawDoc, Vec<error::DelegateVerification>>, RepositoryError>
where
    S: ReadRepository,
{
    if additions.is_empty() && removals.is_empty() {
        return Ok(Ok(raw));
    }

    raw.delegates = raw
        .delegates
        .into_iter()
        .chain(additions)
        .filter(|d| !removals.contains(d))
        .collect::<Vec<_>>();
    match verify_delegates(&raw, repo)? {
        Some(errors) => Ok(Err(errors)),
        None => Ok(Ok(raw)),
    }
}

/// [`Payload`]: super::Payload
/// A change (update or insertion) to particular `key` within a [`Payload`]
/// in a document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PayloadUpsert {
    /// [`Payload`]: super::Payload
    /// The identifier for the document [`Payload`].
    pub id: PayloadId,
    /// [`Payload`]: super::Payload
    /// The key within the [`Payload`] that is being updated.
    pub key: String,
    /// [`Payload`]: super::Payload
    /// The value to update within the [`Payload`].
    pub value: json::Value,
}

// TODO(finto): I think this API would likely be much nicer if we use [JSON Patch][patch] and [JSON Merge Patch][merge]
//
// [patch]: https://datatracker.ietf.org/doc/html/rfc6902
// [merge]: https://datatracker.ietf.org/doc/html/rfc7396
/// [`Payload`]: super::Payload
/// Change (update or insert) a key in a [`Payload`] of the document,
/// using the provided `updates`.
///
/// # Errors
///
/// This fails if one of the [`PayloadId`]s does not point to a JSON object as
/// its value.
pub fn payload(
    mut raw: RawDoc,
    upserts: impl IntoIterator<Item = PayloadUpsert>,
) -> Result<RawDoc, error::PayloadError> {
    for PayloadUpsert { id, key, value } in upserts {
        if let Some(ref mut payload) = raw.payload.get_mut(&id) {
            if let Some(obj) = payload.as_object_mut() {
                if value.is_null() {
                    obj.remove(&key);
                } else {
                    obj.insert(key, value);
                }
            } else {
                return Err(error::PayloadError::ExpectedObject { id });
            }
        } else {
            raw.payload
                .insert(id, serde_json::json!({ key: value }).into());
        }
    }
    Ok(raw)
}

/// Verify the document.
///
/// This ensures performs the verification of the [`RawDoc`] into the [`Doc`],
/// while also checking the [`Project`] and [`CanonicalRefs`] will also
/// deserialize correctly.
///
/// [`Project`]: crate::identity::Project
/// [`CanonicalRefs`]: crate::identity::CanonicalRefs
pub fn verify(raw: RawDoc) -> Result<Doc, error::DocVerification> {
    let proposal = raw.clone().verified()?;
    // Verify that the project payload is valid
    // TODO(finto): perhaps this should be handled by JSON Schemas instead
    let project = match proposal.project() {
        Ok(project) => Some(project),
        Err(PayloadError::NotFound(_)) => None,
        Err(PayloadError::Json(e)) => {
            return Err(error::DocVerification::PayloadError {
                id: PayloadId::project(),
                err: e.to_string(),
            })
        }
    };
    // Ensure that if we have canonical reference rules and a project, that no
    // rule exists for the default branch. This rule must be synthesized when
    // constructing the canonical reference rules.
    match raw
        .raw_canonical_refs()
        .map(|rcrefs| rcrefs.and_then(|c| project.map(|p| (c, p))))
    {
        Ok(Some((crefs, project))) => {
            let default =
                git::fmt::Qualified::from(git::fmt::lit::refs_heads(project.default_branch()));
            let matches = crefs
                .raw_rules()
                .matches(&default)
                .map(|(pattern, _)| pattern.to_string())
                .collect::<Vec<_>>();
            if !matches.is_empty() {
                return Err(error::DocVerification::DisallowDefault { matches, default });
            }
        }
        _ => { /* we validate below */ }
    }
    // Verify that the canonical references payload is valid
    if let Err(e) = proposal.canonical_refs() {
        return Err(error::DocVerification::PayloadError {
            id: PayloadId::canonical_refs(),
            err: e.to_string(),
        });
    }
    Ok(proposal)
}

fn verify_delegates<S>(
    proposal: &RawDoc,
    repo: &S,
) -> Result<Option<Vec<error::DelegateVerification>>, RepositoryError>
where
    S: ReadRepository,
{
    let dids = &proposal.delegates;
    let threshold = proposal.threshold;
    let (canonical, _) = repo.canonical_head()?;
    let mut missing = Vec::with_capacity(dids.len());

    for did in dids {
        match refs::SignedRefsAt::load((*did).into(), repo)? {
            None => {
                missing.push(error::DelegateVerification::MissingDelegate { did: *did });
            }
            Some(refs::SignedRefsAt { sigrefs, .. }) => {
                if sigrefs.get(&canonical).is_none() {
                    missing.push(error::DelegateVerification::MissingDefaultBranch {
                        branch: canonical.to_ref_string(),
                        did: *did,
                    });
                }
            }
        }
    }

    Ok((dids.len() - missing.len() < threshold).then_some(missing))
}

#[allow(clippy::unwrap_used)]
#[cfg(test)]
mod test {
    use serde_json::json;

    use crate::{
        git,
        identity::{
            crefs::GetCanonicalRefs,
            doc::{update::error, PayloadId},
        },
        prelude::RawDoc,
        test::arbitrary,
    };

    use super::PayloadUpsert;

    #[test]
    fn test_can_update_crefs() {
        let raw = arbitrary::gen::<RawDoc>(1);
        let raw = super::payload(
            raw,
            [PayloadUpsert {
                id: PayloadId::canonical_refs(),
                key: "rules".to_string(),
                value: json!({
                    "refs/tags/*": {
                        "threshold": 1,
                        "allow": "delegates"
                    }
                }),
            }],
        )
        .unwrap();
        let verified = super::verify(raw);
        assert!(verified.is_ok(), "Unexpected error {verified:?}");
    }

    #[test]
    fn test_cannot_include_default_branch_rule() {
        let raw = arbitrary::gen::<RawDoc>(1);
        let branch = git::fmt::Qualified::from(git::fmt::lit::refs_heads(
            raw.project().unwrap().default_branch(),
        ));
        let raw = super::payload(
            raw,
            [PayloadUpsert {
                id: PayloadId::canonical_refs(),
                key: "rules".to_string(),
                value: json!({
                    "refs/tags/*": {
                        "threshold": 1,
                        "allow": "delegates"
                    },
                    branch.as_str(): {
                        "threshold": 1,
                        "allow": "delegates",
                    }
                }),
            }],
        )
        .unwrap();
        assert!(
            matches!(
                super::verify(raw),
                Err(error::DocVerification::DisallowDefault { .. })
            ),
            "Verification should be rejected for including default branch rule"
        )
    }

    #[test]
    fn test_default_branch_rule_exists_after_verification() {
        let raw = arbitrary::gen::<RawDoc>(1);
        let branch = git::fmt::Qualified::from(git::fmt::lit::refs_heads(
            raw.project().unwrap().default_branch(),
        ));
        let raw = super::payload(
            raw,
            [PayloadUpsert {
                id: PayloadId::canonical_refs(),
                key: "rules".to_string(),
                value: json!({
                    "refs/tags/*": {
                        "threshold": 1,
                        "allow": "delegates"
                    }
                }),
            }],
        )
        .unwrap();
        let verified = super::verify(raw).unwrap();
        let crefs = verified.canonical_refs().unwrap().unwrap();
        assert!(
            crefs.rules().matches(&branch).next().is_some(),
            "Default branch rule is missing!"
        );
    }
}

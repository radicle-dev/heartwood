pub mod error;

use std::{collections::BTreeSet, str::FromStr};

use serde_json as json;

use crate::{
    identity::crefs::GetCanonicalRefs as _,
    prelude::Did,
    storage::{refs, ReadRepository, RepositoryError},
};

use super::{Doc, PayloadId, RawDoc, Visibility};

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

// TODO(finto): I think this API would likely be much nicer if we use [JSON Patch][patch] and [JSON Merge Patch][merge]
//
// [patch]: https://datatracker.ietf.org/doc/html/rfc6902
// [merge]: https://datatracker.ietf.org/doc/html/rfc7396
/// Change the payload of the document, using the set of triples:
///
///   - [`PayloadId`]: the identifier for the document [`Payload`]
///   - [`String`]: the key within the [`Payload`] that is being updated
///   - [`json::Value`]: the value to update the [`Payload`]
///
/// # Errors
///
/// This fails if one of the [`PayloadId`]s does not point to a JSON object as
/// its value.
///
/// [`Payload`]: super::Payload
pub fn payload(
    mut raw: RawDoc,
    payload: Vec<(PayloadId, String, json::Value)>,
) -> Result<RawDoc, error::PayloadError> {
    for (id, key, val) in payload {
        if let Some(ref mut payload) = raw.payload.get_mut(&id) {
            if let Some(obj) = payload.as_object_mut() {
                if val.is_null() {
                    obj.remove(&key);
                } else {
                    obj.insert(key, val);
                }
            } else {
                return Err(error::PayloadError::ExpectedObject { id });
            }
        } else {
            raw.payload
                .insert(id, serde_json::json!({ key: val }).into());
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
    let proposal = raw.verified()?;
    // Verify that the payloads can still be parsed into the correct types.
    if let Err(super::PayloadError::Json(e)) = proposal.project() {
        return Err(error::DocVerification::PayloadJson {
            id: PayloadId::project(),
            err: e,
        });
    }
    if let Err(super::PayloadError::Json(e)) = proposal.canonical_refs() {
        return Err(error::DocVerification::PayloadJson {
            id: PayloadId::canonical_refs(),
            err: e,
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

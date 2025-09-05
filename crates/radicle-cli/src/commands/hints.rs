//! Provide auto-completion hints for CLI usage.

use clap_complete::CompletionCandidate;

use radicle::{
    identity::Did,
    issue::{cache::IssuesExt as _, Issues},
    storage::ReadStorage as _,
};

/// List the `DID`s associated with the current repository, and are assigned
/// to any issue, filtering by the `prefix`.
pub fn assignee_dids(prefix: &str) -> Option<Vec<String>> {
    let (_, rid) = radicle::rad::cwd().ok()?;
    radicle::Profile::load()
        .ok()
        .and_then(|profile| profile.storage.repository(rid).ok())
        .and_then(|repo| {
            Issues::open(&repo).ok().and_then(|issues| {
                issues
                    .all()
                    .map(|issues| {
                        issues
                            .flat_map(|issue| {
                                issue.map_or(vec![], |(_, issue)| {
                                    issue.assignees().cloned().collect::<Vec<_>>()
                                })
                            })
                            .filter_map(|did| {
                                let did = did.to_human();
                                did.starts_with(prefix).then_some(did)
                            })
                            .collect::<Vec<_>>()
                    })
                    .ok()
            })
        })
}

/// Wrapper for [`assignee_dids`] to support clap API.
pub(crate) fn assignee_dids_completer(current: &std::ffi::OsStr) -> Vec<CompletionCandidate> {
    let current = current.to_string_lossy();
    let result = assignee_dids(&current).unwrap_or_default();

    result.into_iter().map(CompletionCandidate::new).collect()
}

/// List the `IssueId`s associated with the current repository, filtered by the `prefix`.
pub fn issue_ids(prefix: &str) -> Option<Vec<String>> {
    let (_, rid) = radicle::rad::cwd().ok()?;
    let profile = radicle::Profile::load().ok()?;
    let repo = profile.storage.repository(rid).ok()?;
    let issues = profile.issues(&repo).ok()?;
    let ids = issues.ids(prefix).ok()?;
    Some(
        ids.filter_map(|result| result.ok().map(|id| id.to_string()))
            .collect(),
    )
}

/// Wrapper for [`issue_ids`] to support clap API.
pub(crate) fn issue_ids_completer(current: &std::ffi::OsStr) -> Vec<CompletionCandidate> {
    let current = current.to_string_lossy();
    let result = issue_ids(&current).unwrap_or_default();

    result.into_iter().map(CompletionCandidate::new).collect()
}

/// Wrapper for [`issue_ids`] to support clap API.
pub(crate) fn patch_ids_completer(current: &std::ffi::OsStr) -> Vec<CompletionCandidate> {
    todo!()
}

/// List the `DID`s associated with the current repository, filtered by the `prefix`.
// TODO: we could make this more like a fuzzy search
pub fn dids(prefix: &str) -> Option<Vec<String>> {
    let (_, rid) = radicle::rad::cwd().ok()?;
    let profile = radicle::Profile::load().ok()?;
    let repo = profile.storage.repository(rid).ok()?;
    let ids = repo.remote_ids().ok()?;
    Some(
        ids.filter_map(|nid| {
            let nid = nid.ok()?;
            let did = Did::from(nid).to_human();
            did.starts_with(prefix).then_some(did)
        })
        .collect(),
    )
}

/// Wrapper for [`dids`] to support clap API.
pub(crate) fn dids_completer(current: &std::ffi::OsStr) -> Vec<CompletionCandidate> {
    let current = current.to_string_lossy();
    let result = dids(&current).unwrap_or_default();

    result.into_iter().map(CompletionCandidate::new).collect()
}

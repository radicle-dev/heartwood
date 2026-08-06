use std::ffi::OsStr;

use clap::Parser as _;
use clap_complete::{ArgValueCompleter, CompletionCandidate};
use nonempty::NonEmpty;

use radicle::storage::ReadStorage as _;
use radicle_cob::{Entry, TypeName};

/// Complete repository IDs from local storage.
pub(crate) fn repo_id() -> ArgValueCompleter {
    ArgValueCompleter::new(repo_id_candidates)
}

/// Complete identity revision IDs in the current repository.
pub(crate) fn identity_revision_id() -> ArgValueCompleter {
    ArgValueCompleter::new(identity_revision_id_candidates)
}

/// Complete issue IDs in the current repository.
pub(crate) fn issue_id() -> ArgValueCompleter {
    cob_id_with_preview::<radicle::cob::issue::Issue>(
        &radicle::cob::issue::TYPENAME,
        radicle::cob::issue::Issue::title,
    )
}

/// Complete comment IDs for the issue in the current command line.
pub(crate) fn issue_comment_id() -> ArgValueCompleter {
    ArgValueCompleter::new(issue_comment_id_candidates)
}

/// Complete patch IDs in the current repository.
pub(crate) fn patch_id() -> ArgValueCompleter {
    cob_id_with_preview::<radicle::cob::patch::Patch>(
        &radicle::cob::patch::TYPENAME,
        radicle::cob::patch::Patch::title,
    )
}

fn repo_id_candidates(current: &OsStr) -> Vec<CompletionCandidate> {
    let Some(current) = current.to_str() else {
        return Vec::new();
    };
    let Ok(profile) = radicle::Profile::load() else {
        return Vec::new();
    };
    let Ok(repositories) = profile.storage.repositories() else {
        return Vec::new();
    };

    repositories
        .into_iter()
        .filter_map(|repository| {
            let rid = repo_id_value(repository.rid, current);
            let help = repository
                .doc
                .project()
                .ok()
                .map(|project| project.name().to_string().into());

            rid.starts_with(current)
                .then(|| CompletionCandidate::new(rid).help(help))
        })
        .collect()
}

fn repo_id_value(rid: radicle::identity::RepoId, current: &str) -> String {
    if current.starts_with("rad://") {
        format!("rad://{}", rid.canonical())
    } else if current.starts_with("rad:") || current.is_empty() {
        rid.urn()
    } else {
        rid.canonical()
    }
}

fn cob_id_with_preview<T>(typename: &'static TypeName, preview: fn(&T) -> &str) -> ArgValueCompleter
where
    T: radicle_cob::Evaluate<radicle::storage::git::Repository> + 'static,
{
    ArgValueCompleter::new(move |current: &OsStr| {
        cob_id_candidates_with_preview::<T>(current, typename, preview)
    })
}

#[allow(dead_code)]
fn cob_id_candidates(current: &OsStr, typename: &TypeName) -> Vec<CompletionCandidate> {
    let Some(current) = current.to_str() else {
        return Vec::new();
    };
    let Some(repository) = current_repository() else {
        return Vec::new();
    };
    let Ok(objects) = radicle_cob::list::<NonEmpty<Entry>, _>(&repository, typename, Some(current))
    else {
        return Vec::new();
    };

    objects
        .into_iter()
        .map(|object| object.id.to_string())
        .filter(|id| id.starts_with(current))
        .map(CompletionCandidate::new)
        .collect()
}

fn identity_revision_id_candidates(current: &OsStr) -> Vec<CompletionCandidate> {
    let Some(current) = current.to_str() else {
        return Vec::new();
    };
    let Some(repository) = current_repository() else {
        return Vec::new();
    };
    let Ok(identity) = radicle::cob::identity::Identity::load(&repository) else {
        return Vec::new();
    };

    identity
        .revisions()
        .filter_map(|revision| {
            let id = revision.id.to_string();

            id.starts_with(current)
                .then(|| CompletionCandidate::new(id).help(Some(revision.title.to_string().into())))
        })
        .collect()
}

fn issue_comment_id_candidates(current: &OsStr) -> Vec<CompletionCandidate> {
    let Some(current) = current.to_str() else {
        return Vec::new();
    };
    let Some(args) = issue_args(std::env::args_os()) else {
        return Vec::new();
    };
    let Some(issue_id) = args.issue_id().map(|id| id.as_str().to_owned()) else {
        return Vec::new();
    };
    let Some(repository) = repository(args.repo) else {
        return Vec::new();
    };
    let Some(issue_id) = repository
        .backend
        .revparse_single(&issue_id)
        .ok()
        .map(|object| object.id().into())
    else {
        return Vec::new();
    };
    let Ok(Some(issue)) = radicle_cob::get::<radicle::cob::issue::Issue, _>(
        &repository,
        &radicle::cob::issue::TYPENAME,
        &issue_id,
    ) else {
        return Vec::new();
    };

    issue
        .object
        .comments()
        .filter_map(|(id, comment)| {
            let id = id.to_string();

            id.starts_with(current)
                .then(|| CompletionCandidate::new(id).help(Some(comment.body().to_owned().into())))
        })
        .collect()
}

fn issue_args(
    args: impl IntoIterator<Item = impl Into<std::ffi::OsString>>,
) -> Option<crate::commands::issue::Args> {
    let args = args
        .into_iter()
        .map(Into::into)
        .collect::<Vec<std::ffi::OsString>>();
    let issue = args.iter().position(|arg| arg == "issue")?;
    let mut args = args.into_iter().skip(issue).peekable();
    let mut parse_args = Vec::new();

    while let Some(arg) = args.next() {
        if matches!(arg.to_str(), Some("--to" | "--reply-to" | "--edit")) {
            args.next();
        } else if !matches!(
            arg.to_str(),
            Some(arg) if arg.starts_with("--to=")
                || arg.starts_with("--reply-to=")
                || arg.starts_with("--edit=")
        ) {
            parse_args.push(arg);
        }
    }

    crate::commands::issue::Args::try_parse_from(parse_args).ok()
}

fn cob_id_candidates_with_preview<T>(
    current: &OsStr,
    typename: &TypeName,
    preview: fn(&T) -> &str,
) -> Vec<CompletionCandidate>
where
    T: radicle_cob::Evaluate<radicle::storage::git::Repository>,
{
    let Some(current) = current.to_str() else {
        return Vec::new();
    };
    let Some(repository) = current_repository() else {
        return Vec::new();
    };
    let Ok(objects) = radicle_cob::list::<T, _>(&repository, typename, Some(current)) else {
        return Vec::new();
    };

    objects
        .into_iter()
        .filter_map(|object| {
            let id = object.id.to_string();

            id.starts_with(current).then(|| {
                CompletionCandidate::new(id).help(Some(preview(&object.object).to_owned().into()))
            })
        })
        .collect()
}

fn current_repository() -> Option<radicle::storage::git::Repository> {
    repository(None)
}

fn repository(rid: Option<radicle::identity::RepoId>) -> Option<radicle::storage::git::Repository> {
    let profile = radicle::Profile::load().ok()?;
    let rid = match rid {
        Some(rid) => rid,
        None => radicle::rad::cwd().ok()?.1,
    };

    profile.storage.repository(rid).ok()
}

#[cfg(test)]
mod test {
    use super::{issue_args, repo_id_value};

    #[test]
    fn preserves_repository_id_spelling() {
        let rid = "rad:z3Tr6bC7ctEg2EHmLvknUr29mEDLH".parse().unwrap();

        assert_eq!(repo_id_value(rid, ""), "rad:z3Tr6bC7ctEg2EHmLvknUr29mEDLH");
        assert_eq!(
            repo_id_value(rid, "rad:z3"),
            "rad:z3Tr6bC7ctEg2EHmLvknUr29mEDLH"
        );
        assert_eq!(
            repo_id_value(rid, "rad://z3"),
            "rad://z3Tr6bC7ctEg2EHmLvknUr29mEDLH"
        );
        assert_eq!(repo_id_value(rid, "z3"), "z3Tr6bC7ctEg2EHmLvknUr29mEDLH");
    }

    #[test]
    fn parses_issue_completion_arguments() {
        assert_eq!(
            issue_args(["rad", "issue", "comment", "abc", "--reply-to", ""])
                .unwrap()
                .issue_id()
                .unwrap()
                .as_str(),
            "abc"
        );
        assert_eq!(
            issue_args([
                "rad",
                "issue",
                "--repo",
                "rad:z3Tr6bC7ctEg2EHmLvknUr29mEDLH",
                "react",
                "def",
                "--to",
                "",
            ])
            .unwrap()
            .issue_id()
            .unwrap()
            .as_str(),
            "def"
        );
    }
}

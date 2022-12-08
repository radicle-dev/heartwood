use std::collections::HashSet;

use axum::handler::Handler;
use axum::http::{header, HeaderValue};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Extension, Json, Router};
use hyper::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tower_http::set_header::SetResponseHeaderLayer;

use radicle::cob::issue::Issues;
use radicle::cob::thread::{self, CommentId};
use radicle::cob::Timestamp;
use radicle::git::raw::BranchType;
use radicle::identity::{Id, PublicKey};
use radicle::node::NodeId;
use radicle::storage::{Oid, ReadRepository, WriteRepository, WriteStorage};
use radicle_surf::git::History;
use radicle_surf::Revision::Sha;

use crate::api::axum_extra::{Path, Query};
use crate::api::error::Error;
use crate::api::project::Info;
use crate::api::{Context, PaginationQuery};

const CACHE_1_HOUR: &str = "public, max-age=3600, must-revalidate";

pub fn router(ctx: Context) -> Router {
    Router::new()
        .route("/projects", get(project_root_handler))
        .route("/projects/:project", get(project_handler))
        .route("/projects/:project/commits", get(history_handler))
        .route("/projects/:project/commits/:sha", get(commit_handler))
        .route(
            "/projects/:project/activity",
            get(
                activity_handler.layer(SetResponseHeaderLayer::if_not_present(
                    header::CACHE_CONTROL,
                    HeaderValue::from_static(CACHE_1_HOUR),
                )),
            ),
        )
        .route("/projects/:project/tree/:sha/*path", get(tree_handler))
        .route("/projects/:project/remotes", get(remotes_handler))
        .route("/projects/:project/remotes/:peer", get(remote_handler))
        .route("/projects/:project/blob/:sha/*path", get(blob_handler))
        .route("/projects/:project/readme/:sha", get(readme_handler))
        .route("/projects/:project/issues", get(issues_handler))
        .route("/projects/:project/issues/:id", get(issue_handler))
        .layer(Extension(ctx))
}

/// List all projects.
/// `GET /projects`
async fn project_root_handler(
    Extension(ctx): Extension<Context>,
    Query(qs): Query<PaginationQuery>,
) -> impl IntoResponse {
    let PaginationQuery { page, per_page } = qs;
    let page = page.unwrap_or(0);
    let per_page = per_page.unwrap_or(10);
    let storage = &ctx.profile.storage;
    let projects = storage
        .projects()?
        .into_iter()
        .filter_map(|id| {
            let Ok(repo) = storage.repository(id) else { return None };
            let Ok((_, head)) = repo.head() else { return None };
            let Ok(payload) = repo.project_of(ctx.profile.id()) else { return None };
            let Ok(issues) = Issues::open(ctx.profile.public_key, &repo) else { return None };
            let Ok(issues) = (*issues).count() else { return None };

            Some(Info {
                payload,
                head,
                issues,
                patches: 0,
                id,
            })
        })
        .skip(page * per_page)
        .take(per_page)
        .collect::<Vec<_>>();

    Ok::<_, Error>(Json(projects))
}

/// Get project metadata.
/// `GET /projects/:project`
async fn project_handler(
    Extension(ctx): Extension<Context>,
    Path(id): Path<Id>,
) -> impl IntoResponse {
    let info = ctx.project_info(id)?;

    Ok::<_, Error>(Json(info))
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct CommitsQueryString {
    pub parent: Option<String>,
    pub since: Option<i64>,
    pub until: Option<i64>,
    pub page: Option<usize>,
    pub per_page: Option<usize>,
}

/// Get project commit range.
/// `GET /projects/:project/commits?since=<sha>`
async fn history_handler(
    Extension(ctx): Extension<Context>,
    Path(project): Path<Id>,
    Query(qs): Query<CommitsQueryString>,
) -> impl IntoResponse {
    let CommitsQueryString {
        since,
        until,
        parent,
        page,
        per_page,
    } = qs;

    let (sha, fallback_to_head) = match parent {
        Some(commit) => (commit, false),
        None => {
            let info = ctx.project_info(project)?;

            (info.head.to_string(), true)
        }
    };

    let storage = &ctx.profile.storage;
    let repo = storage.repository(project)?;

    // If a pagination is defined, we do not want to paginate the commits, and we return all of them on the first page.
    let page = page.unwrap_or(0);
    let per_page = if per_page.is_none() && (since.is_some() || until.is_some()) {
        usize::MAX
    } else {
        per_page.unwrap_or(30)
    };

    let headers = History::new(repo.raw().into(), sha.as_str())?
        .filter(|q| {
            if let Ok(q) = q {
                if let (Some(since), Some(until)) = (since, until) {
                    q.committer.time.seconds() >= since && q.committer.time.seconds() < until
                } else if let Some(since) = since {
                    q.committer.time.seconds() >= since
                } else if let Some(until) = until {
                    q.committer.time.seconds() < until
                } else {
                    // If neither `since` nor `until` are specified, we include the commit.
                    true
                }
            } else {
                false
            }
        })
        .skip(page * per_page)
        .take(per_page)
        .filter_map(|commit| {
            if let Ok(commit) = commit {
                radicle_surf::commit(&repo.raw().into(), commit.id).ok()
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    let response = json!({
        "headers": headers,
        "stats":  stats(&repo)?,
    });

    if fallback_to_head {
        return Ok::<_, Error>((StatusCode::FOUND, Json(response)));
    }

    Ok::<_, Error>((StatusCode::OK, Json(response)))
}

/// Get project commit.
/// `GET /projects/:project/commits/:sha`
async fn commit_handler(
    Extension(ctx): Extension<Context>,
    Path((project, sha)): Path<(Id, Oid)>,
) -> impl IntoResponse {
    let storage = &ctx.profile.storage;
    let repo = storage.repository(project)?;
    let commit = radicle_surf::commit(&repo.raw().into(), sha)?;

    Ok::<_, Error>(Json(commit))
}

/// Get project activity for the past year.
/// `GET /projects/:project/activity`
async fn activity_handler(
    Extension(ctx): Extension<Context>,
    Path(project): Path<Id>,
) -> impl IntoResponse {
    let current_date = chrono::Utc::now().timestamp();
    let one_year_ago = chrono::Duration::weeks(52);
    let storage = &ctx.profile.storage;
    let repo = storage.repository(project)?;
    let (_, head) = repo.head()?;
    let timestamps = History::new(repo.raw().into(), head)?
        .filter_map(|a| {
            if let Ok(a) = a {
                let seconds = a.committer.time.seconds();
                if seconds > current_date - one_year_ago.num_seconds() {
                    return Some(seconds);
                }
            }
            None
        })
        .collect::<Vec<i64>>();

    Ok::<_, Error>((StatusCode::OK, Json(json!({ "activity": timestamps }))))
}

/// Get project source tree.
/// `GET /projects/:project/tree/:sha/*path`
async fn tree_handler(
    Extension(ctx): Extension<Context>,
    Path((project, sha, path)): Path<(Id, Oid, String)>,
) -> impl IntoResponse {
    let path = path.strip_prefix('/').ok_or(Error::NotFound)?.to_string();
    let storage = &ctx.profile.storage;
    let repo = storage.repository(project)?;
    let tree = radicle_surf::object::tree(&repo.raw().into(), Some(Sha { sha }), Some(path))?;
    let response = json!({
        "path": &tree.path,
        "entries": &tree.entries,
        "info": &tree.info,
        "stats": stats(&repo)?,
    });

    Ok::<_, Error>(Json(response))
}

/// Get all project remotes.
/// `GET /projects/:project/remotes`
async fn remotes_handler(
    Extension(ctx): Extension<Context>,
    Path(project): Path<Id>,
) -> impl IntoResponse {
    let storage = &ctx.profile.storage;
    let repo = storage.repository(project)?;
    let remotes = repo
        .remotes()?
        .filter_map(|r| r.map(|r| r.1).ok())
        .collect::<Vec<_>>();

    Ok::<_, Error>(Json(remotes))
}

/// Get project remote.
/// `GET /projects/:project/remotes/:peer`
async fn remote_handler(
    Extension(ctx): Extension<Context>,
    Path((project, node_id)): Path<(Id, NodeId)>,
) -> impl IntoResponse {
    let storage = &ctx.profile.storage;
    let repo = storage.repository(project)?;
    let remote = repo.remote(&node_id)?;

    Ok::<_, Error>(Json(remote))
}

/// Get project source file.
/// `GET /projects/:project/blob/:sha/*path`
async fn blob_handler(
    Extension(ctx): Extension<Context>,
    Path((project, sha, path)): Path<(Id, Oid, String)>,
) -> impl IntoResponse {
    let path = path.strip_prefix('/').ok_or(Error::NotFound)?;
    let storage = &ctx.profile.storage;
    let repo = storage.repository(project)?;
    let blob = radicle_surf::blob::blob(&repo.raw().into(), Some(Sha { sha }), path)?;

    Ok::<_, Error>(Json(blob))
}

/// Get project readme.
/// `GET /projects/:project/readme/:sha`
async fn readme_handler(
    Extension(ctx): Extension<Context>,
    Path((project, sha)): Path<(Id, Oid)>,
) -> impl IntoResponse {
    let storage = &ctx.profile.storage;
    let repo = storage.repository(project)?;
    let paths = &[
        "README",
        "README.md",
        "README.markdown",
        "README.txt",
        "README.rst",
        "Readme.md",
    ];

    for path in paths {
        if let Ok(blob) = radicle_surf::blob::blob(&repo.raw().into(), Some(Sha { sha }), path) {
            return Ok::<_, Error>(Json(blob));
        }
    }

    Err(radicle_surf::object::Error::PathNotFound(
        radicle_surf::file_system::Path::try_from("README").unwrap(),
    ))?
}

/// Get project issues list.
/// `GET /projects/:project/issues`
async fn issues_handler(
    Extension(ctx): Extension<Context>,
    Path(project): Path<Id>,
    Query(qs): Query<PaginationQuery>,
) -> impl IntoResponse {
    let PaginationQuery { page, per_page } = qs;
    let page = page.unwrap_or(0);
    let per_page = per_page.unwrap_or(10);
    let storage = &ctx.profile.storage;
    let repo = storage.repository(project)?;
    let issues = Issues::open(ctx.profile.public_key, &repo)?;
    let issues = issues
        .all()?
        .into_iter()
        .filter_map(|r| r.ok())
        .map(|(id, issue, _)| {
            json!({
                "id": id.to_string(),
                "author": issue.author(),
                "title": issue.title(),
                "state": issue.state(),
                "discussion": issue.comments().collect::<Comments>(),
                "tags": issue.tags().collect::<Vec<_>>(),
            })
        })
        .skip(page * per_page)
        .take(per_page)
        .collect::<Vec<_>>();

    Ok::<_, Error>(Json(issues))
}

/// Get project issue.
/// `GET /projects/:project/issues/:id`
async fn issue_handler(
    Extension(ctx): Extension<Context>,
    Path((project, issue_id)): Path<(Id, Oid)>,
) -> impl IntoResponse {
    let storage = &ctx.profile.storage;
    let repo = storage.repository(project)?;
    let issue = Issues::open(ctx.profile.public_key, &repo)?
        .get(&issue_id.into())?
        .ok_or(Error::NotFound)?;
    let issue = json!({
        "id": issue_id,
        "author": issue.author(),
        "title": issue.title(),
        "state": issue.state(),
        "discussion": issue.comments().collect::<Comments>(),
        "tags": issue.tags().collect::<Vec<_>>(),
    });

    Ok::<_, Error>(Json(issue))
}

#[derive(Serialize)]
struct Author {
    id: PublicKey,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Comment {
    author: Author,
    body: String,
    reactions: [String; 0],
    timestamp: Timestamp,
    reply_to: Option<CommentId>,
}

#[derive(Serialize)]
struct Comments(Vec<Comment>);

impl<'a> FromIterator<(&'a CommentId, &'a thread::Comment)> for Comments {
    fn from_iter<I: IntoIterator<Item = (&'a CommentId, &'a thread::Comment)>>(iter: I) -> Self {
        let mut comments = Vec::new();

        for (comment_id, comment) in iter {
            comments.push(Comment {
                author: Author { id: comment_id.1 },
                body: comment.body.to_owned(),
                reactions: [],
                timestamp: comment.timestamp,
                reply_to: comment.reply_to,
            });
        }

        Comments(comments)
    }
}

#[derive(Serialize)]
struct Stats {
    branches: usize,
    commits: usize,
    contributors: usize,
}

fn stats<R: WriteRepository>(repo: &R) -> Result<Stats, Error> {
    let branches = repo.raw().branches(Some(BranchType::Local))?.count();
    let (_, head) = repo.head()?;
    let mut commits = 0;
    let contributors = History::new(repo.raw().into(), head)?
        .filter_map(|commit| {
            commits += 1;
            if let Ok(commit) = commit {
                Some((commit.author.name, commit.author.email))
            } else {
                None
            }
        })
        .collect::<HashSet<_>>();

    Ok(Stats {
        branches,
        commits,
        contributors: contributors.len(),
    })
}

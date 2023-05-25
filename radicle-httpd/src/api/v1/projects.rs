use std::collections::BTreeMap;

use axum::extract::State;
use axum::handler::Handler;
use axum::http::{header, HeaderValue};
use axum::response::IntoResponse;
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use axum_auth::AuthBearer;
use hyper::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tower_http::set_header::SetResponseHeaderLayer;

use radicle::cob::{issue, patch, thread, ActorId, Tag};
use radicle::identity::Id;
use radicle::node::NodeId;
use radicle::storage::git::paths;
use radicle::storage::{ReadRepository, ReadStorage, WriteRepository};
use radicle_surf::{Glob, Oid, Repository};

use crate::api::error::Error;
use crate::api::project::Info;
use crate::api::{self, CobsQuery, Context, PaginationQuery};
use crate::axum_extra::{Path, Query};

const CACHE_1_HOUR: &str = "public, max-age=3600, must-revalidate";

pub fn router(ctx: Context) -> Router {
    Router::new()
        .route("/projects", get(project_root_handler))
        .route("/projects/:project", get(project_handler))
        .route("/projects/:project/commits", get(history_handler))
        .route("/projects/:project/commits/:sha", get(commit_handler))
        .route("/projects/:project/diff/:base/:oid", get(diff_handler))
        .route(
            "/projects/:project/activity",
            get(
                activity_handler.layer(SetResponseHeaderLayer::if_not_present(
                    header::CACHE_CONTROL,
                    HeaderValue::from_static(CACHE_1_HOUR),
                )),
            ),
        )
        .route("/projects/:project/tree/:sha/", get(tree_handler_root))
        .route("/projects/:project/tree/:sha/*path", get(tree_handler))
        .route("/projects/:project/remotes", get(remotes_handler))
        .route("/projects/:project/remotes/:peer", get(remote_handler))
        .route("/projects/:project/blob/:sha/*path", get(blob_handler))
        .route("/projects/:project/readme/:sha", get(readme_handler))
        .route(
            "/projects/:project/issues",
            post(issue_create_handler).get(issues_handler),
        )
        .route(
            "/projects/:project/issues/:id",
            patch(issue_update_handler).get(issue_handler),
        )
        .route(
            "/projects/:project/patches",
            post(patch_create_handler).get(patches_handler),
        )
        .route(
            "/projects/:project/patches/:id",
            get(patch_handler).patch(patch_update_handler),
        )
        .with_state(ctx)
}

/// List all projects.
/// `GET /projects`
async fn project_root_handler(
    State(ctx): State<Context>,
    Query(qs): Query<PaginationQuery>,
) -> impl IntoResponse {
    let PaginationQuery { page, per_page } = qs;
    let page = page.unwrap_or(0);
    let per_page = per_page.unwrap_or(10);
    let storage = &ctx.profile.storage;
    let projects = storage
        .repositories()?
        .into_iter()
        .filter_map(|id| {
            let Ok(repo) = storage.repository(id) else { return None };
            let Ok((_, head)) = repo.head() else { return None };
            let Ok((_, doc)) = repo.identity_doc() else { return None };
            let Ok(doc) = doc.verified() else { return None };
            let Ok(payload) = doc.project() else { return None };
            let Ok(issues) = issue::Issues::open(&repo) else { return None };
            let Ok(issues) = issues.counts() else { return None };
            let Ok(patches) = patch::Patches::open(&repo) else { return None };
            let Ok(patches) = patches.counts() else { return None };
            let delegates = doc.delegates;

            Some(Info {
                payload,
                delegates,
                head,
                issues,
                patches,
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
async fn project_handler(State(ctx): State<Context>, Path(id): Path<Id>) -> impl IntoResponse {
    let info = ctx.project_info(id)?;

    Ok::<_, Error>(Json(info))
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
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
    State(ctx): State<Context>,
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

    let sha = match parent {
        Some(commit) => commit,
        None => {
            let info = ctx.project_info(project)?;

            info.head.to_string()
        }
    };

    let storage = &ctx.profile.storage;
    let repo = Repository::open(paths::repository(storage, &project))?;

    // If a pagination is defined, we do not want to paginate the commits, and we return all of them on the first page.
    let page = page.unwrap_or(0);
    let per_page = if per_page.is_none() && (since.is_some() || until.is_some()) {
        usize::MAX
    } else {
        per_page.unwrap_or(30)
    };

    let commits = repo
        .history(&sha)?
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
        .map(|r| {
            r.and_then(|c| {
                let glob = Glob::all_heads().branches().and(Glob::all_remotes());
                let branches: Vec<String> = repo
                    .revision_branches(c.id, glob)?
                    .iter()
                    .map(|b| b.refname().to_string())
                    .collect();
                let diff = repo.diff_commit(c.id)?;
                Ok(json!({
                    "commit": api::json::commit(&c),
                    "diff": diff,
                    "branches": branches
                }))
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    let response = json!({
        "commits": commits,
        "stats":  repo.stats()?,
    });

    Ok::<_, Error>((StatusCode::OK, Json(response)))
}

/// Get project commit.
/// `GET /projects/:project/commits/:sha`
async fn commit_handler(
    State(ctx): State<Context>,
    Path((project, sha)): Path<(Id, Oid)>,
) -> impl IntoResponse {
    let storage = &ctx.profile.storage;
    let repo = Repository::open(paths::repository(storage, &project))?;
    let commit = repo.commit(sha)?;

    let diff = repo.diff_commit(commit.id)?;
    let glob = Glob::all_heads().branches().and(Glob::all_remotes());
    let branches: Vec<String> = repo
        .revision_branches(commit.id, glob)?
        .iter()
        .map(|b| b.refname().to_string())
        .collect();

    let response = json!({
      "commit": api::json::commit(&commit),
      "diff": diff,
      "branches": branches
    });
    Ok::<_, Error>(Json(response))
}

/// Get diff between two commits
/// `GET /projects/:project/diff/:base/:oid`
async fn diff_handler(
    State(ctx): State<Context>,
    Path((project, base, oid)): Path<(Id, Oid, Oid)>,
) -> impl IntoResponse {
    let storage = &ctx.profile.storage;
    let repo = Repository::open(paths::repository(storage, &project))?;
    let base = repo.commit(base)?;
    let commit = repo.commit(oid)?;
    let diff = repo.diff(base.id, commit.id)?;

    let commits = repo
        .history(commit.id)?
        .take_while(|c| {
            if let Ok(c) = c {
                c.id != base.id
            } else {
                false
            }
        })
        .map(|r| r.map(|c| api::json::commit(&c)))
        .collect::<Result<Vec<_>, _>>()?;

    let response = json!({ "diff": diff, "commits": commits });

    Ok::<_, Error>(Json(response))
}

/// Get project activity for the past year.
/// `GET /projects/:project/activity`
async fn activity_handler(
    State(ctx): State<Context>,
    Path(project): Path<Id>,
) -> impl IntoResponse {
    let current_date = chrono::Utc::now().timestamp();
    let one_year_ago = chrono::Duration::weeks(52);
    let storage = &ctx.profile.storage;
    let repo = Repository::open(paths::repository(storage, &project))?;
    let head = repo.head()?;
    let timestamps = repo
        .history(head)?
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

/// Get project source tree for '/' path.
/// `GET /projects/:project/tree/:sha/`
async fn tree_handler_root(
    State(ctx): State<Context>,
    Path((project, sha)): Path<(Id, Oid)>,
) -> impl IntoResponse {
    tree_handler(State(ctx), Path((project, sha, String::new()))).await
}

/// Get project source tree.
/// `GET /projects/:project/tree/:sha/*path`
async fn tree_handler(
    State(ctx): State<Context>,
    Path((project, sha, path)): Path<(Id, Oid, String)>,
) -> impl IntoResponse {
    let storage = &ctx.profile.storage;
    let repo = Repository::open(paths::repository(storage, &project))?;
    let tree = repo.tree(sha, &path)?;
    let stats = repo.stats_from(&sha)?;
    let response = api::json::tree(&tree, &path, &stats);

    Ok::<_, Error>(Json(response))
}

/// Get all project remotes.
/// `GET /projects/:project/remotes`
async fn remotes_handler(State(ctx): State<Context>, Path(project): Path<Id>) -> impl IntoResponse {
    let storage = &ctx.profile.storage;
    let repo = storage.repository(project)?;
    let delegates = repo.delegates()?;
    let tracking_store = &ctx.profile.tracking()?;
    let remotes = repo
        .remotes()?
        .filter_map(|r| r.map(|r| r.1).ok())
        .map(|remote| {
            let refs = remote
                .refs
                .iter()
                .filter_map(|(r, oid)| {
                    r.as_str()
                        .strip_prefix("refs/heads/")
                        .map(|head| (head.to_string(), oid))
                })
                .collect::<BTreeMap<String, &Oid>>();

            match tracking_store.alias(&remote.id) {
                Some(alias) => json!({
                    "id": remote.id,
                    "alias": alias,
                    "heads": refs,
                    "delegate": delegates.contains(&remote.id.into()),
                }),
                None => json!({
                    "id": remote.id,
                    "heads": refs,
                    "delegate": delegates.contains(&remote.id.into()),
                }),
            }
        })
        .collect::<Vec<_>>();

    Ok::<_, Error>(Json(remotes))
}

/// Get project remote.
/// `GET /projects/:project/remotes/:peer`
async fn remote_handler(
    State(ctx): State<Context>,
    Path((project, node_id)): Path<(Id, NodeId)>,
) -> impl IntoResponse {
    let storage = &ctx.profile.storage;
    let repo = storage.repository(project)?;
    let delegates = repo.delegates()?;
    let remote = repo.remote(&node_id)?;
    let refs = remote
        .refs
        .iter()
        .filter_map(|(r, oid)| {
            r.as_str()
                .strip_prefix("refs/heads/")
                .map(|head| (head.to_string(), oid))
        })
        .collect::<BTreeMap<String, &Oid>>();
    let remote = json!({
        "id": remote.id,
        "heads": refs,
        "delegate": delegates.contains(&remote.id.into()),
    });

    Ok::<_, Error>(Json(remote))
}

/// Get project source file.
/// `GET /projects/:project/blob/:sha/*path`
async fn blob_handler(
    State(ctx): State<Context>,
    Path((project, sha, path)): Path<(Id, Oid, String)>,
) -> impl IntoResponse {
    let storage = &ctx.profile.storage;
    let repo = Repository::open(paths::repository(storage, &project))?;
    let blob = repo.blob(sha, &path)?;
    let response = api::json::blob(&blob, &path);

    Ok::<_, Error>(Json(response))
}

/// Get project readme.
/// `GET /projects/:project/readme/:sha`
async fn readme_handler(
    State(ctx): State<Context>,
    Path((project, sha)): Path<(Id, Oid)>,
) -> impl IntoResponse {
    let storage = &ctx.profile.storage;
    let repo = Repository::open(paths::repository(storage, &project))?;
    let paths = &[
        "README",
        "README.md",
        "README.markdown",
        "README.txt",
        "README.rst",
        "Readme.md",
    ];

    for path in paths {
        if let Ok(blob) = repo.blob(sha, path) {
            let response = api::json::blob(&blob, path);
            return Ok::<_, Error>(Json(response));
        }
    }

    Err(Error::NotFound)
}

/// Get project issues list.
/// `GET /projects/:project/issues`
async fn issues_handler(
    State(ctx): State<Context>,
    Path(project): Path<Id>,
    Query(qs): Query<CobsQuery<api::IssueState>>,
) -> impl IntoResponse {
    let CobsQuery {
        page,
        per_page,
        state,
    } = qs;
    let page = page.unwrap_or(0);
    let per_page = per_page.unwrap_or(10);
    let state = state.unwrap_or_default();
    let storage = &ctx.profile.storage;
    let repo = storage.repository(project)?;
    let issues = issue::Issues::open(&repo)?;
    let mut issues: Vec<_> = issues
        .all()?
        .filter_map(|r| {
            let (id, issue, clock) = r.ok()?;
            (state.matches(issue.state())).then_some((id, issue, clock))
        })
        .collect::<Vec<_>>();

    issues.sort_by(|(_, a, _), (_, b, _)| b.timestamp().cmp(&a.timestamp()));
    let tracking_store = &ctx.profile.tracking()?;
    let issues = issues
        .into_iter()
        .map(|(id, issue, _)| api::json::issue(id, issue, tracking_store))
        .skip(page * per_page)
        .take(per_page)
        .collect::<Vec<_>>();

    Ok::<_, Error>(Json(issues))
}

#[derive(Debug, Deserialize, Serialize)]
pub struct IssueCreate {
    pub title: String,
    pub description: String,
    pub tags: Vec<Tag>,
    pub assignees: Vec<ActorId>,
}

/// Create a new issue.
/// `POST /projects/:project/issues`
async fn issue_create_handler(
    State(ctx): State<Context>,
    AuthBearer(token): AuthBearer,
    Path(project): Path<Id>,
    Json(issue): Json<IssueCreate>,
) -> impl IntoResponse {
    let sessions = ctx.sessions.read().await;
    sessions.get(&token).ok_or(Error::Auth("Unauthorized"))?;
    let storage = &ctx.profile.storage;
    let signer = ctx
        .profile
        .signer()
        .map_err(|_| Error::Auth("Unauthorized"))?;
    let repo = storage.repository(project)?;
    let mut issues = issue::Issues::open(&repo)?;
    let issue = issues
        .create(
            issue.title,
            issue.description,
            &issue.tags,
            &issue.assignees,
            &signer,
        )
        .map_err(Error::from)?;

    Ok::<_, Error>((
        StatusCode::CREATED,
        Json(json!({ "success": true, "id": issue.id().to_string() })),
    ))
}

/// Update an issue.
/// `PATCH /projects/:project/issues/:id`
async fn issue_update_handler(
    State(ctx): State<Context>,
    AuthBearer(token): AuthBearer,
    Path((project, issue_id)): Path<(Id, Oid)>,
    Json(action): Json<issue::Action>,
) -> impl IntoResponse {
    ctx.sessions
        .write()
        .await
        .get(&token)
        .ok_or(Error::Auth("Unauthorized"))?;

    let storage = &ctx.profile.storage;
    let signer = ctx.profile.signer().unwrap();
    let repo = storage.repository(project)?;
    let mut issues = issue::Issues::open(&repo)?;
    let mut issue = issues.get_mut(&issue_id.into())?;

    match action {
        issue::Action::Assign { add, remove } => {
            issue.assign(add, &signer)?;
            issue.unassign(remove, &signer)?;
        }
        issue::Action::Lifecycle { state } => {
            issue.lifecycle(state, &signer)?;
        }
        issue::Action::Tag { add, remove } => {
            issue.tag(add, remove, &signer)?;
        }
        issue::Action::Edit { title } => {
            issue.edit(title, &signer)?;
        }
        issue::Action::Thread { action } => match action {
            thread::Action::Comment { body, reply_to } => {
                if let Some(reply_to) = reply_to {
                    issue.comment(body, reply_to, &signer)?;
                } else {
                    issue.thread(body, &signer)?;
                }
            }
            thread::Action::React { to, reaction, .. } => {
                issue.react(to, reaction, &signer)?;
            }
            thread::Action::Edit { .. } => {
                todo!();
            }
            thread::Action::Redact { .. } => {
                todo!();
            }
        },
    };

    Ok::<_, Error>(Json(json!({ "success": true })))
}

/// Get project issue.
/// `GET /projects/:project/issues/:id`
async fn issue_handler(
    State(ctx): State<Context>,
    Path((project, issue_id)): Path<(Id, Oid)>,
) -> impl IntoResponse {
    let storage = &ctx.profile.storage;
    let repo = storage.repository(project)?;
    let issue = issue::Issues::open(&repo)?
        .get(&issue_id.into())?
        .ok_or(Error::NotFound)?;
    let tracking_store = &ctx.profile.tracking()?;

    Ok::<_, Error>(Json(api::json::issue(
        issue_id.into(),
        issue,
        tracking_store,
    )))
}

#[derive(Deserialize, Serialize)]
pub struct PatchCreate {
    pub title: String,
    pub description: String,
    pub target: Oid,
    pub oid: Oid,
    pub tags: Vec<Tag>,
}

/// Create a new patch.
/// `POST /projects/:project/patches`
async fn patch_create_handler(
    State(ctx): State<Context>,
    AuthBearer(token): AuthBearer,
    Path(project): Path<Id>,
    Json(patch): Json<PatchCreate>,
) -> impl IntoResponse {
    ctx.sessions
        .read()
        .await
        .get(&token)
        .ok_or(Error::Auth("Unauthorized"))?;
    let storage = &ctx.profile.storage;
    let signer = ctx
        .profile
        .signer()
        .map_err(|_| Error::Auth("Unauthorized"))?;
    let repo = storage.repository(project)?;
    let mut patches = patch::Patches::open(&repo)?;
    let base_oid = repo.raw().merge_base(*patch.target, *patch.oid)?;

    let patch = patches
        .create(
            patch.title,
            patch.description,
            patch::MergeTarget::default(),
            base_oid,
            patch.oid,
            &patch.tags,
            &signer,
        )
        .map_err(Error::from)?;

    Ok::<_, Error>((
        StatusCode::CREATED,
        Json(json!({ "success": true, "id": patch.id.to_string() })),
    ))
}
/// Update an patch.
/// `PATCH /projects/:project/patches/:id`
async fn patch_update_handler(
    State(ctx): State<Context>,
    AuthBearer(token): AuthBearer,
    Path((project, patch_id)): Path<(Id, Oid)>,
    Json(action): Json<patch::Action>,
) -> impl IntoResponse {
    ctx.sessions
        .write()
        .await
        .get(&token)
        .ok_or(Error::Auth("Unauthorized"))?;
    let storage = &ctx.profile.storage;
    let signer = ctx
        .profile
        .signer()
        .map_err(|_| Error::Auth("Unauthorized"))?;
    let repo = storage.repository(project)?;
    let mut patches = patch::Patches::open(&repo)?;
    let mut patch = patches.get_mut(&patch_id.into())?;
    match action {
        patch::Action::Edit { title, target } => {
            patch.edit(title, target, &signer)?;
        }
        patch::Action::EditRevision {
            revision,
            description,
        } => {
            patch.edit_revision(revision, description, &signer)?;
        }
        patch::Action::Tag { add, remove } => {
            patch.tag(add, remove, &signer)?;
        }
        patch::Action::Revision {
            description,
            base,
            oid,
        } => {
            patch.update(description, base, oid, &signer)?;
        }
        patch::Action::Redact { .. } => {
            todo!()
        }
        patch::Action::Lifecycle { state } => {
            patch.lifecycle(state, &signer)?;
        }
        patch::Action::Review {
            revision,
            comment,
            verdict,
            inline,
        } => {
            patch.review(revision, verdict, comment, inline, &signer)?;
        }
        patch::Action::Merge { revision, commit } => {
            patch.merge(revision, commit, &signer)?;
        }
        patch::Action::Thread { action, revision } => match action {
            thread::Action::Comment { body, reply_to } => {
                if let Some(reply_to) = reply_to {
                    patch.comment(revision, body, Some(reply_to), &signer)?;
                } else {
                    patch.thread(revision, body, &signer)?;
                }
            }
            thread::Action::Edit { .. } => {
                todo!();
            }
            thread::Action::Redact { .. } => {
                todo!();
            }
            thread::Action::React { .. } => {
                todo!();
            }
        },
    };

    Ok::<_, Error>(Json(json!({ "success": true })))
}

/// Get project patches list.
/// `GET /projects/:project/patches`
async fn patches_handler(
    State(ctx): State<Context>,
    Path(project): Path<Id>,
    Query(qs): Query<CobsQuery<api::PatchState>>,
) -> impl IntoResponse {
    let CobsQuery {
        page,
        per_page,
        state,
    } = qs;
    let page = page.unwrap_or(0);
    let per_page = per_page.unwrap_or(10);
    let state = state.unwrap_or_default();
    let storage = &ctx.profile.storage;
    let repo = storage.repository(project)?;
    let patches = patch::Patches::open(&repo)?;
    let mut patches = patches
        .all()?
        .filter_map(|r| {
            let (id, patch, clock) = r.ok()?;
            (state.matches(patch.state())).then_some((id, patch, clock))
        })
        .collect::<Vec<_>>();
    patches.sort_by(|(_, a, _), (_, b, _)| b.timestamp().cmp(&a.timestamp()));
    let tracking_store = &ctx.profile.tracking()?;
    let patches = patches
        .into_iter()
        .map(|(id, patch, _)| api::json::patch(id, patch, &repo, tracking_store))
        .skip(page * per_page)
        .take(per_page)
        .collect::<Vec<_>>();

    Ok::<_, Error>(Json(patches))
}

/// Get project patch.
/// `GET /projects/:project/patches/:id`
async fn patch_handler(
    State(ctx): State<Context>,
    Path((project, patch_id)): Path<(Id, Oid)>,
) -> impl IntoResponse {
    let storage = &ctx.profile.storage;
    let repo = storage.repository(project)?;
    let patch = patch::Patches::open(&repo)?
        .get(&patch_id.into())?
        .ok_or(Error::NotFound)?;
    let tracking_store = &ctx.profile.tracking()?;

    Ok::<_, Error>(Json(api::json::patch(
        patch_id.into(),
        patch,
        &repo,
        tracking_store,
    )))
}

#[cfg(test)]
mod routes {
    use axum::body::Body;
    use axum::http::StatusCode;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use crate::test::*;

    #[tokio::test]
    async fn test_projects_root() {
        let tmp = tempfile::tempdir().unwrap();
        let app = super::router(seed(tmp.path()));
        let response = get(&app, "/projects").await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.json().await,
            json!([
              {
                "name": "hello-world",
                "description": "Rad repository for tests",
                "defaultBranch": "master",
                "delegates": [DID],
                "head": HEAD,
                "patches": {
                  "open": 1,
                  "draft": 0,
                  "archived": 0,
                  "merged": 0,
                },
                "issues": {
                  "open": 1,
                  "closed": 0,
                },
                "id": RID
              }
            ])
        );
    }

    #[tokio::test]
    async fn test_projects() {
        let tmp = tempfile::tempdir().unwrap();
        let app = super::router(seed(tmp.path()));
        let response = get(&app, format!("/projects/{RID}")).await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.json().await,
            json!({
               "name": "hello-world",
               "description": "Rad repository for tests",
               "defaultBranch": "master",
               "delegates": [DID],
               "head": HEAD,
               "patches": {
                 "open": 1,
                 "draft": 0,
                 "archived": 0,
                 "merged": 0,
               },
               "issues": {
                 "open": 1,
                 "closed": 0,
               },
               "id": RID
            })
        );
    }

    #[tokio::test]
    async fn test_projects_commits_root() {
        let tmp = tempfile::tempdir().unwrap();
        let app = super::router(seed(tmp.path()));
        let response = get(&app, format!("/projects/{RID}/commits")).await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.json().await,
            json!({
              "commits": [
                {
                  "commit": {
                    "id": HEAD,
                    "author": {
                      "name": "Alice Liddell",
                      "email": "alice@radicle.xyz"
                    },
                    "summary": "Add another folder",
                    "description": "",
                    "committer": {
                      "name": "Alice Liddell",
                      "email": "alice@radicle.xyz",
                      "time": 1673003014
                    },
                  },
                  "diff": {
                    "added": [
                      {
                        "path": "README",
                        "diff": {
                          "type": "plain",
                          "hunks": [
                            {
                              "header": "@@ -0,0 +1 @@\n",
                              "lines": [
                                {
                                  "line": "Hello World!\n",
                                  "lineNo": 1,
                                  "type": "addition",
                                },
                              ],
                            },
                          ],
                          "eof": "noneMissing",
                        },
                      },
                      {
                        "path": "dir1/README",
                        "diff": {
                          "type": "plain",
                          "hunks": [
                            {
                              "header": "@@ -0,0 +1 @@\n",
                              "lines": [
                                {
                                  "line": "Hello World from dir1!\n",
                                  "lineNo": 1,
                                  "type": "addition"
                                }
                              ]
                            }
                          ],
                          "eof": "noneMissing",
                        }
                      }
                    ],
                    "deleted": [
                      {
                        "path": "CONTRIBUTING",
                        "diff": {
                          "type": "plain",
                          "hunks": [
                            {
                              "header": "@@ -1 +0,0 @@\n",
                              "lines": [
                                {
                                  "line": "Thank you very much!\n",
                                  "lineNo": 1,
                                  "type": "deletion",
                                },
                              ],
                            },
                          ],
                          "eof": "noneMissing",
                        },
                      },
                    ],
                    "moved": [],
                    "copied": [],
                    "modified": [],
                    "stats": {
                      "filesChanged": 3,
                      "insertions": 2,
                      "deletions": 1
                    }
                  },
                  "branches": [
                    "refs/heads/master"
                  ]
                },
                {
                  "commit": {
                    "id": PARENT,
                    "author": {
                      "name": "Alice Liddell",
                      "email": "alice@radicle.xyz"
                    },
                    "summary": "Add contributing file",
                    "description": "",
                    "committer": {
                      "name": "Alice Liddell",
                      "email": "alice@radicle.xyz",
                      "time": 1673002014,
                    },
                  },
                  "diff": {
                    "added": [
                      {
                        "path": "CONTRIBUTING",
                        "diff": {
                          "type": "plain",
                          "hunks": [
                            {
                              "header": "@@ -0,0 +1 @@\n",
                              "lines": [
                                {
                                  "line": "Thank you very much!\n",
                                  "lineNo": 1,
                                  "type": "addition",
                                },
                              ],
                            },
                          ],
                          "eof": "noneMissing",
                        },
                      },
                    ],
                    "deleted": [
                      {
                        "path": "README",
                        "diff": {
                          "type": "plain",
                          "hunks": [
                            {
                              "header": "@@ -1 +0,0 @@\n",
                              "lines": [
                                {
                                  "line": "Hello World!\n",
                                  "lineNo": 1,
                                  "type": "deletion",
                                },
                              ],
                            },
                          ],
                          "eof": "noneMissing",
                        },
                      },
                    ],
                    "moved": [],
                    "copied": [],
                    "modified": [],
                    "stats": {
                      "filesChanged": 2,
                      "insertions": 1,
                      "deletions": 1,
                    },
                  },
                  "branches": [
                    "refs/heads/master",
                  ],
                },
                {
                  "commit": {
                    "id": INITIAL_COMMIT,
                    "author": {
                      "name": "Alice Liddell",
                      "email": "alice@radicle.xyz",
                    },
                    "summary": "Initial commit",
                    "description": "",
                    "committer": {
                      "name": "Alice Liddell",
                      "email": "alice@radicle.xyz",
                      "time": 1673001014,
                    },
                  },
                  "diff": {
                    "added": [
                      {
                        "path": "README",
                        "diff": {
                          "type": "plain",
                          "hunks": [
                            {
                              "header": "@@ -0,0 +1 @@\n",
                              "lines": [
                                {
                                  "line": "Hello World!\n",
                                  "lineNo": 1,
                                  "type": "addition"
                                }
                              ]
                            }
                          ],
                          "eof": "noneMissing",
                        }
                      }
                    ],
                    "deleted": [],
                    "moved": [],
                    "copied": [],
                    "modified": [],
                    "stats": {
                      "filesChanged": 1,
                      "insertions": 1,
                      "deletions": 0
                    }
                  },
                  "branches": [
                    "refs/heads/master"
                  ]
                }
              ],
              "stats": {
                "commits": 3,
                "branches": 1,
                "contributors": 1
              }
            })
        );
    }

    #[tokio::test]
    async fn test_projects_commits() {
        let tmp = tempfile::tempdir().unwrap();
        let app = super::router(seed(tmp.path()));
        let response = get(&app, format!("/projects/{RID}/commits/{HEAD}")).await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.json().await,
            json!({
              "commit": {
                "id": HEAD,
                "author": {
                  "name": "Alice Liddell",
                  "email": "alice@radicle.xyz"
                },
                "summary": "Add another folder",
                "description": "",
                "committer": {
                  "name": "Alice Liddell",
                  "email": "alice@radicle.xyz",
                  "time": 1673003014
                },
              },
              "diff": {
                "added": [
                  {
                    "path": "README",
                    "diff": {
                        "type": "plain",
                        "hunks": [
                          {
                            "header": "@@ -0,0 +1 @@\n",
                            "lines": [
                              {
                                "line": "Hello World!\n",
                                "lineNo": 1,
                                "type": "addition",
                              },
                            ],
                          },
                        ],
                        "eof": "noneMissing",
                    },
                  },
                  {
                    "path": "dir1/README",
                    "diff": {
                      "type": "plain",
                      "hunks": [
                        {
                          "header": "@@ -0,0 +1 @@\n",
                          "lines": [
                            {
                              "line": "Hello World from dir1!\n",
                              "lineNo": 1,
                              "type": "addition"
                            }
                          ]
                        }
                      ],
                      "eof": "noneMissing",
                    }
                  }
                ],
                "deleted": [
                  {
                    "path": "CONTRIBUTING",
                    "diff": {
                        "type": "plain",
                        "hunks": [
                          {
                            "header": "@@ -1 +0,0 @@\n",
                            "lines": [
                              {
                                "line": "Thank you very much!\n",
                                "lineNo": 1,
                                "type": "deletion",
                              },
                            ],
                          },
                        ],
                        "eof": "noneMissing",
                    },
                  },
                ],
                "moved": [],
                "copied": [],
                "modified": [],
                "stats": {
                  "filesChanged": 3,
                  "insertions": 2,
                  "deletions": 1
                }
              },
              "branches": [
                "refs/heads/master"
              ]
            })
        );
    }

    #[tokio::test]
    async fn test_projects_tree() {
        let tmp = tempfile::tempdir().unwrap();
        let app = super::router(seed(tmp.path()));
        let response = get(&app, format!("/projects/{RID}/tree/{HEAD}/")).await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.json().await,
            json!({
                "entries": [
                  {
                    "path": "dir1",
                    "name": "dir1",
                    "kind": "tree"
                  },
                  {
                    "path": "README",
                    "name": "README",
                    "kind": "blob"
                  }
                ],
                "lastCommit": {
                  "id": HEAD,
                  "author": {
                    "name": "Alice Liddell",
                    "email": "alice@radicle.xyz"
                  },
                  "summary": "Add another folder",
                  "description": "",
                  "committer": {
                    "name": "Alice Liddell",
                    "email": "alice@radicle.xyz",
                    "time": 1673003014
                  },
                },
                "name": "",
                "path": "",
                "stats": {
                  "commits": 3,
                  "branches": 1,
                  "contributors": 1
                }
              }
            )
        );

        let response = get(&app, format!("/projects/{RID}/tree/{HEAD}/dir1")).await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.json().await,
            json!({
              "entries": [
                {
                  "path": "dir1/README",
                  "name": "README",
                  "kind": "blob"
                }
              ],
              "lastCommit": {
                "id": HEAD,
                "author": {
                  "name": "Alice Liddell",
                  "email": "alice@radicle.xyz"
                },
                "summary": "Add another folder",
                "description": "",
                "committer": {
                  "name": "Alice Liddell",
                  "email": "alice@radicle.xyz",
                  "time": 1673003014
                },
              },
              "name": "dir1",
              "path": "dir1",
              "stats": {
                "branches": 1,
                "commits": 3,
                "contributors": 1
              }
            })
        );
    }

    #[tokio::test]
    async fn test_projects_remotes_root() {
        let tmp = tempfile::tempdir().unwrap();
        let app = super::router(seed(tmp.path()));
        let response = get(&app, format!("/projects/{RID}/remotes")).await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.json().await,
            json!([
              {
                "id": "z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi",
                "heads": {
                  "master": HEAD
                },
                "delegate": true
              }
            ])
        );
    }

    #[tokio::test]
    async fn test_projects_remotes() {
        let tmp = tempfile::tempdir().unwrap();
        let app = super::router(seed(tmp.path()));
        let response = get(
            &app,
            format!("/projects/{RID}/remotes/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi"),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.json().await,
            json!({
                "id": "z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi",
                "heads": {
                    "master": HEAD
                },
                "delegate": true
            })
        );
    }

    #[tokio::test]
    async fn test_projects_blob() {
        let tmp = tempfile::tempdir().unwrap();
        let app = super::router(seed(tmp.path()));
        let response = get(&app, format!("/projects/{RID}/blob/{HEAD}/README")).await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.json().await,
            json!({
                "binary": false,
                "name": "README",
                "path": "README",
                "lastCommit": {
                  "id": HEAD,
                  "author": {
                    "name": "Alice Liddell",
                    "email": "alice@radicle.xyz"
                  },
                  "summary": "Add another folder",
                  "description": "",
                  "committer": {
                    "name": "Alice Liddell",
                    "email": "alice@radicle.xyz",
                    "time": 1673003014
                  },
                },
                "content": "Hello World!\n",
            })
        );
    }

    #[tokio::test]
    async fn test_projects_readme() {
        let tmp = tempfile::tempdir().unwrap();
        let app = super::router(seed(tmp.path()));
        let response = get(&app, format!("/projects/{RID}/readme/{INITIAL_COMMIT}")).await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.json().await,
            json!({
                "binary": false,
                "content": "Hello World!\n",
                "lastCommit": {
                  "id": INITIAL_COMMIT,
                  "author": {
                    "name": "Alice Liddell",
                    "email": "alice@radicle.xyz"
                  },
                  "summary": "Initial commit",
                  "description": "",
                  "committer": {
                    "name": "Alice Liddell",
                    "email": "alice@radicle.xyz",
                    "time": 1673001014
                  },
                },
                "name": "README",
                "path": "README"
            })
        );
    }

    #[tokio::test]
    async fn test_projects_diff() {
        let tmp = tempfile::tempdir().unwrap();
        let app = super::router(seed(tmp.path()));
        let response = get(
            &app,
            format!("/projects/{RID}/diff/{INITIAL_COMMIT}/{HEAD}"),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.json().await,
            json!({
                "diff": {
                  "added": [
                    {
                      "path": "dir1/README",
                      "diff": {
                        "type": "plain",
                        "hunks": [
                          {
                            "header": "@@ -0,0 +1 @@\n",
                            "lines": [
                              {
                                "line": "Hello World from dir1!\n",
                                "lineNo": 1,
                                "type": "addition",
                              },
                            ],
                          },
                        ],
                        "eof": "noneMissing",
                      },
                    },
                  ],
                  "deleted": [],
                  "moved": [],
                  "copied": [],
                  "modified": [],
                  "stats": {
                    "filesChanged": 1,
                    "insertions": 1,
                    "deletions": 0,
                  },
                },
                "commits": [
                  {
                    "id": HEAD,
                    "author": {
                      "name": "Alice Liddell",
                      "email": "alice@radicle.xyz",
                    },
                    "summary": "Add another folder",
                    "description": "",
                    "committer": {
                      "name": "Alice Liddell",
                      "email": "alice@radicle.xyz",
                      "time": 1673003014,
                    },
                  },
                  {
                    "id": PARENT,
                    "author": {
                      "name": "Alice Liddell",
                      "email": "alice@radicle.xyz",
                    },
                    "summary": "Add contributing file",
                    "description": "",
                    "committer": {
                      "name": "Alice Liddell",
                      "email": "alice@radicle.xyz",
                      "time": 1673002014,
                    }
                  }
                ],
            })
        );
    }

    #[tokio::test]
    async fn test_projects_issues_root() {
        let tmp = tempfile::tempdir().unwrap();
        let app = super::router(seed(tmp.path()));
        let response = get(&app, format!("/projects/{RID}/issues")).await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.json().await,
            json!([
              {
                "id": ISSUE_ID,
                "author": {
                  "id": DID
                },
                "title": "Issue #1",
                "state": {
                  "status": "open"
                },
                "assignees": [],
                "discussion": [
                  {
                    "id": ISSUE_ID,
                    "author": {
                      "id": DID
                    },
                    "body": "Change 'hello world' to 'hello everyone'",
                    "reactions": [],
                    "timestamp": TIMESTAMP,
                    "replyTo": null
                  }
                ],
                "tags": []
              }
            ])
        );
    }

    #[tokio::test]
    async fn test_projects_issues_create() {
        const CREATED_ISSUE_ID: &str = "b457364fbe2ef0eac69a835a087f60ee13ccb367";

        let tmp = tempfile::tempdir().unwrap();
        let ctx = contributor(tmp.path());
        let app = super::router(ctx.to_owned());

        create_session(ctx).await;

        let body = serde_json::to_vec(&json!({
            "title": "Issue #2",
            "description": "Change 'hello world' to 'hello everyone'",
            "tags": ["bug"],
            "assignees": [],
        }))
        .unwrap();

        let response = post(
            &app,
            format!("/projects/{CONTRIBUTOR_RID}/issues"),
            Some(Body::from(body)),
            Some(SESSION_ID.to_string()),
        )
        .await;

        assert_eq!(response.status(), StatusCode::CREATED);
        assert_eq!(
            response.json().await,
            json!({ "success": true, "id": CREATED_ISSUE_ID })
        );

        let response = get(
            &app,
            format!("/projects/{CONTRIBUTOR_RID}/issues/{CREATED_ISSUE_ID}"),
        )
        .await;

        assert_eq!(
            response.json().await,
            json!({
              "id": CREATED_ISSUE_ID,
              "author": {
                "id": CONTRIBUTOR_DID,
              },
              "assignees": [],
              "title": "Issue #2",
              "state": {
                "status": "open",
              },
              "discussion": [{
                "id": CREATED_ISSUE_ID,
                "author": {
                  "id": CONTRIBUTOR_DID,
                },
                "body": "Change 'hello world' to 'hello everyone'",
                "reactions": [],
                "timestamp": TIMESTAMP,
                "replyTo": null,
              }],
              "tags": [
                  "bug",
              ],
            })
        );
    }

    #[tokio::test]
    async fn test_projects_issues_comment() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = contributor(tmp.path());
        let app = super::router(ctx.to_owned());

        create_session(ctx).await;

        let body = serde_json::to_vec(&json!({
          "type": "thread",
          "action": {
            "type": "comment",
            "body": "This is first-level comment",
          }
        }))
        .unwrap();

        let response = patch(
            &app,
            format!("/projects/{CONTRIBUTOR_RID}/issues/{CONTRIBUTOR_ISSUE_ID}"),
            Some(Body::from(body)),
            Some(SESSION_ID.to_string()),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.json().await, json!({ "success": true }));

        let body = serde_json::to_vec(&json!({
          "type": "thread",
          "action": {
            "type": "react",
            "to": "9685b141c2e939c3d60f8ca34f8c7bf01a609af1",
            "reaction": "🚀",
            "active": true,
          }
        }))
        .unwrap();
        patch(
            &app,
            format!("/projects/{CONTRIBUTOR_RID}/issues/{CONTRIBUTOR_ISSUE_ID}"),
            Some(Body::from(body)),
            Some(SESSION_ID.to_string()),
        )
        .await;

        let response = get(
            &app,
            format!("/projects/{CONTRIBUTOR_RID}/issues/{CONTRIBUTOR_ISSUE_ID}"),
        )
        .await;

        assert_eq!(
            response.json().await,
            json!({
              "id": CONTRIBUTOR_ISSUE_ID,
              "author": {
                "id": CONTRIBUTOR_DID,
              },
              "title": "Issue #1",
              "state": {
                "status": "open",
              },
              "assignees": [],
              "discussion": [
                {
                  "id": ISSUE_DISCUSSION_ID,
                  "author": {
                    "id": CONTRIBUTOR_DID,
                  },
                  "body": "Change 'hello world' to 'hello everyone'",
                  "reactions": [],
                  "timestamp": TIMESTAMP,
                  "replyTo": null,
                },
                {
                  "id": "9685b141c2e939c3d60f8ca34f8c7bf01a609af1",
                  "author": {
                    "id": CONTRIBUTOR_DID,
                  },
                  "body": "This is first-level comment",
                  "reactions": [
                    [
                      "z6Mkk7oqY4pPxhMmGEotDYsFo97vhCj85BLY1H256HrJmjN8",
                      "🚀",
                    ],
                  ],
                  "timestamp": TIMESTAMP,
                  "replyTo": null,
                },
              ],
              "tags": [],
            })
        );
    }

    #[tokio::test]
    async fn test_projects_issues_reply() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = contributor(tmp.path());
        let app = super::router(ctx.to_owned());

        create_session(ctx).await;

        let body = serde_json::to_vec(&json!({
          "type": "thread",
          "action": {
            "type": "comment",
            "body": "This is a reply to the first comment",
            "replyTo": ISSUE_DISCUSSION_ID,
          }
        }))
        .unwrap();

        let _ = get(&app, format!("/projects/{CONTRIBUTOR_RID}/issues")).await;
        let response = patch(
            &app,
            format!("/projects/{CONTRIBUTOR_RID}/issues/{CONTRIBUTOR_ISSUE_ID}"),
            Some(Body::from(body)),
            Some(SESSION_ID.to_string()),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.json().await, json!({ "success": true }));

        let response = get(
            &app,
            format!("/projects/{CONTRIBUTOR_RID}/issues/{CONTRIBUTOR_ISSUE_ID}"),
        )
        .await;

        assert_eq!(
            response.json().await,
            json!({
              "id": CONTRIBUTOR_ISSUE_ID,
              "author": {
                "id": CONTRIBUTOR_DID,
              },
              "assignees": [],
              "title": "Issue #1",
              "state": {
                "status": "open",
              },
              "discussion": [
                {
                  "id": ISSUE_DISCUSSION_ID,
                  "author": {
                    "id": CONTRIBUTOR_DID,
                  },
                  "body": "Change 'hello world' to 'hello everyone'",
                  "reactions": [],
                  "timestamp": TIMESTAMP,
                  "replyTo": null,
                },
                {
                  "id": ISSUE_COMMENT_ID,
                  "author": {
                    "id": CONTRIBUTOR_DID,
                  },
                  "body": "This is a reply to the first comment",
                  "reactions": [],
                  "timestamp": TIMESTAMP,
                  "replyTo": ISSUE_DISCUSSION_ID,
                },
              ],
              "tags": [],
            })
        );
    }

    #[tokio::test]
    async fn test_projects_patches() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = contributor(tmp.path());
        let app = super::router(ctx.to_owned());
        let response = get(&app, format!("/projects/{CONTRIBUTOR_RID}/patches")).await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.json().await,
            json!([
              {
                "id": CONTRIBUTOR_PATCH_ID,
                "author": {
                  "id": CONTRIBUTOR_DID
                },
                "title": "A new `hello world`",
                "state": { "status": "open" },
                "target": "delegates",
                "tags": [],
                "merges": [],
                "reviewers": [],
                "revisions": [
                  {
                    "id": CONTRIBUTOR_PATCH_ID,
                    "description": "change `hello world` in README to something else",
                    "base": PARENT,
                    "oid": HEAD,
                    "refs": [
                      "refs/heads/master",
                    ],
                    "discussions": [],
                    "timestamp": TIMESTAMP,
                    "reviews": [],
                  }
                ],
              }
            ])
        );

        let response = get(
            &app,
            format!("/projects/{CONTRIBUTOR_RID}/patches/{CONTRIBUTOR_PATCH_ID}"),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.json().await,
            json!(
              {
                "id": CONTRIBUTOR_PATCH_ID,
                "author": {
                  "id": CONTRIBUTOR_DID
                },
                "title": "A new `hello world`",
                "state": { "status": "open" },
                "target": "delegates",
                "tags": [],
                "merges": [],
                "reviewers": [],
                "revisions": [
                  {
                    "id": CONTRIBUTOR_PATCH_ID,
                    "description": "change `hello world` in README to something else",
                    "base": PARENT,
                    "oid": HEAD,
                    "refs": [
                      "refs/heads/master",
                    ],
                    "discussions": [],
                    "timestamp": TIMESTAMP,
                    "reviews": [],
                  }
                ],
              }
            )
        );
    }

    #[tokio::test]
    async fn test_projects_create_patches() {
        const CREATED_PATCH_ID: &str = "768e76ae6611d9392f04122a5aa7a587b47b9e19";

        let tmp = tempfile::tempdir().unwrap();
        let ctx = contributor(tmp.path());
        let app = super::router(ctx.to_owned());

        create_session(ctx).await;

        let body = serde_json::to_vec(&json!({
          "title": "Update README",
          "description": "Do some changes to README",
          "target": INITIAL_COMMIT,
          "oid": HEAD,
          "tags": [],
        }))
        .unwrap();

        let response = post(
            &app,
            format!("/projects/{CONTRIBUTOR_RID}/patches"),
            Some(Body::from(body)),
            Some(SESSION_ID.to_string()),
        )
        .await;

        assert_eq!(response.status(), StatusCode::CREATED);
        assert_eq!(
            response.json().await,
            json!(
              {
                "success": true,
                "id": CREATED_PATCH_ID,
              }
            )
        );

        let response = get(
            &app,
            format!("/projects/{CONTRIBUTOR_RID}/patches/{CREATED_PATCH_ID}"),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.json().await,
            json!(
              {
                "id": CREATED_PATCH_ID,
                "author": {
                  "id": CONTRIBUTOR_DID
                },
                "title": "Update README",
                "state": { "status": "open" },
                "target": "delegates",
                "tags": [],
                "merges": [],
                "reviewers": [],
                "revisions": [
                  {
                    "id": CREATED_PATCH_ID,
                    "description": "Do some changes to README",
                    "base": INITIAL_COMMIT,
                    "oid": HEAD,
                    "refs": [
                      "refs/heads/master",
                    ],
                    "discussions": [],
                    "timestamp": TIMESTAMP,
                    "reviews": [],
                  }
                ],
              }
            )
        );
    }

    #[tokio::test]
    async fn test_projects_patches_tag() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = contributor(tmp.path());
        let app = super::router(ctx.to_owned());
        create_session(ctx).await;
        let body = serde_json::to_vec(&json!({
          "type": "tag",
          "add": ["bug","design"],
          "remove": []
        }))
        .unwrap();
        let response = patch(
            &app,
            format!("/projects/{CONTRIBUTOR_RID}/patches/{CONTRIBUTOR_PATCH_ID}"),
            Some(Body::from(body)),
            Some(SESSION_ID.to_string()),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);

        let response = get(
            &app,
            format!("/projects/{CONTRIBUTOR_RID}/patches/{CONTRIBUTOR_PATCH_ID}"),
        )
        .await;

        assert_eq!(
            response.json().await,
            json!({
              "id": CONTRIBUTOR_PATCH_ID,
              "author": {
                "id": CONTRIBUTOR_DID,
              },
              "title": "A new `hello world`",
              "state": { "status": "open" },
              "target": "delegates",
              "tags": [
                "bug",
                "design"
              ],
              "merges": [],
              "reviewers": [],
              "revisions": [
                {
                  "id": CONTRIBUTOR_PATCH_ID,
                  "description": "change `hello world` in README to something else",
                  "base": PARENT,
                  "oid": HEAD,
                  "refs": [
                    "refs/heads/master",
                  ],
                  "discussions": [],
                  "timestamp": TIMESTAMP,
                  "reviews": [],
                },
              ],
            })
        );
    }

    #[tokio::test]
    async fn test_projects_patches_revisions() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = contributor(tmp.path());
        let app = super::router(ctx.to_owned());
        create_session(ctx).await;
        let body = serde_json::to_vec(&json!({
          "type": "revision",
          "description": "This is a new revision",
          "base": PARENT,
          "oid": HEAD,
        }))
        .unwrap();
        let response = patch(
            &app,
            format!("/projects/{CONTRIBUTOR_RID}/patches/{CONTRIBUTOR_PATCH_ID}"),
            Some(Body::from(body)),
            Some(SESSION_ID.to_string()),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);

        let response = get(
            &app,
            format!("/projects/{CONTRIBUTOR_RID}/patches/{CONTRIBUTOR_PATCH_ID}"),
        )
        .await;

        assert_eq!(
            response.json().await,
            json!({
              "id": CONTRIBUTOR_PATCH_ID,
              "author": {
                "id": CONTRIBUTOR_DID,
              },
              "title": "A new `hello world`",
              "state": { "status": "open" },
              "target": "delegates",
              "tags": [],
              "merges": [],
              "reviewers": [],
              "revisions": [
                {
                  "id": CONTRIBUTOR_PATCH_ID,
                  "description": "change `hello world` in README to something else",
                  "base": PARENT,
                  "oid": HEAD,
                  "refs": [
                    "refs/heads/master",
                  ],
                  "discussions": [],
                  "timestamp": TIMESTAMP,
                  "reviews": [],
                },
                {
                  "id": "181e4219bc132e7716126a84200d4dbd628dd6be",
                  "description": "This is a new revision",
                  "base": PARENT,
                  "oid": HEAD,
                  "refs": [
                    "refs/heads/master",
                  ],
                  "discussions": [],
                  "timestamp": TIMESTAMP,
                  "reviews": [],
                }
              ],
            })
        );
    }

    #[tokio::test]
    async fn test_projects_patches_edit() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = contributor(tmp.path());
        let app = super::router(ctx.to_owned());
        create_session(ctx).await;
        let body = serde_json::to_vec(&json!({
          "type": "edit",
          "title": "This is a updated title",
          "description": "Let's write some description",
          "target": "delegates",
        }))
        .unwrap();
        let response = patch(
            &app,
            format!("/projects/{CONTRIBUTOR_RID}/patches/{CONTRIBUTOR_PATCH_ID}"),
            Some(Body::from(body)),
            Some(SESSION_ID.to_string()),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);

        let response = get(
            &app,
            format!("/projects/{CONTRIBUTOR_RID}/patches/{CONTRIBUTOR_PATCH_ID}"),
        )
        .await;

        assert_eq!(
            response.json().await,
            json!({
              "id": CONTRIBUTOR_PATCH_ID,
              "author": {
                "id": CONTRIBUTOR_DID,
              },
              "title": "This is a updated title",
              "state": { "status": "open" },
              "target": "delegates",
              "tags": [],
              "merges": [],
              "reviewers": [],
              "revisions": [
                {
                  "id": CONTRIBUTOR_PATCH_ID,
                  "description": "change `hello world` in README to something else",
                  "base": PARENT,
                  "oid": HEAD,
                  "refs": [
                    "refs/heads/master",
                  ],
                  "discussions": [],
                  "timestamp": TIMESTAMP,
                  "reviews": [],
                },
              ],
            })
        );
    }

    #[tokio::test]
    async fn test_projects_patches_discussions() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = contributor(tmp.path());
        let app = super::router(ctx.to_owned());
        create_session(ctx).await;
        let thread_body = serde_json::to_vec(&json!({
          "type": "thread",
          "revision": CONTRIBUTOR_PATCH_ID,
          "action": {
            "type": "comment",
            "body": "This is a root level comment"
          }
        }))
        .unwrap();
        let response = patch(
            &app,
            format!("/projects/{CONTRIBUTOR_RID}/patches/{CONTRIBUTOR_PATCH_ID}"),
            Some(Body::from(thread_body)),
            Some(SESSION_ID.to_string()),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);

        let reply_body = serde_json::to_vec(&json!({
          "type": "thread",
          "revision": CONTRIBUTOR_PATCH_ID,
          "action": {
            "type": "comment",
            "body": "This is a root level comment",
            "replyTo": CONTRIBUTOR_COMMENT_1,
          }
        }))
        .unwrap();
        let response = patch(
            &app,
            format!("/projects/{CONTRIBUTOR_RID}/patches/{CONTRIBUTOR_PATCH_ID}"),
            Some(Body::from(reply_body)),
            Some(SESSION_ID.to_string()),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);

        let response = get(
            &app,
            format!("/projects/{CONTRIBUTOR_RID}/patches/{CONTRIBUTOR_PATCH_ID}"),
        )
        .await;

        assert_eq!(
            response.json().await,
            json!({
              "id": CONTRIBUTOR_PATCH_ID,
              "author": {
                "id": CONTRIBUTOR_DID,
              },
              "title": "A new `hello world`",
              "state": { "status": "open" },
              "target": "delegates",
              "tags": [],
              "merges": [],
              "reviewers": [],
              "revisions": [
                {
                  "id": CONTRIBUTOR_PATCH_ID,
                  "description": "change `hello world` in README to something else",
                  "base": PARENT,
                  "oid": HEAD,
                  "refs": [
                    "refs/heads/master",
                  ],
                  "discussions": [
                    {
                      "id": CONTRIBUTOR_COMMENT_1,
                      "author": {
                        "id": CONTRIBUTOR_DID,
                      },
                      "body": "This is a root level comment",
                      "reactions": [],
                      "timestamp": TIMESTAMP,
                      "replyTo": null,
                    },
                    {
                      "id": CONTRIBUTOR_COMMENT_2,
                      "author": {
                        "id": CONTRIBUTOR_DID,
                      },
                      "body": "This is a root level comment",
                      "reactions": [],
                      "timestamp": TIMESTAMP,
                      "replyTo": CONTRIBUTOR_COMMENT_1,
                    },
                  ],
                  "timestamp": TIMESTAMP,
                  "reviews": [],
                },
              ],
            })
        );
    }

    #[tokio::test]
    async fn test_projects_patches_reviews() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = contributor(tmp.path());
        let app = super::router(ctx.to_owned());
        create_session(ctx).await;
        let thread_body = serde_json::to_vec(&json!({
          "type": "review",
          "revision": CONTRIBUTOR_PATCH_ID,
          "comment": "A small review",
          "verdict": "accept",
          "inline": [
            {
              "location": {
                "blob": "82eb77880c693655bce074e3dbbd9fa711dc018b",
                "path": "./README.md",
                "commit": HEAD,
                "lines": {
                    "start": 1,
                    "end": 3,
                },
              },
              "comment": "This is a comment on line 1",
              "timestamp": TIMESTAMP,
            }
          ]
        }))
        .unwrap();
        let response = patch(
            &app,
            format!("/projects/{CONTRIBUTOR_RID}/patches/{CONTRIBUTOR_PATCH_ID}"),
            Some(Body::from(thread_body)),
            Some(SESSION_ID.to_string()),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);

        let response = get(
            &app,
            format!("/projects/{CONTRIBUTOR_RID}/patches/{CONTRIBUTOR_PATCH_ID}"),
        )
        .await;

        assert_eq!(
            response.json().await,
            json!({
              "id": CONTRIBUTOR_PATCH_ID,
              "author": {
                "id": CONTRIBUTOR_DID,
              },
              "title": "A new `hello world`",
              "state": { "status": "open" },
              "target": "delegates",
              "tags": [],
              "merges": [],
              "reviewers": [],
              "revisions": [
                {
                  "id": CONTRIBUTOR_PATCH_ID,
                  "description": "change `hello world` in README to something else",
                  "base": PARENT,
                  "oid": HEAD,
                  "refs": [
                    "refs/heads/master",
                  ],
                  "discussions": [],
                  "timestamp": TIMESTAMP,
                  "reviews": [
                    [
                      CONTRIBUTOR_NID,
                      {
                        "verdict": "accept",
                        "comment": "A small review",
                        "inline": [
                          {
                            "location": {
                              "blob": "82eb77880c693655bce074e3dbbd9fa711dc018b",
                              "path": "./README.md",
                              "commit": HEAD,
                              "lines": {
                                "start": 1,
                                "end": 3,
                              },
                            },
                            "comment": "This is a comment on line 1",
                            "timestamp": TIMESTAMP,
                          }
                        ],
                        "timestamp": TIMESTAMP,
                      },
                    ],
                  ],
                },
              ],
            })
        );
    }

    #[tokio::test]
    async fn test_projects_patches_merges() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = contributor(tmp.path());
        let app = super::router(ctx.to_owned());
        create_session(ctx).await;
        let thread_body = serde_json::to_vec(&json!({
          "type": "merge",
          "revision": CONTRIBUTOR_PATCH_ID,
          "commit": PARENT,
        }))
        .unwrap();
        let response = patch(
            &app,
            format!("/projects/{CONTRIBUTOR_RID}/patches/{CONTRIBUTOR_PATCH_ID}"),
            Some(Body::from(thread_body)),
            Some(SESSION_ID.to_string()),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);

        let response = get(
            &app,
            format!("/projects/{CONTRIBUTOR_RID}/patches/{CONTRIBUTOR_PATCH_ID}"),
        )
        .await;

        assert_eq!(
            response.json().await,
            json!({
              "id": CONTRIBUTOR_PATCH_ID,
              "author": {
                "id": CONTRIBUTOR_DID,
              },
              "title": "A new `hello world`",
              "state": {
                  "status": "merged",
                  "revision": CONTRIBUTOR_PATCH_ID,
                  "commit": PARENT,
              },
              "target": "delegates",
              "tags": [],
              "merges": [{
                  "author": CONTRIBUTOR_NID,
                  "revision": CONTRIBUTOR_PATCH_ID,
                  "commit": PARENT,
                  "timestamp": TIMESTAMP,
              }],
              "reviewers": [],
              "revisions": [
                {
                  "id": CONTRIBUTOR_PATCH_ID,
                  "description": "change `hello world` in README to something else",
                  "base": PARENT,
                  "oid": HEAD,
                  "refs": [
                    "refs/heads/master",
                  ],
                  "discussions": [],
                  "timestamp": TIMESTAMP,
                  "reviews": [],
                },
              ],
            })
        );
    }
}

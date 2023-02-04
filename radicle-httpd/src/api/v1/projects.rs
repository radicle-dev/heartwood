use std::collections::BTreeMap;

use axum::extract::State;
use axum::handler::Handler;
use axum::http::{header, HeaderValue};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use hyper::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tower_http::set_header::SetResponseHeaderLayer;

use radicle::cob::issue::Issues;
use radicle::cob::patch::Patches;
use radicle::cob::thread::{self, CommentId};
use radicle::cob::Timestamp;
use radicle::identity::{Id, PublicKey};
use radicle::node::NodeId;
use radicle::storage::{git::paths, ReadRepository, WriteStorage};
use radicle_surf::{Glob, Oid, Repository};

use crate::api::axum_extra::{Path, Query};
use crate::api::error::Error;
use crate::api::project::Info;
use crate::api::{self, Context, PaginationQuery};

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
        .route("/projects/:project/tree/:sha/", get(tree_handler_root))
        .route("/projects/:project/tree/:sha/*path", get(tree_handler))
        .route("/projects/:project/remotes", get(remotes_handler))
        .route("/projects/:project/remotes/:peer", get(remote_handler))
        .route("/projects/:project/blob/:sha/*path", get(blob_handler))
        .route("/projects/:project/readme/:sha", get(readme_handler))
        .route("/projects/:project/issues", get(issues_handler))
        .route("/projects/:project/issues/:id", get(issue_handler))
        .route("/projects/:project/patches", get(patches_handler))
        .route("/projects/:project/patches/:id", get(patch_handler))
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
async fn project_handler(State(ctx): State<Context>, Path(id): Path<Id>) -> impl IntoResponse {
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

    let (sha, fallback_to_head) = match parent {
        Some(commit) => (commit, false),
        None => {
            let info = ctx.project_info(project)?;

            (info.head.to_string(), true)
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

    if fallback_to_head {
        return Ok::<_, Error>((StatusCode::FOUND, Json(response)));
    }

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

            json!({
                "id": remote.id,
                "heads": refs,
                "delegate": remote.delegate,
            })
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
        "delegate": remote.delegate,
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
    State(ctx): State<Context>,
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

/// Get project patches list.
/// `GET /projects/:project/patches`
async fn patches_handler(
    State(ctx): State<Context>,
    Path(project): Path<Id>,
    Query(qs): Query<PaginationQuery>,
) -> impl IntoResponse {
    let PaginationQuery { page, per_page } = qs;
    let page = page.unwrap_or(0);
    let per_page = per_page.unwrap_or(10);
    let storage = &ctx.profile.storage;
    let repo = storage.repository(project)?;
    let patches = Patches::open(ctx.profile.public_key, &repo)?;
    let patches = patches
        .all()?
        .into_iter()
        .filter_map(|r| r.ok())
        .map(|(id, patch, _)| api::json::patch(id, patch))
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
    let patch = Patches::open(ctx.profile.public_key, &repo)?
        .get(&patch_id.into())?
        .ok_or(Error::NotFound)?;

    Ok::<_, Error>(Json(api::json::patch(patch_id.into(), patch)))
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

        for (_, comment) in iter {
            comments.push(Comment {
                author: Author {
                    id: comment.author(),
                },
                body: comment.body().to_owned(),
                reactions: [],
                timestamp: comment.timestamp(),
                reply_to: comment.reply_to(),
            });
        }

        Comments(comments)
    }
}

#[cfg(test)]
mod routes {
    use axum::http::StatusCode;
    use serde_json::json;

    use crate::api::test::{self, get, HEAD, HEAD_1};

    #[tokio::test]
    async fn test_projects_root() {
        let tmp = tempfile::tempdir().unwrap();
        let app = super::router(test::seed(tmp.path()));
        let response = get(&app, "/projects").await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.json().await,
            json!([
              {
                "name": "hello-world",
                "description": "Rad repository for tests",
                "defaultBranch": "master",
                "head": HEAD,
                "patches": 0,
                "issues": 1,
                "id": "rad:z4FucBZHZMCsxTyQE1dfE2YR59Qbp"
              }
            ])
        );
    }

    #[tokio::test]
    async fn test_projects() {
        let tmp = tempfile::tempdir().unwrap();
        let app = super::router(test::seed(tmp.path()));
        let response = get(&app, "/projects/rad:z4FucBZHZMCsxTyQE1dfE2YR59Qbp").await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.json().await,
            json!({
               "name": "hello-world",
               "description": "Rad repository for tests",
               "defaultBranch": "master",
               "head": HEAD,
               "patches": 0,
               "issues": 1,
               "id": "rad:z4FucBZHZMCsxTyQE1dfE2YR59Qbp"
            })
        );
    }

    #[tokio::test]
    async fn test_projects_commits_root() {
        let tmp = tempfile::tempdir().unwrap();
        let app = super::router(test::seed(tmp.path()));
        let response = get(&app, "/projects/rad:z4FucBZHZMCsxTyQE1dfE2YR59Qbp/commits").await;

        assert_eq!(response.status(), StatusCode::FOUND);
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
                      "time": 1673001014
                    },
                  },
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
                                  "type": "addition"
                                }
                              ]
                            }
                          ]
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
                },
                {
                  "commit": {
                    "id": HEAD_1,
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
                          ]
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
                "commits": 2,
                "branches": 1,
                "contributors": 1
              }

            })
        );
    }

    #[tokio::test]
    async fn test_projects_commits() {
        let tmp = tempfile::tempdir().unwrap();
        let app = super::router(test::seed(tmp.path()));
        let response = get(
            &app,
            format!("/projects/rad:z4FucBZHZMCsxTyQE1dfE2YR59Qbp/commits/{HEAD}"),
        )
        .await;

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
                  "time": 1673001014
                },
              },
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
                              "type": "addition"
                            }
                          ]
                        }
                      ]
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
            })
        );
    }

    #[tokio::test]
    async fn test_projects_tree() {
        let tmp = tempfile::tempdir().unwrap();
        let app = super::router(test::seed(tmp.path()));
        let response = get(
            &app,
            format!("/projects/rad:z4FucBZHZMCsxTyQE1dfE2YR59Qbp/tree/{HEAD}/"),
        )
        .await;

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
                    "time": 1673001014
                  },
                },
                "name": "",
                "path": "",
                "stats": {
                  "branches": 1,
                  "commits": 2,
                  "contributors": 1
                }
              }
            )
        );

        let response = get(
            &app,
            format!("/projects/rad:z4FucBZHZMCsxTyQE1dfE2YR59Qbp/tree/{HEAD}/dir1"),
        )
        .await;

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
                  "time": 1673001014
                },
              },
              "name": "dir1",
              "path": "dir1",
              "stats": {
                "branches": 1,
                "commits": 2,
                "contributors": 1
              }
            })
        );
    }

    #[tokio::test]
    async fn test_projects_remotes_root() {
        let tmp = tempfile::tempdir().unwrap();
        let app = super::router(test::seed(tmp.path()));
        let response = get(&app, "/projects/rad:z4FucBZHZMCsxTyQE1dfE2YR59Qbp/remotes").await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.json().await,
            json!([
              {
                "id": "z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi",
                "heads": {
                  "master": HEAD
                },
                "delegate": false
              }
            ])
        );
    }

    #[tokio::test]
    async fn test_projects_remotes() {
        let tmp = tempfile::tempdir().unwrap();
        let app = super::router(test::seed(tmp.path()));
        let response = get(&app, "/projects/rad:z4FucBZHZMCsxTyQE1dfE2YR59Qbp/remotes/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi").await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.json().await,
            json!({
                "id": "z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi",
                "heads": {
                    "master": HEAD
                },
                "delegate": false
            })
        );
    }

    #[tokio::test]
    async fn test_projects_blob() {
        let tmp = tempfile::tempdir().unwrap();
        let app = super::router(test::seed(tmp.path()));
        let response = get(
            &app,
            format!("/projects/rad:z4FucBZHZMCsxTyQE1dfE2YR59Qbp/blob/{HEAD}/README"),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.json().await,
            json!({
                "binary": false,
                "content": "Hello World!\n",
                "lastCommit": {
                    "id": HEAD_1,
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
    async fn test_projects_readme() {
        let tmp = tempfile::tempdir().unwrap();
        let app = super::router(test::seed(tmp.path()));
        let response = get(
            &app,
            format!("/projects/rad:z4FucBZHZMCsxTyQE1dfE2YR59Qbp/readme/{HEAD}"),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.json().await,
            json!({
                "binary": false,
                "content": "Hello World!\n",
                "lastCommit": {
                    "id": HEAD_1,
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
    async fn test_projects_issues_root() {
        let tmp = tempfile::tempdir().unwrap();
        let app = super::router(test::seed(tmp.path()));
        let response = get(&app, "/projects/rad:z4FucBZHZMCsxTyQE1dfE2YR59Qbp/issues").await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.json().await,
            json!([
              {
                "id": "458bbd9f6d47eed3d60cd905141687ad1f99251e",
                "author": {
                    "id": "z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi"
                },
                "title": "Issue #1",
                "state": {
                    "status": "open"
                },
                "discussion": [
                  {
                    "author": {
                        "id": "z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi"
                    },
                    "body": "Change 'hello world' to 'hello everyone'",
                    "reactions": [],
                    "timestamp": 1673001014,
                    "replyTo": null
                  }
                ],
                "tags": []
              }
            ])
        );
    }

    #[tokio::test]
    async fn test_projects_patches() {
        let tmp = tempfile::tempdir().unwrap();
        let app = super::router(test::seed(tmp.path()));
        let response = get(&app, "/projects/rad:z4FucBZHZMCsxTyQE1dfE2YR59Qbp/patches").await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.json().await,
            json!([
              {
                "id": "5de9f17ca5326258412ab02f9a5339b6482198ce",
                "author": {
                    "id": "z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi"
                },
                "title": "A new `hello word`",
                "description": "change `hello world` in README to something else",
                "state": "proposed",
                "target": "delegates",
                "tags": [],
                "revisions": [
                    {
                        "id": "z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/1",
                        "description": "",
                        "reviews": [],
                    }
                ],
              }
            ])
        );

        let response = get(
            &app,
            "/projects/rad:z4FucBZHZMCsxTyQE1dfE2YR59Qbp/patches/5de9f17ca5326258412ab02f9a5339b6482198ce",
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.json().await,
            json!(
              {
                "id": "5de9f17ca5326258412ab02f9a5339b6482198ce",
                "author": {
                    "id": "z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi"
                },
                "title": "A new `hello word`",
                "description": "change `hello world` in README to something else",
                "state": "proposed",
                "target": "delegates",
                "tags": [],
                "revisions": [
                    {
                        "id": "z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/1",
                        "description": "",
                        "reviews": [],
                    }
                ],
              }
            )
        );
    }
}

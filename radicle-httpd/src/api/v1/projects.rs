use std::collections::{BTreeMap, HashMap};

use axum::extract::{DefaultBodyLimit, State};
use axum::handler::Handler;
use axum::http::{header, HeaderValue};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use axum_auth::AuthBearer;
use hyper::StatusCode;
use radicle_surf::blob::BlobRef;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tower_http::set_header::SetResponseHeaderLayer;

use radicle::cob::{
    issue, issue::cache::Issues as _, patch, patch::cache::Patches as _, resolve_embed, Embed,
    Label, Uri,
};
use radicle::identity::{Did, RepoId};
use radicle::node::routing::Store;
use radicle::node::{AliasStore, Node, NodeId};
use radicle::storage::{ReadRepository, ReadStorage, RemoteRepository, WriteRepository};
use radicle_surf::{diff, Glob, Oid, Repository};

use crate::api::error::Error;
use crate::api::project::Info;
use crate::api::{self, announce_refs, CobsQuery, Context, PaginationQuery, ProjectQuery};
use crate::axum_extra::{immutable_response, Path, Query};

const CACHE_1_HOUR: &str = "public, max-age=3600, must-revalidate";
const MAX_BODY_LIMIT: usize = 4_194_304;

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
                    |response: &Response| {
                        response
                            .status()
                            .is_success()
                            .then_some(HeaderValue::from_static(CACHE_1_HOUR))
                    },
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
            patch(patch_update_handler).get(patch_handler),
        )
        .with_state(ctx)
        .layer(DefaultBodyLimit::max(MAX_BODY_LIMIT))
}

/// List all projects.
/// `GET /projects`
async fn project_root_handler(
    State(ctx): State<Context>,
    Query(qs): Query<PaginationQuery>,
) -> impl IntoResponse {
    let PaginationQuery {
        show,
        page,
        per_page,
    } = qs;
    let page = page.unwrap_or(0);
    let per_page = per_page.unwrap_or(10);
    let storage = &ctx.profile.storage;
    let db = &ctx.profile.database()?;
    let pinned = &ctx.profile.config.web.pinned;
    let policies = ctx.profile.policies()?;

    let mut projects = match show {
        ProjectQuery::All => storage
            .repositories()?
            .into_iter()
            .filter(|repo| repo.doc.visibility.is_public())
            .collect::<Vec<_>>(),
        ProjectQuery::Pinned => storage
            .repositories_by_id(pinned.repositories.iter())?
            .into_iter()
            .filter(|repo| repo.doc.visibility.is_public())
            .collect::<Vec<_>>(),
    };
    projects.sort_by_key(|p| p.rid);

    let infos = projects
        .into_iter()
        .filter_map(|info| {
            if !policies.is_seeding(&info.rid).unwrap_or_default() {
                return None;
            }
            let Ok(repo) = storage.repository(info.rid) else {
                return None;
            };
            let Ok((_, head)) = repo.head() else {
                return None;
            };
            let Ok(payload) = info.doc.project() else {
                return None;
            };
            let Ok(issues) = ctx.profile.issues(&repo) else {
                return None;
            };
            let Ok(issues) = issues.counts() else {
                return None;
            };
            let Ok(patches) = ctx.profile.patches(&repo) else {
                return None;
            };
            let Ok(patches) = patches.counts() else {
                return None;
            };
            let delegates = info.doc.delegates;
            let seeding = db.count(&info.rid).unwrap_or_default();

            Some(Info {
                payload,
                delegates,
                head,
                visibility: info.doc.visibility,
                issues,
                patches,
                id: info.rid,
                seeding,
            })
        })
        .skip(page * per_page)
        .take(per_page)
        .collect::<Vec<_>>();

    Ok::<_, Error>(Json(infos))
}

/// Get project metadata.
/// `GET /projects/:project`
async fn project_handler(State(ctx): State<Context>, Path(rid): Path<RepoId>) -> impl IntoResponse {
    let (repo, doc) = ctx.repo(rid)?;
    let info = ctx.project_info(&repo, doc)?;

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
/// `GET /projects/:project/commits?parent=<sha>`
async fn history_handler(
    State(ctx): State<Context>,
    Path(rid): Path<RepoId>,
    Query(qs): Query<CommitsQueryString>,
) -> impl IntoResponse {
    let (repo, doc) = ctx.repo(rid)?;
    let CommitsQueryString {
        since,
        until,
        parent,
        page,
        per_page,
    } = qs;

    // If the parent commit is provided, the response depends only on the query
    // string and not on the state of the repository. This means we can instruct
    // the caches to treat the response as immutable.
    let is_immutable = parent.is_some();

    let sha = match parent {
        Some(commit) => commit,
        None => ctx.project_info(&repo, doc)?.head.to_string(),
    };
    let repo = Repository::open(repo.path())?;

    // If a pagination is defined, we do not want to paginate the commits, and we return all of them on the first page.
    let page = page.unwrap_or(0);
    let per_page = if per_page.is_none() && (since.is_some() || until.is_some()) {
        usize::MAX
    } else {
        per_page.unwrap_or(30)
    };

    let commits = repo
        .history(&sha)?
        .filter_map(|commit| {
            let commit = commit.ok()?;
            let time = commit.committer.time.seconds();
            let commit = api::json::commit(&commit);
            match (since, until) {
                (Some(since), Some(until)) if time >= since && time < until => Some(commit),
                (Some(since), None) if time >= since => Some(commit),
                (None, Some(until)) if time < until => Some(commit),
                (None, None) => Some(commit),
                _ => None,
            }
        })
        .skip(page * per_page)
        .take(per_page)
        .collect::<Vec<_>>();

    let response = json!({
        "commits": commits,
        "stats":  repo.stats_from(&sha)?,
    });

    if is_immutable {
        Ok::<_, Error>(immutable_response(response).into_response())
    } else {
        Ok::<_, Error>(Json(response).into_response())
    }
}

/// Get project commit.
/// `GET /projects/:project/commits/:sha`
async fn commit_handler(
    State(ctx): State<Context>,
    Path((project, sha)): Path<(RepoId, Oid)>,
) -> impl IntoResponse {
    let (repo, _) = ctx.repo(project)?;
    let repo = Repository::open(repo.path())?;
    let commit = repo.commit(sha)?;

    let diff = repo.diff_commit(commit.id)?;
    let glob = Glob::all_heads().branches().and(Glob::all_remotes());
    let branches: Vec<String> = repo
        .revision_branches(commit.id, glob)?
        .iter()
        .map(|b| b.refname().to_string())
        .collect();

    let mut files: HashMap<Oid, BlobRef<'_>> = HashMap::new();
    diff.files().for_each(|file_diff| match file_diff {
        diff::FileDiff::Added(added) => {
            if let Ok(blob) = repo.blob_ref(added.new.oid) {
                files.insert(blob.id(), blob);
            }
        }
        diff::FileDiff::Deleted(deleted) => {
            if let Ok(old_blob) = repo.blob_ref(deleted.old.oid) {
                files.insert(old_blob.id(), old_blob);
            }
        }
        diff::FileDiff::Modified(modified) => {
            if let (Ok(old_blob), Ok(new_blob)) = (
                repo.blob_ref(modified.old.oid),
                repo.blob_ref(modified.new.oid),
            ) {
                files.insert(old_blob.id(), old_blob);
                files.insert(new_blob.id(), new_blob);
            }
        }
        diff::FileDiff::Moved(moved) => {
            if let (Ok(old_blob), Ok(new_blob)) =
                (repo.blob_ref(moved.old.oid), repo.blob_ref(moved.new.oid))
            {
                files.insert(old_blob.id(), old_blob);
                files.insert(new_blob.id(), new_blob);
            }
        }
        diff::FileDiff::Copied(copied) => {
            if let (Ok(old_blob), Ok(new_blob)) =
                (repo.blob_ref(copied.old.oid), repo.blob_ref(copied.new.oid))
            {
                files.insert(old_blob.id(), old_blob);
                files.insert(new_blob.id(), new_blob);
            }
        }
    });

    let response: serde_json::Value = json!({
      "commit": api::json::commit(&commit),
      "diff": diff,
      "files": files,
      "branches": branches
    });
    Ok::<_, Error>(immutable_response(response))
}

/// Get diff between two commits
/// `GET /projects/:project/diff/:base/:oid`
async fn diff_handler(
    State(ctx): State<Context>,
    Path((project, base, oid)): Path<(RepoId, Oid, Oid)>,
) -> impl IntoResponse {
    let (repo, _) = ctx.repo(project)?;
    let repo = Repository::open(repo.path())?;
    let base = repo.commit(base)?;
    let commit = repo.commit(oid)?;
    let diff = repo.diff(base.id, commit.id)?;
    let mut files: HashMap<Oid, BlobRef<'_>> = HashMap::new();
    diff.files().for_each(|file_diff| match file_diff {
        diff::FileDiff::Added(added) => {
            if let Ok(new_blob) = repo.blob_ref(added.new.oid) {
                files.insert(new_blob.id(), new_blob);
            }
        }
        diff::FileDiff::Deleted(deleted) => {
            if let Ok(old_blob) = repo.blob_ref(deleted.old.oid) {
                files.insert(old_blob.id(), old_blob);
            }
        }
        diff::FileDiff::Modified(modified) => {
            if let (Ok(new_blob), Ok(old_blob)) = (
                repo.blob_ref(modified.old.oid),
                repo.blob_ref(modified.new.oid),
            ) {
                files.insert(new_blob.id(), new_blob);
                files.insert(old_blob.id(), old_blob);
            }
        }
        diff::FileDiff::Moved(moved) => {
            if let (Ok(new_blob), Ok(old_blob)) =
                (repo.blob_ref(moved.new.oid), repo.blob_ref(moved.old.oid))
            {
                files.insert(new_blob.id(), new_blob);
                files.insert(old_blob.id(), old_blob);
            }
        }
        diff::FileDiff::Copied(copied) => {
            if let (Ok(new_blob), Ok(old_blob)) =
                (repo.blob_ref(copied.new.oid), repo.blob_ref(copied.old.oid))
            {
                files.insert(new_blob.id(), new_blob);
                files.insert(old_blob.id(), old_blob);
            }
        }
    });

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

    let response = json!({ "diff": diff, "files": files, "commits": commits });

    Ok::<_, Error>(immutable_response(response))
}

/// Get project activity for the past year.
/// `GET /projects/:project/activity`
async fn activity_handler(
    State(ctx): State<Context>,
    Path(project): Path<RepoId>,
) -> impl IntoResponse {
    let (repo, _) = ctx.repo(project)?;
    let current_date = chrono::Utc::now().timestamp();
    // SAFETY: The number of weeks is static and not out of bounds.
    #[allow(clippy::unwrap_used)]
    let one_year_ago = chrono::Duration::try_weeks(52).unwrap();
    let repo = Repository::open(repo.path())?;
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
    Path((rid, sha)): Path<(RepoId, Oid)>,
) -> impl IntoResponse {
    tree_handler(State(ctx), Path((rid, sha, String::new()))).await
}

/// Get project source tree.
/// `GET /projects/:project/tree/:sha/*path`
async fn tree_handler(
    State(ctx): State<Context>,
    Path((project, sha, path)): Path<(RepoId, Oid, String)>,
) -> impl IntoResponse {
    let (repo, _) = ctx.repo(project)?;

    if let Some(ref cache) = ctx.cache {
        let cache = &mut cache.tree.lock().await;
        if let Some(response) = cache.get(&(project, sha, path.clone())) {
            return Ok::<_, Error>(immutable_response(response.clone()));
        }
    }

    let repo = Repository::open(repo.path())?;
    let tree = repo.tree(sha, &path)?;
    let stats = repo.stats_from(&sha)?;
    let response = api::json::tree(&tree, &path, &stats);

    if let Some(cache) = &ctx.cache {
        let cache = &mut cache.tree.lock().await;
        cache.put((project, sha, path.clone()), response.clone());
    }

    Ok::<_, Error>(immutable_response(response))
}

/// Get all project remotes.
/// `GET /projects/:project/remotes`
async fn remotes_handler(
    State(ctx): State<Context>,
    Path(project): Path<RepoId>,
) -> impl IntoResponse {
    let (repo, doc) = ctx.repo(project)?;
    let delegates = &doc.delegates;
    let aliases = &ctx.profile.aliases();
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

            match aliases.alias(&remote.id) {
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
    Path((project, node_id)): Path<(RepoId, NodeId)>,
) -> impl IntoResponse {
    let (repo, doc) = ctx.repo(project)?;
    let delegates = &doc.delegates;
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
    Path((project, sha, path)): Path<(RepoId, Oid, String)>,
) -> impl IntoResponse {
    let (repo, _) = ctx.repo(project)?;
    let repo = Repository::open(repo.path())?;
    let blob = repo.blob(sha, &path)?;
    let response = api::json::blob(&blob, &path);

    Ok::<_, Error>(Json(response))
}

/// Get project readme.
/// `GET /projects/:project/readme/:sha`
async fn readme_handler(
    State(ctx): State<Context>,
    Path((project, sha)): Path<(RepoId, Oid)>,
) -> impl IntoResponse {
    let (repo, _) = ctx.repo(project)?;
    let repo = Repository::open(repo.path())?;
    let paths = [
        "README",
        "README.md",
        "README.markdown",
        "README.txt",
        "README.rst",
        "Readme.md",
    ];

    for path in paths
        .iter()
        .map(ToString::to_string)
        .chain(paths.iter().map(|p| p.to_lowercase()))
    {
        if let Ok(blob) = repo.blob(sha, &path) {
            let response = api::json::blob(&blob, &path);
            return Ok::<_, Error>(immutable_response(response));
        }
    }

    Err(Error::NotFound)
}

/// Get project issues list.
/// `GET /projects/:project/issues`
async fn issues_handler(
    State(ctx): State<Context>,
    Path(project): Path<RepoId>,
    Query(qs): Query<CobsQuery<api::IssueState>>,
) -> impl IntoResponse {
    let (repo, _) = ctx.repo(project)?;
    let CobsQuery {
        page,
        per_page,
        state,
    } = qs;
    let page = page.unwrap_or(0);
    let per_page = per_page.unwrap_or(10);
    let state = state.unwrap_or_default();
    let issues = ctx.profile.issues(&repo)?;
    let mut issues: Vec<_> = issues
        .list()?
        .filter_map(|r| {
            let (id, issue) = r.ok()?;
            (state.matches(issue.state())).then_some((id, issue))
        })
        .collect::<Vec<_>>();

    issues.sort_by(|(_, a), (_, b)| b.timestamp().cmp(&a.timestamp()));
    let aliases = &ctx.profile.aliases();
    let issues = issues
        .into_iter()
        .map(|(id, issue)| api::json::issue(id, issue, aliases))
        .skip(page * per_page)
        .take(per_page)
        .collect::<Vec<_>>();

    Ok::<_, Error>(Json(issues))
}

#[derive(Debug, Deserialize, Serialize)]
pub struct IssueCreate {
    pub title: String,
    pub description: String,
    pub labels: Vec<Label>,
    pub assignees: Vec<Did>,
    pub embeds: Vec<Embed<Uri>>,
}

/// Create a new issue.
/// `POST /projects/:project/issues`
async fn issue_create_handler(
    State(ctx): State<Context>,
    AuthBearer(token): AuthBearer,
    Path(project): Path<RepoId>,
    Json(issue): Json<IssueCreate>,
) -> impl IntoResponse {
    api::auth::validate(&ctx, &token).await?;

    let (repo, _) = ctx.repo(project)?;
    let node = Node::new(ctx.profile.socket());
    let signer = ctx
        .profile
        .signer()
        .map_err(|_| Error::Auth("Unauthorized"))?;
    let embeds: Vec<Embed> = issue
        .embeds
        .into_iter()
        .filter_map(|embed| resolve_embed(&repo, embed))
        .collect();

    let mut issues = ctx.profile.issues_mut(&repo)?;
    let issue = issues
        .create(
            issue.title,
            issue.description,
            &issue.labels,
            &issue.assignees,
            embeds,
            &signer,
        )
        .map_err(Error::from)?;

    announce_refs(node, repo.id())?;

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
    Path((project, issue_id)): Path<(RepoId, Oid)>,
    Json(action): Json<issue::Action>,
) -> impl IntoResponse {
    api::auth::validate(&ctx, &token).await?;

    let (repo, _) = ctx.repo(project)?;
    let node = Node::new(ctx.profile.socket());
    let signer = ctx.profile.signer()?;
    let mut issues = ctx.profile.issues_mut(&repo)?;
    let mut issue = issues.get_mut(&issue_id.into())?;

    let id = match action {
        issue::Action::Assign { assignees } => issue.assign(assignees, &signer)?,
        issue::Action::Lifecycle { state } => issue.lifecycle(state, &signer)?,
        issue::Action::Label { labels } => issue.label(labels, &signer)?,
        issue::Action::Edit { title } => issue.edit(title, &signer)?,
        issue::Action::Comment {
            body,
            reply_to,
            embeds,
        } => {
            let embeds: Vec<Embed> = embeds
                .into_iter()
                .filter_map(|embed| resolve_embed(&repo, embed))
                .collect();
            if let Some(to) = reply_to {
                issue.comment(body, to, embeds, &signer)?
            } else {
                return Err(Error::BadRequest("`replyTo` missing".to_owned()));
            }
        }
        issue::Action::CommentReact {
            id,
            reaction,
            active,
        } => issue.react(id, reaction, active, &signer)?,
        issue::Action::CommentEdit { id, body, embeds } => {
            let embeds: Vec<Embed> = embeds
                .into_iter()
                .filter_map(|embed| resolve_embed(&repo, embed))
                .collect();
            issue.edit_comment(id, body, embeds, &signer)?
        }
        issue::Action::CommentRedact { id } => issue.redact_comment(id, &signer)?,
    };

    announce_refs(node, repo.id())?;

    Ok::<_, Error>(Json(json!({ "success": true, "id": id })))
}

/// Get project issue.
/// `GET /projects/:project/issues/:id`
async fn issue_handler(
    State(ctx): State<Context>,
    Path((project, issue_id)): Path<(RepoId, Oid)>,
) -> impl IntoResponse {
    let (repo, _) = ctx.repo(project)?;
    let issue = ctx
        .profile
        .issues(&repo)?
        .get(&issue_id.into())?
        .ok_or(Error::NotFound)?;
    let aliases = ctx.profile.aliases();

    Ok::<_, Error>(Json(api::json::issue(issue_id.into(), issue, &aliases)))
}

#[derive(Deserialize, Serialize)]
pub struct PatchCreate {
    pub title: String,
    pub description: String,
    pub target: Oid,
    pub oid: Oid,
    pub labels: Vec<Label>,
}

/// Create a new patch.
/// `POST /projects/:project/patches`
async fn patch_create_handler(
    State(ctx): State<Context>,
    AuthBearer(token): AuthBearer,
    Path(project): Path<RepoId>,
    Json(patch): Json<PatchCreate>,
) -> impl IntoResponse {
    api::auth::validate(&ctx, &token).await?;

    let node = Node::new(ctx.profile.socket());
    let signer = ctx
        .profile
        .signer()
        .map_err(|_| Error::Auth("Unauthorized"))?;
    let (repo, _) = ctx.repo(project)?;
    let mut patches = ctx.profile.patches_mut(&repo)?;
    let base_oid = repo.raw().merge_base(*patch.target, *patch.oid)?;

    let patch = patches
        .create(
            patch.title,
            patch.description,
            patch::MergeTarget::default(),
            base_oid,
            patch.oid,
            &patch.labels,
            &signer,
        )
        .map_err(Error::from)?;

    announce_refs(node, repo.id())?;

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
    Path((project, patch_id)): Path<(RepoId, Oid)>,
    Json(action): Json<patch::Action>,
) -> impl IntoResponse {
    api::auth::validate(&ctx, &token).await?;

    let node = Node::new(ctx.profile.socket());
    let signer = ctx
        .profile
        .signer()
        .map_err(|_| Error::Auth("Unauthorized"))?;
    let (repo, _) = ctx.repo(project)?;
    let mut patches = ctx.profile.patches_mut(&repo)?;
    let mut patch = patches.get_mut(&patch_id.into())?;
    let id = match action {
        patch::Action::Edit { title, target } => patch.edit(title, target, &signer)?,
        patch::Action::Label { labels } => patch.label(labels, &signer)?,
        patch::Action::Lifecycle { state } => patch.lifecycle(state, &signer)?,
        patch::Action::Assign { assignees } => patch.assign(assignees, &signer)?,
        patch::Action::Merge { revision, commit } => {
            // TODO: We should cleanup the stored copy at least.
            patch.merge(revision, commit, &signer)?.entry
        }
        patch::Action::Review {
            revision,
            summary,
            verdict,
            labels,
        } => *patch.review(revision, verdict, summary, labels, &signer)?,
        patch::Action::ReviewEdit {
            review,
            summary,
            verdict,
        } => patch.edit_review(review, summary, verdict, &signer)?,
        patch::Action::ReviewRedact { review } => patch.redact_review(review, &signer)?,
        patch::Action::ReviewComment {
            review,
            body,
            reply_to,
            location,
            embeds,
        } => {
            let embeds: Vec<Embed> = embeds
                .into_iter()
                .filter_map(|embed| resolve_embed(&repo, embed))
                .collect();
            patch.review_comment(review, body, location, reply_to, embeds, &signer)?
        }
        patch::Action::ReviewCommentEdit {
            review,
            comment,
            body,
            embeds,
        } => {
            let embeds: Vec<Embed> = embeds
                .into_iter()
                .filter_map(|embed| resolve_embed(&repo, embed))
                .collect();
            patch.edit_review_comment(review, comment, body, embeds, &signer)?
        }
        patch::Action::ReviewCommentReact {
            review,
            comment,
            reaction,
            active,
        } => patch.react_review_comment(review, comment, reaction, active, &signer)?,
        patch::Action::ReviewCommentRedact { review, comment } => {
            patch.redact_review_comment(review, comment, &signer)?
        }
        patch::Action::ReviewCommentResolve { review, comment } => {
            patch.resolve_review_comment(review, comment, &signer)?
        }
        patch::Action::ReviewCommentUnresolve { review, comment } => {
            patch.unresolve_review_comment(review, comment, &signer)?
        }
        patch::Action::Revision {
            description,
            base,
            oid,
            ..
        } => patch.update(description, base, oid, &signer)?.into(),
        patch::Action::RevisionEdit {
            revision,
            description,
            embeds,
        } => {
            let embeds: Vec<Embed> = embeds
                .into_iter()
                .filter_map(|embed| resolve_embed(&repo, embed))
                .collect();
            patch.edit_revision(revision, description, embeds, &signer)?
        }
        patch::Action::RevisionRedact { revision } => patch.redact(revision, &signer)?,
        patch::Action::RevisionReact {
            revision,
            reaction,
            active,
            location,
        } => patch.react(revision, reaction, location, active, &signer)?,
        patch::Action::RevisionComment {
            revision,
            body,
            reply_to,
            location,
            embeds,
        } => {
            let embeds: Vec<Embed> = embeds
                .into_iter()
                .filter_map(|embed| resolve_embed(&repo, embed))
                .collect();
            patch.comment(revision, body, reply_to, location, embeds, &signer)?
        }
        patch::Action::RevisionCommentEdit {
            revision,
            comment,
            body,
            embeds,
        } => {
            let embeds: Vec<Embed> = embeds
                .into_iter()
                .filter_map(|embed| resolve_embed(&repo, embed))
                .collect();
            patch.comment_edit(revision, comment, body, embeds, &signer)?
        }
        patch::Action::RevisionCommentReact {
            revision,
            comment,
            reaction,
            active,
        } => patch.comment_react(revision, comment, reaction, active, &signer)?,
        patch::Action::RevisionCommentRedact { revision, comment } => {
            patch.comment_redact(revision, comment, &signer)?
        }
    };

    announce_refs(node, repo.id())?;

    Ok::<_, Error>(Json(json!({ "success": true, "id": id })))
}

/// Get project patches list.
/// `GET /projects/:project/patches`
async fn patches_handler(
    State(ctx): State<Context>,
    Path(rid): Path<RepoId>,
    Query(qs): Query<CobsQuery<api::PatchState>>,
) -> impl IntoResponse {
    let (repo, _) = ctx.repo(rid)?;
    let CobsQuery {
        page,
        per_page,
        state,
    } = qs;
    let page = page.unwrap_or(0);
    let per_page = per_page.unwrap_or(10);
    let state = state.unwrap_or_default();
    let patches = ctx.profile.patches(&repo)?;
    let mut patches = patches
        .list()?
        .filter_map(|r| {
            let (id, patch) = r.ok()?;
            (state.matches(patch.state())).then_some((id, patch))
        })
        .collect::<Vec<_>>();
    patches.sort_by(|(_, a), (_, b)| b.timestamp().cmp(&a.timestamp()));
    let aliases = ctx.profile.aliases();
    let patches = patches
        .into_iter()
        .map(|(id, patch)| api::json::patch(id, patch, &repo, &aliases))
        .skip(page * per_page)
        .take(per_page)
        .collect::<Vec<_>>();

    Ok::<_, Error>(Json(patches))
}

/// Get project patch.
/// `GET /projects/:project/patches/:id`
async fn patch_handler(
    State(ctx): State<Context>,
    Path((rid, patch_id)): Path<(RepoId, Oid)>,
) -> impl IntoResponse {
    let (repo, _) = ctx.repo(rid)?;
    let patches = ctx.profile.patches(&repo)?;
    let patch = patches.get(&patch_id.into())?.ok_or(Error::NotFound)?;
    let aliases = ctx.profile.aliases();

    Ok::<_, Error>(Json(api::json::patch(
        patch_id.into(),
        patch,
        &repo,
        &aliases,
    )))
}

#[cfg(test)]
mod routes {
    use std::net::SocketAddr;

    use axum::body::Body;
    use axum::extract::connect_info::MockConnectInfo;
    use axum::http::StatusCode;
    use pretty_assertions::assert_eq;
    use radicle::storage::ReadStorage;
    use serde_json::json;

    use crate::test::*;

    #[tokio::test]
    async fn test_projects_root() {
        let tmp = tempfile::tempdir().unwrap();
        let seed = seed(tmp.path());
        let app = super::router(seed.clone())
            .layer(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 8080))));
        let response = get(&app, "/projects?show=all").await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.json().await,
            json!([
              {
                "name": "hello-world",
                "description": "Rad repository for tests",
                "defaultBranch": "master",
                "delegates": [DID],
                "visibility": {
                  "type": "public"
                },
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
                "id": RID,
                "seeding": 0,
              },
            ])
        );

        let app = super::router(seed).layer(MockConnectInfo(SocketAddr::from((
            [192, 168, 13, 37],
            8080,
        ))));
        let response = get(&app, "/projects?show=all").await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.json().await,
            json!([
              {
                "name": "hello-world",
                "description": "Rad repository for tests",
                "defaultBranch": "master",
                "delegates": [DID],
                "visibility": {
                  "type": "public"
                },
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
                "id": RID,
                "seeding": 0,
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
               "visibility": {
                 "type": "public"
               },
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
               "id": RID,
               "seeding": 0,
            })
        );
    }

    #[tokio::test]
    async fn test_projects_not_found() {
        let tmp = tempfile::tempdir().unwrap();
        let app = super::router(seed(tmp.path()));
        let response = get(&app, "/projects/rad:z2u2CP3ZJzB7ZqE8jHrau19yjcfCQ").await;

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
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
                  "id": HEAD,
                  "author": {
                    "name": "Alice Liddell",
                    "email": "alice@radicle.xyz"
                  },
                  "summary": "Add another folder",
                  "description": "",
                  "parents": [
                    "ee8d6a29304623a78ebfa5eeed5af674d0e58f83",
                  ],
                  "committer": {
                    "name": "Alice Liddell",
                    "email": "alice@radicle.xyz",
                    "time": 1673003014
                  },
                },
                {
                  "id": PARENT,
                  "author": {
                    "name": "Alice Liddell",
                    "email": "alice@radicle.xyz"
                  },
                  "summary": "Add contributing file",
                  "description": "",
                  "parents": [
                    "f604ce9fd5b7cc77b7609beda45ea8760bee78f7",
                  ],
                  "committer": {
                    "name": "Alice Liddell",
                    "email": "alice@radicle.xyz",
                    "time": 1673002014,
                  },
                },
                {
                  "id": INITIAL_COMMIT,
                  "author": {
                    "name": "Alice Liddell",
                    "email": "alice@radicle.xyz",
                  },
                  "summary": "Initial commit",
                  "description": "",
                  "parents": [],
                  "committer": {
                    "name": "Alice Liddell",
                    "email": "alice@radicle.xyz",
                    "time": 1673001014,
                  },
                },
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
                "parents": [
                  "ee8d6a29304623a78ebfa5eeed5af674d0e58f83",
                ],
                "committer": {
                  "name": "Alice Liddell",
                  "email": "alice@radicle.xyz",
                  "time": 1673003014
                },
              },
              "diff": {
                "files": [
                  {
                    "state": "deleted",
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
                          "old":  {
                            "start": 1,
                            "end": 2,
                          },
                          "new": {
                            "start": 0,
                            "end": 0,
                          },
                        },
                      ],
                      "stats": {
                        "additions": 0,
                        "deletions": 1,
                      },
                      "eof": "noneMissing",
                    },
                    "old": {
                      "oid": "82eb77880c693655bce074e3dbbd9fa711dc018b",
                      "mode": "blob",
                    },
                  },
                  {
                    "state": "added",
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
                          "old":  {
                            "start": 0,
                            "end": 0,
                          },
                          "new": {
                            "start": 1,
                            "end": 2,
                          },
                        },
                      ],
                      "stats": {
                        "additions": 1,
                        "deletions": 0,
                      },
                      "eof": "noneMissing",
                    },
                    "new": {
                      "oid": "980a0d5f19a64b4b30a87d4206aade58726b60e3",
                      "mode": "blob",
                    },
                  },
                  {
                    "state": "added",
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
                          ],
                          "old":  {
                            "start": 0,
                            "end": 0,
                          },
                          "new": {
                            "start": 1,
                            "end": 2,
                          },
                        }
                      ],
                      "stats": {
                        "additions": 1,
                        "deletions": 0,
                      },
                      "eof": "noneMissing",
                    },
                    "new": {
                      "oid": "1dd5654ca2d2cf9f33b14c92b5ca9e1d21a91ae1",
                      "mode": "blob",
                    },
                  },
                ],
                "stats": {
                  "filesChanged": 3,
                  "insertions": 2,
                  "deletions": 1
                }
              },
              "files": {
                "82eb77880c693655bce074e3dbbd9fa711dc018b": {
                  "id": "82eb77880c693655bce074e3dbbd9fa711dc018b",
                  "binary": false,
                  "content": "Thank you very much!\n",
                },
                "980a0d5f19a64b4b30a87d4206aade58726b60e3": {
                  "id": "980a0d5f19a64b4b30a87d4206aade58726b60e3",
                  "binary": false,
                  "content": "Hello World!\n",
                },
                "1dd5654ca2d2cf9f33b14c92b5ca9e1d21a91ae1": {
                  "id": "1dd5654ca2d2cf9f33b14c92b5ca9e1d21a91ae1",
                  "binary": false,
                  "content": "Hello World from dir1!\n",
                },
              },
              "branches": [
                "refs/heads/master"
              ]
            })
        );
    }

    #[tokio::test]
    async fn test_projects_commits_not_found() {
        let tmp = tempfile::tempdir().unwrap();
        let app = super::router(seed(tmp.path()));
        let response = get(
            &app,
            format!("/projects/{RID}/commits/ffffffffffffffffffffffffffffffffffffffff"),
        )
        .await;

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
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
                    "oid": "2d1c3cbfcf1d190d7fc77ac8f9e53db0e91a9ad3",
                    "name": "dir1",
                    "kind": "tree"
                  },
                  {
                    "path": "README",
                    "oid": "980a0d5f19a64b4b30a87d4206aade58726b60e3",
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
                  "parents": [
                    "ee8d6a29304623a78ebfa5eeed5af674d0e58f83",
                  ],
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
                  "oid": "1dd5654ca2d2cf9f33b14c92b5ca9e1d21a91ae1",
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
                "parents": [
                  "ee8d6a29304623a78ebfa5eeed5af674d0e58f83",
                ],
                "committer": {
                  "name": "Alice Liddell",
                  "email": "alice@radicle.xyz",
                  "time": 1673003014
                },
              },
              "name": "dir1",
              "path": "dir1",
              "stats": {
                "commits": 3,
                "branches": 1,
                "contributors": 1
              }
            })
        );
    }

    #[tokio::test]
    async fn test_projects_tree_not_found() {
        let tmp = tempfile::tempdir().unwrap();
        let app = super::router(seed(tmp.path()));
        let response = get(
            &app,
            format!("/projects/{RID}/tree/ffffffffffffffffffffffffffffffffffffffff"),
        )
        .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let response = get(&app, format!("/projects/{RID}/tree/{HEAD}/unknown")).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
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
                "alias": CONTRIBUTOR_ALIAS,
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
    async fn test_projects_remotes_not_found() {
        let tmp = tempfile::tempdir().unwrap();
        let app = super::router(seed(tmp.path()));
        let response = get(
            &app,
            format!("/projects/{RID}/remotes/z6MksFqXN3Yhqk8pTJdUGLwATkRfQvwZXPqR2qMEhbS9wzpT"),
        )
        .await;

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
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
                  "parents": [
                    "ee8d6a29304623a78ebfa5eeed5af674d0e58f83"
                  ],
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
    async fn test_projects_blob_not_found() {
        let tmp = tempfile::tempdir().unwrap();
        let app = super::router(seed(tmp.path()));
        let response = get(&app, format!("/projects/{RID}/blob/{HEAD}/unknown")).await;

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
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
                "name": "README",
                "path": "README",
                "lastCommit": {
                  "id": INITIAL_COMMIT,
                  "author": {
                    "name": "Alice Liddell",
                    "email": "alice@radicle.xyz"
                  },
                  "summary": "Initial commit",
                  "description": "",
                  "parents": [],
                  "committer": {
                    "name": "Alice Liddell",
                    "email": "alice@radicle.xyz",
                    "time": 1673001014
                  },
                },
                "content": "Hello World!\n"
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
                  "files": [
                    {
                      "state": "added",
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
                            "old":  {
                              "start": 0,
                              "end": 0,
                            },
                            "new": {
                              "start": 1,
                              "end": 2,
                            },
                          },
                        ],
                        "stats": {
                          "additions": 1,
                          "deletions": 0,
                        },
                        "eof": "noneMissing",
                      },
                      "new": {
                        "oid": "1dd5654ca2d2cf9f33b14c92b5ca9e1d21a91ae1",
                        "mode": "blob",
                      },
                    },
                  ],
                  "stats": {
                    "filesChanged": 1,
                    "insertions": 1,
                    "deletions": 0,
                  },
                },
                "files": {
                  "1dd5654ca2d2cf9f33b14c92b5ca9e1d21a91ae1": {
                    "id": "1dd5654ca2d2cf9f33b14c92b5ca9e1d21a91ae1",
                    "binary": false,
                    "content": "Hello World from dir1!\n",
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
                    "parents": [
                      "ee8d6a29304623a78ebfa5eeed5af674d0e58f83"
                    ],
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
                    "parents": [
                      "f604ce9fd5b7cc77b7609beda45ea8760bee78f7",
                    ],
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
                  "id": DID,
                  "alias": CONTRIBUTOR_ALIAS
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
                      "id": DID,
                      "alias": CONTRIBUTOR_ALIAS
                    },
                    "body": "Change 'hello world' to 'hello everyone'",
                    "edits": [
                      {
                        "author": {
                          "id": DID,
                          "alias": CONTRIBUTOR_ALIAS
                        },
                        "body": "Change 'hello world' to 'hello everyone'",
                        "timestamp": TIMESTAMP,
                        "embeds": [],
                      },
                    ],
                    "embeds": [],
                    "reactions": [],
                    "timestamp": TIMESTAMP,
                    "replyTo": null,
                    "resolved": false,
                  }
                ],
                "labels": []
              }
            ])
        );
    }

    #[tokio::test]
    async fn test_projects_issues_create() {
        const CREATED_ISSUE_ID: &str = "fcd0d5940b55df596cf8079fd1845903f1104bcd";

        let tmp = tempfile::tempdir().unwrap();
        let ctx = contributor(tmp.path());
        let app = super::router(ctx.to_owned());

        create_session(ctx).await;

        let body = serde_json::to_vec(&json!({
            "title": "Issue #2",
            "description": "Change 'hello world' to 'hello everyone'",
            "labels": ["bug"],
            "embeds": [
              {
                "name": "example.html",
                "content": "data:image/png;base64,PGh0bWw+SGVsbG8gV29ybGQhPC9odG1sPg=="
              }
            ],
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
                "alias": CONTRIBUTOR_ALIAS
              },
              "title": "Issue #2",
              "state": {
                "status": "open",
              },
              "assignees": [],
              "discussion": [{
                "id": CREATED_ISSUE_ID,
                "author": {
                  "id": CONTRIBUTOR_DID,
                  "alias": CONTRIBUTOR_ALIAS
                },
                "body": "Change 'hello world' to 'hello everyone'",
                "edits": [
                  {
                    "author": {
                      "id": CONTRIBUTOR_DID,
                      "alias": CONTRIBUTOR_ALIAS
                    },
                    "body": "Change 'hello world' to 'hello everyone'",
                    "timestamp": TIMESTAMP,
                    "embeds": [
                      {
                        "name": "example.html",
                        "content": "git:b62df2ec90365e3749cd4fa431cb844492908b84",
                      },
                    ],
                  },
                ],
                "embeds": [
                  {
                    "name": "example.html",
                    "content": "git:b62df2ec90365e3749cd4fa431cb844492908b84"
                  }
                ],
                "reactions": [],
                "timestamp": TIMESTAMP,
                "replyTo": null,
                "resolved": false,
              }],
              "labels": [
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
          "type": "comment",
          "body": "This is first-level comment",
          "embeds": [
            {
              "name": "image.jpg",
              "content": "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGP4//8/AAX+Av4N70a4AAAAAElFTkSuQmCC"
            }
          ],
          "replyTo": ISSUE_DISCUSSION_ID,
        }))
        .unwrap();

        let response = patch(
            &app,
            format!("/projects/{CONTRIBUTOR_RID}/issues/{ISSUE_DISCUSSION_ID}"),
            Some(Body::from(body)),
            Some(SESSION_ID.to_string()),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);

        // Get ID to redact later in the test
        let response = response.json().await;
        let id = &response["id"];
        assert!(id.is_string());

        let body = serde_json::to_vec(&json!({
          "type": "comment.react",
          "id": ISSUE_DISCUSSION_ID,
          "reaction": "🚀",
          "active": true,
        }))
        .unwrap();
        patch(
            &app,
            format!("/projects/{CONTRIBUTOR_RID}/issues/{ISSUE_DISCUSSION_ID}"),
            Some(Body::from(body)),
            Some(SESSION_ID.to_string()),
        )
        .await;

        let body = serde_json::to_vec(&json!({
          "type": "comment.edit",
          "id": ISSUE_DISCUSSION_ID,
          "body": "EDIT: Change 'hello world' to 'hello anyone'",
          "embeds": [
            {
              "name":"image.jpg",
              "content": "git:94381b429d7f7fe87e1bade52d893ab348ae29cc"
            }
          ]
        }))
        .unwrap();

        let response = patch(
            &app,
            format!("/projects/{CONTRIBUTOR_RID}/issues/{ISSUE_DISCUSSION_ID}"),
            Some(Body::from(body)),
            Some(SESSION_ID.to_string()),
        )
        .await;

        assert_eq!(response.success().await, true);

        let body = serde_json::to_vec(&json!({
          "type": "comment.redact",
          "id": id.as_str().unwrap(),
        }))
        .unwrap();

        let response = patch(
            &app,
            format!("/projects/{CONTRIBUTOR_RID}/issues/{ISSUE_DISCUSSION_ID}"),
            Some(Body::from(body)),
            Some(SESSION_ID.to_string()),
        )
        .await;

        assert_eq!(response.success().await, true);

        let response = get(
            &app,
            format!("/projects/{CONTRIBUTOR_RID}/issues/{ISSUE_DISCUSSION_ID}"),
        )
        .await;

        assert_eq!(
            response.json().await,
            json!({
              "id": ISSUE_DISCUSSION_ID,
              "author": {
                "id": CONTRIBUTOR_DID,
                "alias": CONTRIBUTOR_ALIAS
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
                    "alias": CONTRIBUTOR_ALIAS
                  },
                  "body": "EDIT: Change 'hello world' to 'hello anyone'",
                  "edits": [
                    {
                      "author": {
                        "id": CONTRIBUTOR_DID,
                        "alias": CONTRIBUTOR_ALIAS
                      },
                      "body": "Change 'hello world' to 'hello everyone'",
                      "timestamp": TIMESTAMP,
                      "embeds": [],
                    },
                    {
                      "author": {
                        "id": CONTRIBUTOR_DID,
                        "alias": CONTRIBUTOR_ALIAS
                      },
                      "body": "EDIT: Change 'hello world' to 'hello anyone'",
                      "timestamp": TIMESTAMP,
                      "embeds": [
                        {
                          "name": "image.jpg",
                          "content": "git:94381b429d7f7fe87e1bade52d893ab348ae29cc",
                        },
                      ],
                    },
                  ],
                  "embeds": [
                    {
                      "name": "image.jpg",
                      "content": "git:94381b429d7f7fe87e1bade52d893ab348ae29cc",
                    }
                  ],
                  "reactions": [
                    {
                      "emoji": "🚀",
                      "authors": [
                        {
                          "id": CONTRIBUTOR_DID,
                          "alias": CONTRIBUTOR_ALIAS,
                        }
                      ],
                    },
                  ],
                  "timestamp": TIMESTAMP,
                  "replyTo": null,
                  "resolved": false,
                },
              ],
              "labels": [],
            })
        );
    }

    #[tokio::test]
    async fn test_projects_issues_assign() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = contributor(tmp.path());
        let app = super::router(ctx.to_owned());

        create_session(ctx).await;

        let body = serde_json::to_vec(&json!({
          "type": "assign",
          "assignees": [CONTRIBUTOR_DID],
        }))
        .unwrap();

        let response = patch(
            &app,
            format!("/projects/{CONTRIBUTOR_RID}/issues/{ISSUE_DISCUSSION_ID}"),
            Some(Body::from(body)),
            Some(SESSION_ID.to_string()),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);

        let response = get(
            &app,
            format!("/projects/{CONTRIBUTOR_RID}/issues/{ISSUE_DISCUSSION_ID}"),
        )
        .await;

        assert_eq!(
            response.json().await,
            json!({
              "id": ISSUE_DISCUSSION_ID,
              "author": {
                "id": CONTRIBUTOR_DID,
                "alias": CONTRIBUTOR_ALIAS,
              },
              "title": "Issue #1",
              "state": {
                "status": "open",
              },
              "assignees": [
                {
                  "id": CONTRIBUTOR_DID,
                  "alias": CONTRIBUTOR_ALIAS,
                }
              ],
              "discussion": [
                {
                  "id": ISSUE_DISCUSSION_ID,
                  "author": {
                    "id": CONTRIBUTOR_DID,
                    "alias": CONTRIBUTOR_ALIAS,
                  },
                  "body": "Change 'hello world' to 'hello everyone'",
                  "edits": [
                    {
                      "author": {
                        "id": CONTRIBUTOR_DID,
                        "alias": CONTRIBUTOR_ALIAS,
                      },
                      "body": "Change 'hello world' to 'hello everyone'",
                      "timestamp": TIMESTAMP,
                      "embeds": [],
                    },
                  ],
                  "embeds": [],
                  "reactions": [],
                  "timestamp": TIMESTAMP,
                  "replyTo": null,
                  "resolved": false,
                },
              ],
              "labels": [],
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
          "type": "comment",
          "body": "This is a reply to the first comment",
          "embeds": [
            {
              "name": "image.jpg",
              "content": "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGP4//8/AAX+Av4N70a4AAAAAElFTkSuQmCC"
            }
          ],
          "replyTo": ISSUE_DISCUSSION_ID,
        }))
        .unwrap();

        let _ = get(&app, format!("/projects/{CONTRIBUTOR_RID}/issues")).await;
        let response = patch(
            &app,
            format!("/projects/{CONTRIBUTOR_RID}/issues/{ISSUE_DISCUSSION_ID}"),
            Some(Body::from(body)),
            Some(SESSION_ID.to_string()),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.success().await, true);

        let response = get(
            &app,
            format!("/projects/{CONTRIBUTOR_RID}/issues/{ISSUE_DISCUSSION_ID}"),
        )
        .await;

        assert_eq!(
            response.json().await,
            json!({
              "id": ISSUE_DISCUSSION_ID,
              "author": {
                "id": CONTRIBUTOR_DID,
                "alias": CONTRIBUTOR_ALIAS
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
                    "alias": CONTRIBUTOR_ALIAS
                  },
                  "body": "Change 'hello world' to 'hello everyone'",
                  "edits": [
                    {
                      "author": {
                        "id": CONTRIBUTOR_DID,
                        "alias": CONTRIBUTOR_ALIAS
                      },
                      "body": "Change 'hello world' to 'hello everyone'",
                      "timestamp": TIMESTAMP,
                      "embeds": [],
                    },
                  ],
                  "embeds": [],
                  "reactions": [],
                  "timestamp": TIMESTAMP,
                  "replyTo": null,
                  "resolved": false,
                },
                {
                  "id": ISSUE_COMMENT_ID,
                  "author": {
                    "id": CONTRIBUTOR_DID,
                    "alias": CONTRIBUTOR_ALIAS
                  },
                  "body": "This is a reply to the first comment",
                  "edits": [
                    {
                      "author": {
                        "id": CONTRIBUTOR_DID,
                        "alias": CONTRIBUTOR_ALIAS
                      },
                      "body": "This is a reply to the first comment",
                      "timestamp": TIMESTAMP,
                      "embeds": [
                        {
                          "name": "image.jpg",
                          "content": "git:94381b429d7f7fe87e1bade52d893ab348ae29cc",
                        },
                      ],
                    },
                  ],
                  "embeds": [
                    {
                      "name": "image.jpg",
                      "content": "git:94381b429d7f7fe87e1bade52d893ab348ae29cc",
                    }
                  ],
                  "reactions": [],
                  "timestamp": TIMESTAMP,
                  "replyTo": ISSUE_DISCUSSION_ID,
                  "resolved": false,
                },
              ],
              "labels": [],
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
                  "id": CONTRIBUTOR_DID,
                  "alias": CONTRIBUTOR_ALIAS
                },
                "title": "A new `hello world`",
                "state": { "status": "open" },
                "target": "delegates",
                "labels": [],
                "merges": [],
                "assignees": [],
                "revisions": [
                  {
                    "id": CONTRIBUTOR_PATCH_ID,
                    "reactions": [],
                    "author": {
                      "id": CONTRIBUTOR_DID,
                      "alias": CONTRIBUTOR_ALIAS
                    },
                    "description": "change `hello world` in README to something else",
                    "edits": [
                      {
                        "author": {
                          "id": CONTRIBUTOR_DID,
                          "alias": CONTRIBUTOR_ALIAS
                        },
                        "body": "change `hello world` in README to something else",
                        "timestamp": TIMESTAMP,
                        "embeds": [],
                      },
                    ],
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
                  "id": CONTRIBUTOR_DID,
                  "alias": CONTRIBUTOR_ALIAS
                },
                "title": "A new `hello world`",
                "state": { "status": "open" },
                "target": "delegates",
                "labels": [],
                "merges": [],
                "assignees": [],
                "revisions": [
                  {
                    "id": CONTRIBUTOR_PATCH_ID,
                    "reactions": [],
                    "author": {
                      "id": CONTRIBUTOR_DID,
                      "alias": CONTRIBUTOR_ALIAS
                    },
                    "description": "change `hello world` in README to something else",
                    "edits": [
                      {
                        "author": {
                          "id": CONTRIBUTOR_DID,
                          "alias": CONTRIBUTOR_ALIAS
                        },
                        "body": "change `hello world` in README to something else",
                        "timestamp": TIMESTAMP,
                        "embeds": [],
                      },
                    ],
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
        const CREATED_PATCH_ID: &str = "9aabc4055fd811f915c55e9a6ea9f525aa3e88f2";

        let tmp = tempfile::tempdir().unwrap();
        let ctx = contributor(tmp.path());
        let app = super::router(ctx.to_owned());

        create_session(ctx).await;

        let body = serde_json::to_vec(&json!({
          "title": "Update README",
          "description": "Do some changes to README",
          "target": INITIAL_COMMIT,
          "oid": HEAD,
          "labels": [],
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
                  "id": CONTRIBUTOR_DID,
                  "alias": CONTRIBUTOR_ALIAS
                },
                "title": "Update README",
                "state": { "status": "open" },
                "target": "delegates",
                "labels": [],
                "merges": [],
                "assignees": [],
                "revisions": [
                  {
                    "id": CREATED_PATCH_ID,
                    "reactions": [],
                    "author": {
                      "id": CONTRIBUTOR_DID,
                      "alias": CONTRIBUTOR_ALIAS
                    },
                    "description": "Do some changes to README",
                    "edits": [
                      {
                        "author": {
                          "id": CONTRIBUTOR_DID,
                          "alias": CONTRIBUTOR_ALIAS
                        },
                        "body": "Do some changes to README",
                        "timestamp": TIMESTAMP,
                        "embeds": [],
                      },
                   ],
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
    async fn test_projects_patches_assign() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = contributor(tmp.path());
        let app = super::router(ctx.to_owned());
        create_session(ctx).await;
        let body = serde_json::to_vec(&json!({
          "type": "assign",
          "assignees": [CONTRIBUTOR_DID]
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
                "alias": CONTRIBUTOR_ALIAS
              },
              "title": "A new `hello world`",
              "state": { "status": "open" },
              "target": "delegates",
              "labels": [],
              "merges": [],
              "assignees": [
                {
                  "id": CONTRIBUTOR_DID,
                  "alias": CONTRIBUTOR_ALIAS,
                }
              ],
              "revisions": [
                {
                  "id": CONTRIBUTOR_PATCH_ID,
                  "author": {
                    "id": CONTRIBUTOR_DID,
                    "alias": CONTRIBUTOR_ALIAS
                  },
                  "description": "change `hello world` in README to something else",
                  "edits": [
                    {
                      "author": {
                        "id": CONTRIBUTOR_DID,
                        "alias": CONTRIBUTOR_ALIAS
                      },
                      "body": "change `hello world` in README to something else",
                      "timestamp": TIMESTAMP,
                      "embeds": [],
                    },
                  ],
                  "reactions": [],
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
    async fn test_projects_patches_label() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = contributor(tmp.path());
        let app = super::router(ctx.to_owned());
        create_session(ctx).await;
        let body = serde_json::to_vec(&json!({
          "type": "label",
          "labels": ["bug","design"],
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
                "alias": CONTRIBUTOR_ALIAS
              },
              "title": "A new `hello world`",
              "state": { "status": "open" },
              "target": "delegates",
              "labels": [
                "bug",
                "design"
              ],
              "merges": [],
              "assignees": [],
              "revisions": [
                {
                  "id": CONTRIBUTOR_PATCH_ID,
                  "author": {
                    "id": CONTRIBUTOR_DID,
                    "alias": CONTRIBUTOR_ALIAS
                  },
                  "description": "change `hello world` in README to something else",
                  "edits": [
                    {
                      "author": {
                        "id": CONTRIBUTOR_DID,
                        "alias": CONTRIBUTOR_ALIAS
                      },
                      "body": "change `hello world` in README to something else",
                      "timestamp": TIMESTAMP,
                      "embeds": [],
                    },
                  ],
                  "reactions": [],
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
                "alias": CONTRIBUTOR_ALIAS
              },
              "title": "A new `hello world`",
              "state": { "status": "open" },
              "target": "delegates",
              "labels": [],
              "merges": [],
              "assignees": [],
              "revisions": [
                {
                  "id": CONTRIBUTOR_PATCH_ID,
                  "author": {
                    "id": CONTRIBUTOR_DID,
                    "alias": CONTRIBUTOR_ALIAS
                  },
                  "description": "change `hello world` in README to something else",
                  "edits": [
                    {
                      "author": {
                        "id": CONTRIBUTOR_DID,
                        "alias": CONTRIBUTOR_ALIAS
                      },
                      "body": "change `hello world` in README to something else",
                      "timestamp": TIMESTAMP,
                      "embeds": [],
                    },
                  ],
                  "reactions": [],
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
                  "id": "cccf3b0675220f25b054b6625d84611cb6506d9a",
                  "author": {
                    "id": CONTRIBUTOR_DID,
                    "alias": CONTRIBUTOR_ALIAS
                  },
                  "description": "This is a new revision",
                  "edits": [
                    {
                      "author": {
                        "id": CONTRIBUTOR_DID,
                        "alias": CONTRIBUTOR_ALIAS
                      },
                      "body": "This is a new revision",
                      "timestamp": TIMESTAMP,
                      "embeds": [],
                    },
                  ],
                  "reactions": [],
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
                "alias": CONTRIBUTOR_ALIAS
              },
              "title": "This is a updated title",
              "state": { "status": "open" },
              "target": "delegates",
              "labels": [],
              "merges": [],
              "assignees": [],
              "revisions": [
                {
                  "id": CONTRIBUTOR_PATCH_ID,
                  "reactions": [],
                  "author": {
                    "id": CONTRIBUTOR_DID,
                    "alias": CONTRIBUTOR_ALIAS
                  },
                  "description": "change `hello world` in README to something else",
                  "edits": [
                    {
                      "author": {
                        "id": CONTRIBUTOR_DID,
                        "alias": CONTRIBUTOR_ALIAS
                      },
                      "body": "change `hello world` in README to something else",
                      "timestamp": TIMESTAMP,
                      "embeds": [],
                    },
                  ],
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
    async fn test_projects_patches_revisions_edit() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = contributor(tmp.path());
        let app = super::router(ctx.to_owned());
        create_session(ctx).await;
        let body = serde_json::to_vec(&json!({
          "type": "revision.edit",
          "revision": CONTRIBUTOR_PATCH_ID,
          "description": "Let's change the description a bit",
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

        let body = serde_json::to_vec(&json!({
          "type": "revision.react",
          "revision": CONTRIBUTOR_PATCH_ID,
          "reaction": "🚀",
          "location": {
            "commit": INITIAL_COMMIT,
            "path": "./README.md",
            "new": {
              "type": "lines",
              "range": {
                "start": 0,
                "end": 1
              }
            }
          },
          "active": true,
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

        let body = serde_json::to_vec(&json!({
          "type": "revision.react",
          "revision": CONTRIBUTOR_PATCH_ID,
          "reaction": "🙏",
          "location": null,
          "active": true,
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
                "alias": CONTRIBUTOR_ALIAS
              },
              "title": "A new `hello world`",
              "state": { "status": "open" },
              "target": "delegates",
              "labels": [],
              "merges": [],
              "assignees": [],
              "revisions": [
                {
                  "id": CONTRIBUTOR_PATCH_ID,
                  "author": {
                    "id": CONTRIBUTOR_DID,
                    "alias": CONTRIBUTOR_ALIAS
                  },
                  "description": "Let's change the description a bit",
                  "edits": [
                    {
                      "author": {
                        "id": CONTRIBUTOR_DID,
                        "alias": CONTRIBUTOR_ALIAS
                      },
                      "body": "change `hello world` in README to something else",
                      "timestamp": TIMESTAMP,
                      "embeds": [],
                    },
                    {
                      "author": {
                        "id": CONTRIBUTOR_DID,
                        "alias": CONTRIBUTOR_ALIAS
                      },
                      "body": "Let's change the description a bit",
                      "timestamp": TIMESTAMP,
                      "embeds": [],
                    },
                  ],
                  "reactions": [
                    {
                      "emoji": "🙏",
                      "authors": [
                        {
                          "id": CONTRIBUTOR_DID,
                          "alias": CONTRIBUTOR_ALIAS
                        }
                      ],
                    },
                    {
                      "location": {
                        "commit": INITIAL_COMMIT,
                        "path": "./README.md",
                        "old": null,
                        "new": {
                          "type": "lines",
                          "range": {
                            "start": 0,
                            "end": 1
                          }
                        }
                      },
                      "emoji": "🚀",
                      "authors": [
                        {
                          "id": CONTRIBUTOR_DID,
                          "alias": CONTRIBUTOR_ALIAS
                        }
                      ]
                    },
                  ],
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
          "type": "revision.comment",
          "revision": CONTRIBUTOR_PATCH_ID,
          "body": "This is a root level comment",
          "embeds": [
            {
              "name": "image.jpg",
              "content": "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGP4//8/AAX+Av4N70a4AAAAAElFTkSuQmCC"
            }
          ],
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

        let comment_id = response.id().await.to_string();
        let comment_react_body = serde_json::to_vec(&json!({
          "type": "revision.comment.react",
          "revision": CONTRIBUTOR_PATCH_ID,
          "comment": comment_id,
          "reaction": "🚀",
          "active": true
        }))
        .unwrap();
        patch(
            &app,
            format!("/projects/{CONTRIBUTOR_RID}/patches/{CONTRIBUTOR_PATCH_ID}"),
            Some(Body::from(comment_react_body)),
            Some(SESSION_ID.to_string()),
        )
        .await;

        let comment_edit = serde_json::to_vec(&json!({
          "type": "revision.comment.edit",
          "revision": CONTRIBUTOR_PATCH_ID,
          "comment": comment_id,
          "body": "EDIT: This is a root level comment",
          "embeds": [
            {
              "name": "image.jpg",
              "content": "git:94381b429d7f7fe87e1bade52d893ab348ae29cc",
            }
          ],
        }))
        .unwrap();
        let response = patch(
            &app,
            format!("/projects/{CONTRIBUTOR_RID}/patches/{CONTRIBUTOR_PATCH_ID}"),
            Some(Body::from(comment_edit)),
            Some(SESSION_ID.to_string()),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        let reply_body = serde_json::to_vec(&json!({
          "type": "revision.comment",
          "revision": CONTRIBUTOR_PATCH_ID,
          "body": "This is a root level comment",
          "replyTo": comment_id,
          "embeds": [],
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
        let comment_id_2 = response.id().await.to_string();

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
                "alias": CONTRIBUTOR_ALIAS
              },
              "title": "A new `hello world`",
              "state": { "status": "open" },
              "target": "delegates",
              "labels": [],
              "merges": [],
              "assignees": [],
              "revisions": [
                {
                  "id": CONTRIBUTOR_PATCH_ID,
                  "author": {
                    "id": CONTRIBUTOR_DID,
                    "alias": CONTRIBUTOR_ALIAS
                  },
                  "description": "change `hello world` in README to something else",
                  "edits": [
                    {
                      "author": {
                        "id": CONTRIBUTOR_DID,
                        "alias": CONTRIBUTOR_ALIAS
                      },
                      "body": "change `hello world` in README to something else",
                      "timestamp": TIMESTAMP,
                      "embeds": [],
                    },
                  ],
                  "reactions": [],
                  "base": PARENT,
                  "oid": HEAD,
                  "refs": [
                    "refs/heads/master",
                  ],
                  "discussions": [
                    {
                      "id": comment_id,
                      "author": {
                        "id": CONTRIBUTOR_DID,
                        "alias": CONTRIBUTOR_ALIAS
                      },
                      "body": "EDIT: This is a root level comment",
                      "edits": [
                        {
                          "author": {
                            "id": CONTRIBUTOR_DID,
                            "alias": CONTRIBUTOR_ALIAS
                          },
                          "body": "This is a root level comment",
                          "timestamp": TIMESTAMP,
                          "embeds": [
                            {
                                "name": "image.jpg",
                                "content": "git:94381b429d7f7fe87e1bade52d893ab348ae29cc",
                            },
                          ],
                        },
                        {
                          "author": {
                            "id": CONTRIBUTOR_DID,
                            "alias": CONTRIBUTOR_ALIAS
                          },
                          "body": "EDIT: This is a root level comment",
                          "timestamp": TIMESTAMP,
                          "embeds": [
                           {
                                "name": "image.jpg",
                                "content": "git:94381b429d7f7fe87e1bade52d893ab348ae29cc",
                            },
                          ],
                        },
                      ],
                      "embeds": [
                        {
                          "name": "image.jpg",
                          "content": "git:94381b429d7f7fe87e1bade52d893ab348ae29cc",
                        }
                      ],
                      "reactions": [
                        {
                          "emoji": "🚀",
                          "authors": [
                            {
                              "id": CONTRIBUTOR_DID,
                              "alias": CONTRIBUTOR_ALIAS
                            }
                          ],
                        },
                      ],
                      "timestamp": TIMESTAMP,
                      "replyTo": null,
                      "location": null,
                      "resolved": false,
                    },
                    {
                      "id": comment_id_2,
                      "author": {
                        "id": CONTRIBUTOR_DID,
                        "alias": CONTRIBUTOR_ALIAS
                      },
                      "body": "This is a root level comment",
                      "edits": [
                        {
                          "author": {
                            "id": CONTRIBUTOR_DID,
                            "alias": CONTRIBUTOR_ALIAS
                          },
                          "body": "This is a root level comment",
                          "timestamp": TIMESTAMP,
                          "embeds": [],
                        },
                      ],
                      "embeds": [],
                      "reactions": [],
                      "timestamp": TIMESTAMP,
                      "replyTo": comment_id,
                      "location": null,
                      "resolved": false,
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
          "summary": "A small review",
          "verdict": "accept",
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

        let review_id = response.id().await.to_string();
        let review_comment_body = serde_json::to_vec(&json!({
          "type": "review.comment",
          "review": review_id,
          "body": "This is a comment on a review",
          "embeds": [
            {
              "name": "image.jpg",
              "content": "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGP4//8/AAX+Av4N70a4AAAAAElFTkSuQmCC"
            }
          ],
          "location": {
            "commit": HEAD,
            "path": "README.md",
            "new": {
              "type": "lines",
              "range": {
                "start": 2,
                "end": 4
              }
            }
          }
        }))
        .unwrap();
        let response = patch(
            &app,
            format!("/projects/{CONTRIBUTOR_RID}/patches/{CONTRIBUTOR_PATCH_ID}"),
            Some(Body::from(review_comment_body)),
            Some(SESSION_ID.to_string()),
        )
        .await;

        let comment_id = response.id().await.to_string();
        let review_comment_edit_body = serde_json::to_vec(&json!({
          "type": "review.comment.edit",
          "review": review_id,
          "comment": comment_id,
          "embeds": [
            {
              "name": "image.jpg",
              "content": "git:94381b429d7f7fe87e1bade52d893ab348ae29cc",
            }
          ],
          "body": "EDIT: This is a comment on a review",
        }))
        .unwrap();
        patch(
            &app,
            format!("/projects/{CONTRIBUTOR_RID}/patches/{CONTRIBUTOR_PATCH_ID}"),
            Some(Body::from(review_comment_edit_body)),
            Some(SESSION_ID.to_string()),
        )
        .await;

        let review_react_body = serde_json::to_vec(&json!({
          "type": "review.comment.react",
          "review": review_id,
          "comment": comment_id,
          "reaction": "🚀",
          "active": true
        }))
        .unwrap();
        patch(
            &app,
            format!("/projects/{CONTRIBUTOR_RID}/patches/{CONTRIBUTOR_PATCH_ID}"),
            Some(Body::from(review_react_body)),
            Some(SESSION_ID.to_string()),
        )
        .await;

        let review_resolve_body = serde_json::to_vec(&json!({
          "type": "review.comment.resolve",
          "review": review_id,
          "comment": comment_id,
        }))
        .unwrap();
        patch(
            &app,
            format!("/projects/{CONTRIBUTOR_RID}/patches/{CONTRIBUTOR_PATCH_ID}"),
            Some(Body::from(review_resolve_body)),
            Some(SESSION_ID.to_string()),
        )
        .await;

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
                "alias": CONTRIBUTOR_ALIAS
              },
              "title": "A new `hello world`",
              "state": { "status": "open" },
              "target": "delegates",
              "labels": [],
              "merges": [],
              "assignees": [],
              "revisions": [
                {
                  "id": CONTRIBUTOR_PATCH_ID,
                  "author": {
                    "id": CONTRIBUTOR_DID,
                    "alias": CONTRIBUTOR_ALIAS
                  },
                  "description": "change `hello world` in README to something else",
                  "edits": [
                    {
                      "author": {
                        "id": CONTRIBUTOR_DID,
                        "alias": CONTRIBUTOR_ALIAS
                      },
                      "body": "change `hello world` in README to something else",
                      "timestamp": TIMESTAMP,
                      "embeds": [],
                    },
                  ],
                  "reactions": [],
                  "base": PARENT,
                  "oid": HEAD,
                  "refs": [
                    "refs/heads/master",
                  ],
                  "discussions": [],
                  "timestamp": TIMESTAMP,
                  "reviews": [
                    {
                      "id": "140a44a4eac2cdb74b2f5f95a9dce97847eb9636",
                      "author": {
                          "id": CONTRIBUTOR_DID,
                          "alias": CONTRIBUTOR_ALIAS
                      },
                      "verdict": "accept",
                      "summary": "A small review",
                      "comments": [
                        {
                          "id": "0dcfca53416761cf975cc4cd6d452790cee06b49",
                          "author": {
                            "id": CONTRIBUTOR_DID,
                            "alias": CONTRIBUTOR_ALIAS
                          },
                          "body": "EDIT: This is a comment on a review",
                          "edits": [
                            {
                              "author": {
                                "id": CONTRIBUTOR_DID,
                                "alias": CONTRIBUTOR_ALIAS
                              },
                              "body": "This is a comment on a review",
                              "timestamp": 1671125284,
                              "embeds": [
                                {
                                  "name": "image.jpg",
                                  "content": "git:94381b429d7f7fe87e1bade52d893ab348ae29cc",
                                },
                              ],
                            },
                            {
                              "author": {
                                "id": CONTRIBUTOR_DID,
                                "alias": CONTRIBUTOR_ALIAS
                              },
                              "body": "EDIT: This is a comment on a review",
                              "timestamp": 1671125284,
                              "embeds": [
                                {
                                  "name": "image.jpg",
                                  "content": "git:94381b429d7f7fe87e1bade52d893ab348ae29cc",
                                },
                              ],
                            },
                          ],
                          "embeds": [
                            {
                              "name": "image.jpg",
                              "content": "git:94381b429d7f7fe87e1bade52d893ab348ae29cc",
                            },
                          ],
                          "reactions": [
                            {
                              "emoji": "🚀",
                              "authors": [
                                {
                                  "id": CONTRIBUTOR_DID,
                                  "alias": CONTRIBUTOR_ALIAS
                                }
                              ],
                            },
                          ],
                          "timestamp": 1671125284,
                          "replyTo": null,
                          "location": {
                            "commit": HEAD,
                            "path": "README.md",
                            "old": null,
                            "new": {
                              "type": "lines",
                              "range": {
                                "start": 2,
                                "end": 4,
                              },
                            },
                          },
                          "resolved": true,
                        }
                      ],
                      "timestamp": 1671125284,
                    },
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
                "alias": CONTRIBUTOR_ALIAS
              },
              "title": "A new `hello world`",
              "state": {
                  "status": "merged",
                  "revision": CONTRIBUTOR_PATCH_ID,
                  "commit": PARENT,
              },
              "target": "delegates",
              "labels": [],
              "merges": [{
                  "author": {
                    "id": CONTRIBUTOR_DID,
                    "alias": CONTRIBUTOR_ALIAS
                  },
                  "commit": PARENT,
                  "timestamp": TIMESTAMP,
                  "revision": CONTRIBUTOR_PATCH_ID,
              }],
              "assignees": [],
              "revisions": [
                {
                  "id": CONTRIBUTOR_PATCH_ID,
                  "author": {
                    "id": CONTRIBUTOR_DID,
                    "alias": CONTRIBUTOR_ALIAS
                  },
                  "description": "change `hello world` in README to something else",
                  "edits": [
                    {
                      "author": {
                        "id": CONTRIBUTOR_DID,
                        "alias": CONTRIBUTOR_ALIAS
                      },
                      "body": "change `hello world` in README to something else",
                      "timestamp": TIMESTAMP,
                      "embeds": [],
                    },
                  ],
                  "reactions": [],
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
    async fn test_projects_private() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = seed(tmp.path());
        let app = super::router(ctx.to_owned());

        // Check that the repo exists.
        ctx.profile()
            .storage
            .repository(RID_PRIVATE.parse().unwrap())
            .unwrap();

        let response = get(&app, format!("/projects/{RID_PRIVATE}")).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let response = get(&app, format!("/projects/{RID_PRIVATE}/patches")).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let response = get(&app, format!("/projects/{RID_PRIVATE}/issues")).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let response = get(&app, format!("/projects/{RID_PRIVATE}/commits")).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let response = get(&app, format!("/projects/{RID_PRIVATE}/remotes")).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}

use axum::handler::Handler;
use axum::http::{header, HeaderValue};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Extension, Json, Router};
use hyper::StatusCode;
use serde_json::json;
use tower_http::set_header::SetResponseHeaderLayer;

use radicle::cob::issue::Issues;
use radicle::identity::{Doc, Id};
use radicle::storage::{Oid, ReadRepository, WriteRepository, WriteStorage};
use radicle_surf::git::History;

use crate::api::axum_extra::{Path, Query};
use crate::api::error::Error;
use crate::api::project::Info;
use crate::api::{Context, PaginationQuery};

const CACHE_1_HOUR: &str = "public, max-age=3600, must-revalidate";

pub fn router(ctx: Context) -> Router {
    Router::new()
        .route("/projects", get(project_root_handler))
        .route("/projects/:project", get(project_handler))
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
            let Ok(Doc { payload, .. }) = repo.project_of(ctx.profile.id()) else { return None };
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

/// Get project commit.
/// `GET /projects/:project/commits/:sha`
async fn commit_handler(
    Extension(ctx): Extension<Context>,
    Path((project, sha)): Path<(Id, Oid)>,
) -> impl IntoResponse {
    let storage = &ctx.profile.storage;
    let repo = storage.repository(project)?;
    let commit = radicle_surf::commit(&repo.raw().into(), sha)?;

    Ok::<_, Error>(Json(json!(commit)))
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
    let timestamps = History::new(repo.raw().into(), head)
        .unwrap()
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

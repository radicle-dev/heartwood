use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Extension, Json, Router};
use serde::Serialize;

use radicle::cob::store::Store;
use radicle::git::Oid;
use radicle::identity::project::Payload;
use radicle::identity::{Did, Doc};
use radicle::storage::{ReadRepository, WriteStorage};

use crate::api::axum_extra::{Path, Query};
use crate::api::error::Error;
use crate::api::Context;
use crate::api::PaginationQuery;

/// Project info.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Info {
    /// Project metadata.
    #[serde(flatten)]
    pub payload: Payload,
    pub head: Oid,
    pub patches: usize,
    pub issues: usize,
}

pub fn router(ctx: Context) -> Router {
    Router::new()
        .route(
            "/delegates/:delegate/projects",
            get(delegates_projects_handler),
        )
        .layer(Extension(ctx))
}

/// List all projects which delegate is a part of.
/// `GET /delegates/:delegate/projects`
async fn delegates_projects_handler(
    Extension(ctx): Extension<Context>,
    Path(delegate): Path<Did>,
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
            let Ok(Doc { payload, delegates, .. }) = repo.project_of(ctx.profile.id()) else { return None };

            if !delegates.iter().any(|d| d.id == delegate) {
                return None;
            }

            let Ok(cobs) = Store::open(ctx.profile.public_key, &repo) else { return None };
            let Ok(issues) = cobs.issues().count() else { return None };
            let Ok(patches) = cobs.patches().count() else { return None };

            Some(Info {
                payload,
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

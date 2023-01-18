use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};

use radicle::cob::issue::Issues;
use radicle::identity::Did;
use radicle::storage::{ReadRepository, WriteStorage};

use crate::api::axum_extra::{Path, Query};
use crate::api::error::Error;
use crate::api::project::Info;
use crate::api::Context;
use crate::api::PaginationQuery;

pub fn router(ctx: Context) -> Router {
    Router::new()
        .route(
            "/delegates/:delegate/projects",
            get(delegates_projects_handler),
        )
        .with_state(ctx)
}

/// List all projects which delegate is a part of.
/// `GET /delegates/:delegate/projects`
async fn delegates_projects_handler(
    State(ctx): State<Context>,
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
            let Ok(doc) = repo.identity_of(ctx.profile.id()) else { return None };
            let Ok(payload) = doc.project() else { return None };

            if !doc.delegates.iter().any(|d| *d == delegate) {
                return None;
            }

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

#[cfg(test)]
mod routes {
    use axum::http::StatusCode;
    use serde_json::json;

    use crate::api::test::{self, request, HEAD};

    #[tokio::test]
    async fn test_delegates_projects() {
        let tmp = tempfile::tempdir().unwrap();
        let app = super::router(test::seed(tmp.path()));
        let response = request(
            &app,
            "/delegates/did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/projects",
        )
        .await;

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
}

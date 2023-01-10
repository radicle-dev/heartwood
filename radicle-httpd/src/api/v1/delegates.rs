use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Extension, Json, Router};

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
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use serde_json::{json, Value};
    use tower::ServiceExt;

    use crate::api::test::{self, HEAD};

    #[tokio::test]
    async fn test_delegates_projects() {
        let app = super::router(test::seed());
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/delegates/did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/projects".to_string())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
        let body: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(
            body,
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

use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};

use radicle::cob::issue::Issues;
use radicle::cob::patch::Patches;
use radicle::identity::Did;
use radicle::storage::{ReadRepository, ReadStorage};

use crate::api::error::Error;
use crate::api::project::Info;
use crate::api::Context;
use crate::api::PaginationQuery;
use crate::axum_extra::{Path, Query};

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
        .repositories()?
        .into_iter()
        .filter_map(|id| {
            let Ok(repo) = storage.repository(id) else { return None };
            let Ok((_, head)) = repo.head() else { return None };
            let Ok((_, doc)) = repo.identity_doc() else { return None };
            let Ok(doc) = doc.verified() else { return None };
            let Ok(payload) = doc.project() else { return None };

            let delegates = doc.delegates;
            if !delegates.iter().any(|d| *d == delegate) {
                return None;
            }

            let Ok(issues) = Issues::open(&repo) else { return None };
            let Ok(issues) = issues.counts() else { return None };
            let Ok(patches) = Patches::open(&repo) else { return None };
            let Ok(patches) = patches.counts() else { return None };

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

#[cfg(test)]
mod routes {
    use axum::http::StatusCode;
    use serde_json::json;

    use crate::test::{self, get, HEAD, RID};

    #[tokio::test]
    async fn test_delegates_projects() {
        let tmp = tempfile::tempdir().unwrap();
        let app = super::router(test::seed(tmp.path()));
        let response = get(
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
                "delegates": ["did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi"],
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
}

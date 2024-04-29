use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};

use radicle::cob::Author;
use radicle::identity::Did;
use radicle::issue::cache::Issues as _;
use radicle::node::routing::Store;
use radicle::node::AliasStore;
use radicle::patch::cache::Patches as _;
use radicle::storage::{ReadRepository, ReadStorage};

use crate::api::error::Error;
use crate::api::json;
use crate::api::project::Info;
use crate::api::Context;
use crate::api::{PaginationQuery, ProjectQuery};
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
    let mut projects = match show {
        ProjectQuery::All => storage
            .repositories()?
            .into_iter()
            .filter(|repo| repo.doc.visibility.is_public())
            .collect::<Vec<_>>(),
        ProjectQuery::Pinned => storage.repositories_by_id(pinned.repositories.iter())?,
    };
    projects.sort_by_key(|p| p.rid);

    let infos = projects
        .into_iter()
        .filter_map(|id| {
            if !id.doc.delegates.iter().any(|d| *d == delegate) {
                return None;
            }
            let Ok(repo) = storage.repository(id.rid) else {
                return None;
            };
            let Ok((_, head)) = repo.head() else {
                return None;
            };
            let Ok(payload) = id.doc.project() else {
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

            let aliases = ctx.profile.aliases();
            let delegates = id
                .doc
                .delegates
                .into_iter()
                .map(|did| json::author(&Author::new(did), aliases.alias(did.as_key())))
                .collect::<Vec<_>>();
            let seeding = db.count(&id.rid).unwrap_or_default();

            Some(Info {
                payload,
                delegates,
                visibility: id.doc.visibility,
                head,
                issues,
                patches,
                id: id.rid,
                seeding,
            })
        })
        .skip(page * per_page)
        .take(per_page)
        .collect::<Vec<_>>();

    Ok::<_, Error>(Json(infos))
}

#[cfg(test)]
mod routes {
    use std::net::SocketAddr;

    use axum::extract::connect_info::MockConnectInfo;
    use axum::http::StatusCode;
    use serde_json::json;

    use crate::test::{self, get, HEAD, RID};

    #[tokio::test]
    async fn test_delegates_projects() {
        let tmp = tempfile::tempdir().unwrap();
        let seed = test::seed(tmp.path());
        let app = super::router(seed.clone())
            .layer(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 8080))));
        let response = get(
            &app,
            "/delegates/did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/projects?show=all",
        )
        .await;

        assert_eq!(
            response.status(),
            StatusCode::OK,
            "failed response: {:?}",
            response.json().await
        );
        assert_eq!(
            response.json().await,
            json!([
              {
                "name": "hello-world",
                "description": "Rad repository for tests",
                "defaultBranch": "master",
                "delegates": [
                  {
                    "id": "did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi",
                    "alias": "seed"
                  }
                ],
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
        let response = get(
            &app,
            "/delegates/did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/projects?show=all",
        )
        .await;

        assert_eq!(
            response.status(),
            StatusCode::OK,
            "failed response: {:?}",
            response.json().await
        );
        assert_eq!(
            response.json().await,
            json!([
              {
                "name": "hello-world",
                "description": "Rad repository for tests",
                "defaultBranch": "master",
                "delegates": [
                  {
                    "id": "did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi",
                    "alias": "seed"
                  }
                ],
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
}

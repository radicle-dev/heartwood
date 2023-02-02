use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
use axum::http::Method;
use axum::response::{IntoResponse, Json};
use axum::routing::get;
use axum::{Extension, Router};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::RwLock;
use tower_http::cors::{self, CorsLayer};

use radicle::cob::issue::Issues;
use radicle::identity::Id;
use radicle::storage::{ReadRepository, WriteStorage};
use radicle::Profile;

mod auth;
mod axum_extra;
mod error;
mod json;
#[cfg(test)]
mod test;
mod v1;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Identifier for sessions
type SessionId = String;

#[derive(Clone)]
pub struct Context {
    profile: Arc<Profile>,
    sessions: Arc<RwLock<HashMap<SessionId, auth::Session>>>,
}

impl Context {
    pub fn new(profile: Arc<Profile>) -> Self {
        Self {
            profile,
            sessions: Default::default(),
        }
    }

    pub fn project_info(&self, id: Id) -> Result<project::Info, error::Error> {
        let storage = &self.profile.storage;
        let repo = storage.repository(id)?;
        let (_, head) = repo.head()?;
        let payload = repo.project_of(self.profile.id())?;
        let issues = (Issues::open(self.profile.public_key, &repo)?).count()?;

        Ok(project::Info {
            payload,
            head,
            issues,
            patches: 0,
            id,
        })
    }
}

pub fn router(ctx: Context) -> Router {
    let root_router = Router::new()
        .route("/", get(root_handler))
        .layer(Extension(ctx.clone()));

    Router::new()
        .merge(root_router)
        .merge(v1::router(ctx))
        .layer(
            CorsLayer::new()
                .max_age(Duration::from_secs(86400))
                .allow_origin(cors::Any)
                .allow_methods([Method::GET, Method::POST, Method::PATCH, Method::PUT])
                .allow_headers([CONTENT_TYPE, AUTHORIZATION]),
        )
}

async fn root_handler(Extension(ctx): Extension<Context>) -> impl IntoResponse {
    let response = json!({
        "message": "Welcome!",
        "service": "radicle-httpd",
        "version": format!("{}-{}", VERSION, env!("GIT_HEAD")),
        "node": { "id": ctx.profile.public_key },
        "path": "/",
        "links": [
            {
                "href": "/v1/projects",
                "rel": "projects",
                "type": "GET"
            },
            {
                "href": "/v1/node",
                "rel": "node",
                "type": "GET"
            },
            {
                "href": "/v1/delegates/:did/projects",
                "rel": "projects",
                "type": "GET"
            },
            {
                "href": "/v1/stats",
                "rel": "stats",
                "type": "GET"
            }
        ]
    });

    Json(response)
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct PaginationQuery {
    pub page: Option<usize>,
    pub per_page: Option<usize>,
}

mod project {
    use radicle::git::Oid;
    use radicle::identity::project::Project;
    use radicle::identity::Id;
    use serde::Serialize;

    /// Project info.
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Info {
        /// Project metadata.
        #[serde(flatten)]
        pub payload: Project,
        pub head: Oid,
        pub patches: usize,
        pub issues: usize,
        pub id: Id,
    }
}

pub mod auth;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
use axum::http::Method;
use axum::response::{IntoResponse, Json};
use axum::routing::get;
use axum::Router;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::json;
use tokio::sync::RwLock;
use tower_http::cors::{self, CorsLayer};

use radicle::cob::issue::Issues;
use radicle::cob::patch::Patches;
use radicle::identity::Id;
use radicle::storage::{ReadRepository, ReadStorage};
use radicle::Profile;

mod error;
mod json;
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
        let doc = repo.identity_doc()?.1.verified()?;
        let payload = doc.project()?;
        let delegates = doc.delegates;
        let issues = Issues::open(&repo)?.counts()?;
        let patches = Patches::open(&repo)?.counts()?;

        Ok(project::Info {
            payload,
            delegates,
            head,
            issues,
            patches,
            id,
        })
    }

    #[cfg(test)]
    pub fn profile(&self) -> &Arc<Profile> {
        &self.profile
    }

    #[cfg(test)]
    pub fn sessions(&self) -> &Arc<RwLock<HashMap<SessionId, auth::Session>>> {
        &self.sessions
    }
}

pub fn router(ctx: Context) -> Router {
    Router::new()
        .route("/", get(root_handler))
        .merge(v1::router(ctx))
        .layer(
            CorsLayer::new()
                .max_age(Duration::from_secs(86400))
                .allow_origin(cors::Any)
                .allow_methods([
                    Method::GET,
                    Method::POST,
                    Method::PATCH,
                    Method::PUT,
                    Method::DELETE,
                ])
                .allow_headers([CONTENT_TYPE, AUTHORIZATION]),
        )
}

async fn root_handler() -> impl IntoResponse {
    let response = json!({
        "path": "/api",
        "links": [
            {
                "href": "/v1",
                "rel": "v1",
                "type": "GET"
            }
        ]
    });

    Json(response)
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PaginationQuery {
    pub page: Option<usize>,
    pub per_page: Option<usize>,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CobsQuery<T> {
    pub page: Option<usize>,
    pub per_page: Option<usize>,
    #[serde(default)]
    #[serde(deserialize_with = "parse_state")]
    #[serde(bound(deserialize = "T: std::str::FromStr, T::Err: std::fmt::Display"))]
    pub state: Option<T>,
}

fn parse_state<'de, T, D>(deserializer: D) -> Result<Option<T>, D::Error>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
    D: Deserializer<'de>,
{
    let state: String = Deserialize::deserialize(deserializer)?;
    T::from_str(&state)
        .map(Some)
        .map_err(serde::de::Error::custom)
}

mod project {
    use nonempty::NonEmpty;
    use serde::Serialize;

    use radicle::cob;
    use radicle::git::Oid;
    use radicle::identity::project::Project;
    use radicle::identity::Id;
    use radicle::prelude::Did;

    /// Project info.
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Info {
        /// Project metadata.
        #[serde(flatten)]
        pub payload: Project,
        pub delegates: NonEmpty<Did>,
        pub head: Oid,
        pub patches: cob::patch::PatchCounts,
        pub issues: cob::issue::IssueCounts,
        pub id: Id,
    }
}

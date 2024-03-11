pub mod auth;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
use axum::http::Method;
use axum::response::{IntoResponse, Json};
use axum::routing::get;
use axum::Router;
use radicle::issue::cache::Issues as _;
use radicle::patch::cache::Patches as _;
use radicle::storage::git::Repository;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::RwLock;
use tower_http::cors::{self, CorsLayer};

use radicle::identity::{DocAt, RepoId};
use radicle::node::policy::Scope;
use radicle::node::routing::Store;
use radicle::node::{Handle, NodeId};
use radicle::storage::{ReadRepository, ReadStorage};
use radicle::{Node, Profile};

mod error;
mod json;
mod v1;

use crate::api::error::Error;
use crate::cache::Cache;
use crate::Options;

pub const VERSION: &str = env!("RADICLE_VERSION");

/// Identifier for sessions
type SessionId = String;

#[derive(Clone)]
pub struct Context {
    profile: Arc<Profile>,
    sessions: Arc<RwLock<HashMap<SessionId, auth::Session>>>,
    cache: Option<Cache>,
}

impl Context {
    pub fn new(profile: Arc<Profile>, options: &Options) -> Self {
        Self {
            profile,
            sessions: Default::default(),
            cache: options.cache.map(Cache::new),
        }
    }

    pub fn project_info<R: ReadRepository + radicle::cob::Store>(
        &self,
        repo: &R,
        doc: DocAt,
    ) -> Result<project::Info, error::Error> {
        let (_, head) = repo.head()?;
        let DocAt { doc, .. } = doc;
        let id = repo.id();

        let payload = doc.project()?;
        let delegates = doc.delegates;
        let issues = self.profile.issues(repo)?.counts()?;
        let patches = self.profile.patches(repo)?.counts()?;
        let db = &self.profile.database()?;
        let seeding = db.count(&id).unwrap_or_default();

        Ok(project::Info {
            payload,
            delegates,
            visibility: doc.visibility,
            head,
            issues,
            patches,
            id,
            seeding,
        })
    }

    /// Get a repository by RID, checking to make sure we're allowed to view it.
    pub fn repo(&self, rid: RepoId) -> Result<(Repository, DocAt), error::Error> {
        let repo = self.profile.storage.repository(rid)?;
        let doc = repo.identity_doc()?;
        // Don't allow accessing private repos.
        if doc.visibility.is_private() {
            return Err(Error::NotFound);
        }
        Ok((repo, doc))
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
    #[serde(default)]
    pub show: ProjectQuery,
    pub page: Option<usize>,
    pub per_page: Option<usize>,
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub enum ProjectQuery {
    All,
    #[default]
    Pinned,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RawQuery {
    pub mime: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CobsQuery<T> {
    pub page: Option<usize>,
    pub per_page: Option<usize>,
    pub status: Option<T>,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PoliciesQuery {
    /// The NID from which to fetch from after tracking a repo.
    pub from: Option<NodeId>,
    pub scope: Option<Scope>,
}

mod project {
    use nonempty::NonEmpty;
    use serde::Serialize;

    use radicle::cob;
    use radicle::git::Oid;
    use radicle::identity::project::Project;
    use radicle::identity::{RepoId, Visibility};
    use radicle::prelude::Did;

    /// Project info.
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Info {
        /// Project metadata.
        #[serde(flatten)]
        pub payload: Project,
        pub delegates: NonEmpty<Did>,
        pub visibility: Visibility,
        pub head: Oid,
        pub patches: cob::patch::PatchCounts,
        pub issues: cob::issue::IssueCounts,
        pub id: RepoId,
        pub seeding: usize,
    }
}

/// Announce refs to the network for the given RID.
pub fn announce_refs(mut node: Node, rid: RepoId) -> Result<(), Error> {
    match node.announce_refs(rid) {
        Ok(_) => Ok(()),
        Err(e) if e.is_connection_err() => Ok(()),
        Err(e) => Err(e.into()),
    }
}

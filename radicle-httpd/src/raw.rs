use std::sync::Arc;

use axum::extract::State;
use axum::http::header;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use hyper::HeaderMap;

use radicle::prelude::Id;
use radicle::profile::Profile;
use radicle::storage::git::paths;
use radicle_surf::{Oid, Repository};

use crate::axum_extra::Path;
use crate::error::Error;

pub fn router(profile: Arc<Profile>) -> Router {
    Router::new()
        .route("/:project/:sha/*path", get(file_handler))
        .with_state(profile)
}

async fn file_handler(
    Path((project, sha, path)): Path<(Id, Oid, String)>,
    State(profile): State<Arc<Profile>>,
) -> impl IntoResponse {
    let storage = &profile.storage;
    let repo = Repository::open(paths::repository(storage, &project))?;
    let blob = repo.blob(sha, &path)?;

    let mut response_headers = HeaderMap::new();
    response_headers.insert(header::CONTENT_TYPE, "text; charset=utf-8".parse().unwrap());

    Ok::<_, Error>((response_headers, blob.content().to_owned()))
}

#[cfg(test)]
mod routes {
    use axum::http::StatusCode;

    use crate::test::{self, get, HEAD};

    #[tokio::test]
    async fn test_file_handler() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = test::seed(tmp.path());
        let app = super::router(ctx.profile().to_owned());

        let response = get(
            &app,
            format!("/rad:z4FucBZHZMCsxTyQE1dfE2YR59Qbp/{HEAD}/dir1/README"),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.body().await, "Hello World from dir1!\n");
    }
}

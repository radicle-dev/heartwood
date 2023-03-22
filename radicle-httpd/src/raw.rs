use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::http::{header, Method, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use hyper::HeaderMap;
use tower_http::cors;

use radicle::prelude::Id;
use radicle::profile::Profile;
use radicle::storage::git::paths;
use radicle_surf::{Oid, Repository};

use crate::axum_extra::Path;
use crate::error::RawError as Error;

const MAX_BLOB_SIZE: usize = 4_194_304;

static MIMES: &[(&str, &str)] = &[
    ("3gp", "video/3gpp"),
    ("7z", "application/x-7z-compressed"),
    ("aac", "audio/aac"),
    ("avi", "video/x-msvideo"),
    ("bin", "application/octet-stream"),
    ("bmp", "image/bmp"),
    ("bz", "application/x-bzip"),
    ("bz2", "application/x-bzip2"),
    ("csh", "application/x-csh"),
    ("css", "text/css"),
    ("csv", "text/csv"),
    ("doc", "application/msword"),
    (
        "docx",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    ),
    ("epub", "application/epub+zip"),
    ("gz", "application/gzip"),
    ("gif", "image/gif"),
    ("htm", "text/html"),
    ("html", "text/html"),
    ("ico", "image/vnd.microsoft.icon"),
    ("jar", "application/java-archive"),
    ("jpeg", "image/jpeg"),
    ("jpg", "image/jpeg"),
    ("js", "text/javascript"),
    ("json", "application/json"),
    ("mjs", "text/javascript"),
    ("mp3", "audio/mpeg"),
    ("mp4", "video/mp4"),
    ("mpeg", "video/mpeg"),
    ("odp", "application/vnd.oasis.opendocument.presentation"),
    ("ods", "application/vnd.oasis.opendocument.spreadsheet"),
    ("odt", "application/vnd.oasis.opendocument.text"),
    ("oga", "audio/ogg"),
    ("ogv", "video/ogg"),
    ("ogx", "application/ogg"),
    ("otf", "font/otf"),
    ("png", "image/png"),
    ("pdf", "application/pdf"),
    ("php", "application/x-httpd-php"),
    ("ppt", "application/vnd.ms-powerpoint"),
    (
        "pptx",
        "application/vnd.openxmlformats-officedocument.presentationml.presentation",
    ),
    ("rar", "application/vnd.rar"),
    ("rtf", "application/rtf"),
    ("sh", "application/x-sh"),
    ("svg", "image/svg+xml"),
    ("tar", "application/x-tar"),
    ("tif", "image/tiff"),
    ("tiff", "image/tiff"),
    ("ttf", "font/ttf"),
    ("txt", "text/plain"),
    ("wav", "audio/wav"),
    ("weba", "audio/webm"),
    ("webm", "video/webm"),
    ("webp", "image/webp"),
    ("woff", "font/woff"),
    ("woff2", "font/woff2"),
    ("xhtml", "application/xhtml+xml"),
    ("xls", "application/vnd.ms-excel"),
    (
        "xlsx",
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    ),
    ("xml", "application/xml"),
    ("zip", "application/zip"),
];

pub fn router(profile: Arc<Profile>) -> Router {
    Router::new()
        .route("/:project/:sha/*path", get(file_handler))
        .with_state(profile)
        .layer(
            cors::CorsLayer::new()
                .max_age(Duration::from_secs(86400))
                .allow_origin(cors::Any)
                .allow_methods([Method::GET])
                .allow_headers([header::CONTENT_TYPE]),
        )
}

async fn file_handler(
    Path((project, sha, path)): Path<(Id, Oid, String)>,
    State(profile): State<Arc<Profile>>,
) -> impl IntoResponse {
    let storage = &profile.storage;
    let repo = Repository::open(paths::repository(storage, &project))?;
    let mut response_headers = HeaderMap::new();

    if repo.file(sha, &path)?.content(&repo)?.size() > MAX_BLOB_SIZE {
        return Ok::<_, Error>((StatusCode::PAYLOAD_TOO_LARGE, response_headers, vec![]));
    }

    let blob = repo.blob(sha, &path)?;
    let mime = {
        if let Some(ext) = path.split('.').last() {
            MIMES
                .binary_search_by(|(k, _)| k.cmp(&ext))
                .map(|k| MIMES[k].1)
                .unwrap_or("text; charset=utf-8")
        } else {
            "application/octet-stream"
        }
    };
    response_headers.insert(header::CONTENT_TYPE, mime.parse().unwrap());

    Ok::<_, Error>((StatusCode::OK, response_headers, blob.content().to_owned()))
}

#[cfg(test)]
mod routes {
    use axum::http::StatusCode;

    use crate::test::{self, get, HEAD, RID};

    #[tokio::test]
    async fn test_file_handler() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = test::seed(tmp.path());
        let app = super::router(ctx.profile().to_owned());

        let response = get(&app, format!("/{RID}/{HEAD}/dir1/README")).await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.body().await, "Hello World from dir1!\n");
    }
}

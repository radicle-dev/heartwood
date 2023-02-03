#![allow(clippy::type_complexity)]
#![allow(clippy::too_many_arguments)]
pub mod error;

use std::collections::HashMap;
use std::io::prelude::*;
use std::net::SocketAddr;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Duration;
use std::{io, net, str};

use anyhow::Context as _;
use axum::body::Body;
use axum::body::{BoxBody, Bytes};
use axum::extract::{ConnectInfo, Path as AxumPath, RawQuery};
use axum::http::header::HeaderName;
use axum::http::HeaderMap;
use axum::http::{Method, StatusCode};
use axum::http::{Request, Response};
use axum::response::IntoResponse;
use axum::routing::any;
use axum::{Extension, Router};
use flate2::write::GzDecoder;
use hyper::body::Buf as _;
use tower_http::trace::TraceLayer;
use tracing::Span;

use radicle::identity::Id;
use radicle::profile::Profile;

use error::Error;

mod api;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone)]
pub struct Options {
    pub listen: net::SocketAddr,
}

/// Run the Server.
pub async fn run(options: Options) -> anyhow::Result<()> {
    let git_version = Command::new("git")
        .arg("version")
        .output()
        .context("'git' command must be available")?
        .stdout;
    tracing::info!("{}", str::from_utf8(&git_version)?.trim());

    let profile = Arc::new(radicle::Profile::load()?);
    tracing::info!("using radicle home at {}", profile.home().display());

    let git_router = Router::new()
        .route("/:project/*request", any(git_handler))
        .layer(Extension(profile.clone()));

    let ctx = api::Context::new(profile);
    let api_router = api::router(ctx);

    tracing::info!("listening on http://{}", options.listen);

    let app = Router::new()
        .merge(git_router)
        .nest("/api", api_router)
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(|request: &Request<Body>| {
                    tracing::info_span!(
                        "request",
                        method = %request.method(),
                        uri = %request.uri(),
                        status = tracing::field::Empty,
                        latency = tracing::field::Empty,
                    )
                })
                .on_response(
                    |response: &Response<BoxBody>, latency: Duration, span: &Span| {
                        span.record("status", &tracing::field::debug(response.status()));
                        span.record("latency", &tracing::field::debug(latency));

                        tracing::info!("Processed");
                    },
                ),
        )
        .into_make_service_with_connect_info::<SocketAddr>();

    axum::Server::bind(&options.listen)
        .serve(app)
        .await
        .map_err(anyhow::Error::from)
}

async fn git_handler(
    Extension(profile): Extension<Arc<Profile>>,
    AxumPath((project, request)): AxumPath<(String, String)>,
    method: Method,
    headers: HeaderMap,
    ConnectInfo(remote): ConnectInfo<SocketAddr>,
    query: RawQuery,
    body: Bytes,
) -> impl IntoResponse {
    let query = query.0.unwrap_or_default();
    let id: Id = project.strip_suffix(".git").unwrap_or(&project).parse()?;

    let (status, headers, body) =
        git_http_backend(&profile, method, headers, body, remote, id, &request, query).await?;

    let mut response_headers = HeaderMap::new();
    for (name, vec) in headers.iter() {
        for value in vec {
            let header: HeaderName = name.try_into()?;
            response_headers.insert(header, value.parse()?);
        }
    }

    Ok::<_, Error>((status, response_headers, body))
}

async fn git_http_backend(
    profile: &Profile,
    method: Method,
    headers: HeaderMap,
    mut body: Bytes,
    remote: net::SocketAddr,
    id: Id,
    path: &str,
    query: String,
) -> Result<(StatusCode, HashMap<String, Vec<String>>, Vec<u8>), Error> {
    let git_dir = radicle::storage::git::paths::repository(&profile.storage, &id);
    let content_type =
        if let Some(Ok(content_type)) = headers.get("Content-Type").map(|h| h.to_str()) {
            content_type
        } else {
            ""
        };

    // Reject push requests.
    match (path, query.as_str()) {
        ("git-receive-pack", _) | (_, "service=git-receive-pack") => {
            return Err(Error::ServiceUnavailable("git-receive-pack"));
        }
        _ => {}
    };

    tracing::debug!("id: {:?}", id);
    tracing::debug!("headers: {:?}", headers);
    tracing::debug!("path: {:?}", path);
    tracing::debug!("method: {:?}", method.as_str());
    tracing::debug!("remote: {:?}", remote.to_string());

    let mut cmd = Command::new("git");
    let mut child = cmd
        .arg("http-backend")
        .env("REQUEST_METHOD", method.as_str())
        .env("GIT_PROJECT_ROOT", git_dir)
        // "The GIT_HTTP_EXPORT_ALL environmental variable may be passed to git-http-backend to bypass
        // the check for the "git-daemon-export-ok" file in each repository before allowing export of
        // that repository."
        .env("GIT_HTTP_EXPORT_ALL", String::default())
        .env("PATH_INFO", Path::new("/").join(path))
        .env("CONTENT_TYPE", content_type)
        .env("QUERY_STRING", query)
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .stdin(Stdio::piped())
        .spawn()?;

    // Whether the request body is compressed.
    let gzip = matches!(
        headers.get("Content-Encoding").map(|h| h.to_str()),
        Some(Ok("gzip"))
    );

    {
        // This is safe because we captured the child's stdin.
        let mut stdin = child.stdin.take().unwrap();

        // Copy the request body to git-http-backend's stdin.
        if gzip {
            let mut decoder = GzDecoder::new(&mut stdin);
            let mut reader = body.reader();

            io::copy(&mut reader, &mut decoder)?;
            decoder.finish()?;
        } else {
            while body.has_remaining() {
                let mut chunk = body.chunk();
                let count = chunk.len();

                io::copy(&mut chunk, &mut stdin)?;
                body.advance(count);
            }
        }
    }

    match child.wait_with_output() {
        Ok(output) if output.status.success() => {
            tracing::info!("git-http-backend: exited successfully for {}", id);

            let mut reader = std::io::Cursor::new(output.stdout);
            let mut headers = HashMap::new();

            // Parse headers returned by git so that we can use them in the client response.
            for line in io::Read::by_ref(&mut reader).lines() {
                let line = line?;

                if line.is_empty() || line == "\r" {
                    break;
                }

                let mut parts = line.splitn(2, ':');
                let key = parts.next();
                let value = parts.next();

                if let (Some(key), Some(value)) = (key, value) {
                    let value = &value[1..];

                    headers
                        .entry(key.to_string())
                        .or_insert_with(Vec::new)
                        .push(value.to_string());
                } else {
                    return Err(Error::Backend);
                }
            }

            let status = {
                tracing::debug!("git-http-backend: {:?}", &headers);

                let line = headers.remove("Status").unwrap_or_default();
                let line = line.into_iter().next().unwrap_or_default();
                let mut parts = line.split(' ');

                parts
                    .next()
                    .and_then(|p| p.parse().ok())
                    .unwrap_or(StatusCode::OK)
            };

            let position = reader.position() as usize;
            let body = reader.into_inner().split_off(position);

            Ok((status, headers, body))
        }
        Ok(output) => {
            tracing::error!("git-http-backend: exited with code {}", output.status);

            if let Ok(output) = std::str::from_utf8(&output.stderr) {
                tracing::error!("git-http-backend: stderr: {}", output.trim_end());
            }
            Err(Error::Backend)
        }
        Err(err) => {
            panic!("failed to wait for git-http-backend: {err}");
        }
    }
}

use std::collections::HashMap;
use std::io::prelude::*;
use std::net::SocketAddr;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::{io, net, str};

use axum::body::Bytes;
use axum::extract::{ConnectInfo, Path as AxumPath, RawQuery, State};
use axum::http::header::HeaderName;
use axum::http::{HeaderMap, Method, StatusCode};
use axum::response::IntoResponse;
use axum::routing::any;
use axum::Router;
use flate2::write::GzDecoder;
use hyper::body::Buf as _;

use radicle::identity::Id;
use radicle::profile::Profile;

use crate::error::Error;

pub fn router(profile: Arc<Profile>) -> Router {
    Router::new()
        .route("/:project/*request", any(git_handler))
        .with_state(profile)
}

async fn git_handler(
    State(profile): State<Arc<Profile>>,
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

#[cfg(test)]
mod routes {
    use std::net::SocketAddr;

    use axum::body::Body;
    use axum::extract::ConnectInfo;
    use axum::http::Request;
    use axum::http::{Method, StatusCode};
    use tower::ServiceExt;

    use crate::api::test;

    #[tokio::test]
    async fn test_invalid_route_returns_404() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = test::seed(tmp.path());
        let app = super::router(ctx.profile().to_owned());

        let request = Request::builder()
            .method(Method::GET)
            .uri("/aa/a".to_string());

        let mut request = request.body(Body::empty()).unwrap();
        let socket_addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        request.extensions_mut().insert(ConnectInfo(socket_addr));

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}

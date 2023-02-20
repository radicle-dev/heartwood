use core::pin::Pin;
use core::task::{Context, Poll};
use futures_core::Stream;
use radicle_surf::blob::BlobRef;
use std::fs::File;
use std::io::{Read, Seek, Write};
use std::sync::Arc;

use axum::body::StreamBody;
use axum::extract::State;
use axum::http::header;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use hyper::HeaderMap;

use radicle::prelude::Id;
use radicle::profile::Profile;
use radicle::storage::git::paths;
use radicle_surf::{blob::Blob, Oid, Repository};

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

    let response_body = blob_body(blob)?;
    Ok::<_, Error>((response_headers, StreamBody::new(response_body)))
}

/// An enum to support both one-shot Vec and streaming.
pub(crate) enum BlobBody {
    Bytes(Vec<u8>),
    Stream(FileStream),
}

impl Stream for BlobBody {
    type Item = std::io::Result<Vec<u8>>;

    fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let me = Pin::into_inner(self);

        match me {
            BlobBody::Bytes(v) => {
                if v.is_empty() {
                    Poll::Ready(None)
                } else {
                    let drain: Vec<_> = v.drain(..).collect();
                    Poll::Ready(Some(Ok(drain)))
                }
            }
            BlobBody::Stream(s) => {
                let mut buf = vec![0u8; s.chunk_sz];
                match s.file.read(&mut buf) {
                    Ok(sz) => {
                        if sz > 0 {
                            buf.truncate(sz);
                            Poll::Ready(Some(Ok(buf)))
                        } else {
                            Poll::Ready(None)
                        }
                    }
                    Err(e) => Poll::Ready(Some(Err(e))),
                }
            }
        }
    }
}

const BLOB_STREAM_MIN_BYTES: usize = 4096000;
const BLOB_STREAM_CHUNK_SIZE: usize = 4096;

/// Creates a `BlobBody` that supports streaming.
fn blob_body(blob: Blob<BlobRef>) -> Result<BlobBody, Error> {
    if blob.size() < BLOB_STREAM_MIN_BYTES {
        Ok(BlobBody::Bytes(blob.content().to_owned()))
    } else {
        let stream = file_stream(blob.content(), BLOB_STREAM_CHUNK_SIZE)?;
        Ok(BlobBody::Stream(stream))
    }
}

/// Creates a [`FileStream`] from `blob` backed by a tmp file.
///
/// `blob` can be freed after this function. The backing tmp file will be
/// deleted automatically after the returned `FileStream` is dropped.
pub(crate) fn file_stream(blob: &[u8], chunk_sz: usize) -> Result<FileStream, Error> {
    let mut file = tempfile::tempfile()?;
    file.write_all(blob)?;
    file.rewind()?;

    Ok(FileStream::new(file, chunk_sz))
}

/// Represents a read stream of a file.
pub(crate) struct FileStream {
    file: File,
    chunk_sz: usize,
}

impl FileStream {
    /// Creates a read stream of a file.
    pub fn new(file: File, chunk_sz: usize) -> Self {
        Self { file, chunk_sz }
    }
}

#[cfg(test)]
mod routes {
    use axum::http::StatusCode;
    use futures_util::StreamExt;

    use crate::raw::{file_stream, BlobBody};
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

    #[tokio::test]
    async fn test_blob_body() {
        let blob = "This is a test blob"; // 19 chars.
        let chunk_size = 10;

        // Create a file stream and verify the first chunk.
        let file_stream = file_stream(blob.as_bytes(), chunk_size).unwrap();
        let mut file_body = BlobBody::Stream(file_stream);
        let first_chunk = file_body.next().await.unwrap();
        assert!(first_chunk.is_ok());
        let first_chunk = first_chunk.unwrap();
        assert_eq!(first_chunk.len(), chunk_size);

        // Verify the second chunk.
        let second_chunk = file_body.next().await.unwrap();
        assert!(second_chunk.is_ok());
        let second_chunk = second_chunk.unwrap();
        assert_eq!(second_chunk.len(), 9);

        // Verify no more chunks.
        let third_chunk = file_body.next().await;
        assert!(third_chunk.is_none());

        // Create an one-shot Vec for `BlobBody` and verify the first chunk.
        let mut blob_body = BlobBody::Bytes(blob.as_bytes().to_vec());
        let first_chunk = blob_body.next().await.unwrap();
        assert!(first_chunk.is_ok());
        let first_chunk = first_chunk.unwrap();
        assert_eq!(first_chunk.len(), 19); // all chars in one-shot.

        // Verify no more chunks.
        let second_chunk = blob_body.next().await;
        assert!(second_chunk.is_none());
    }
}

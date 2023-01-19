//! Utilities for building JSON responses of our API.

use std::path::Path;

use serde::Serialize;
use serde_json::{json, Value};

use radicle::cob::issue::{Issue, IssueId};
use radicle::cob::patch::{Patch, PatchId};
use radicle::cob::thread::{self, CommentId};
use radicle::cob::Timestamp;
use radicle::identity::PublicKey;
use radicle_surf::blob::Blob;
use radicle_surf::tree::Tree;
use radicle_surf::{Commit, Stats};

use crate::api::auth::Session;

/// Returns JSON of a commit.
pub(crate) fn commit(commit: &Commit) -> Value {
    json!({
      "id": commit.id,
      "author": {
        "name": commit.author.name,
        "email": commit.author.email
      },
      "summary": commit.summary,
      "description": commit.description(),
      "committer": {
        "name": commit.committer.name,
        "email": commit.committer.email,
        "time": commit.committer.time.seconds()
      }
    })
}

/// Returns JSON of a session.
pub(crate) fn session(session_id: String, session: &Session) -> Value {
    json!({
      "sessionId": session_id,
      "status": session.status,
      "publicKey": session.public_key,
      "issuedAt": session.issued_at.unix_timestamp(),
      "expiresAt": session.expires_at.unix_timestamp()
    })
}

/// Returns JSON for a blob with a given `path`.
pub(crate) fn blob(blob: &Blob, path: &str) -> Value {
    json!({
        "binary": blob.is_binary(),
        "content": blob.content(),
        "name": name_in_path(path),
        "path": path,
        "lastCommit": commit(blob.commit())
    })
}

/// Returns JSON for a tree with a given `path` and `stats`.
pub(crate) fn tree(tree: &Tree, path: &str, stats: &Stats) -> Value {
    let prefix = Path::new(path);
    let entries = tree
        .entries()
        .iter()
        .map(|entry| {
            json!({
                "path": prefix.join(entry.name()),
                "name": entry.name(),
                "kind": if entry.is_tree() { "tree" } else { "blob" },
            })
        })
        .collect::<Vec<_>>();

    json!({
        "entries": &entries,
        "lastCommit": commit(tree.commit()),
        "name": name_in_path(path),
        "path": path,
        "stats": stats,
    })
}

/// Returns JSON for an `issue`.
pub(crate) fn issue(id: IssueId, issue: Issue) -> Value {
    json!({
        "id": id.to_string(),
        "author": issue.author(),
        "assignees": issue.assigned().collect::<Vec<_>>(),
        "title": issue.title(),
        "state": issue.state(),
        "discussion": issue.comments().collect::<Comments>(),
        "tags": issue.tags().collect::<Vec<_>>(),
    })
}

/// Returns JSON for a `patch`.
pub(crate) fn patch(id: PatchId, patch: Patch) -> Value {
    json!({
        "id": id.to_string(),
        "author": patch.author(),
        "title": patch.title(),
        "description": patch.description(),
        "state": patch.state(),
        "target": patch.target(),
        "tags": patch.tags().collect::<Vec<_>>(),
        "revisions": patch.revisions().map(|(id, rev)| {
            json!({
                "id": id,
                "description": rev.description(),
                "reviews": rev.reviews().collect::<Vec<_>>(),
            })
        }).collect::<Vec<_>>(),
    })
}

/// Returns the name part of a path string.
fn name_in_path(path: &str) -> &str {
    match path.rsplit('/').next() {
        Some(name) => name,
        None => path,
    }
}

#[derive(Serialize)]
struct Author {
    id: PublicKey,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Comment {
    author: Author,
    body: String,
    reactions: [String; 0],
    timestamp: Timestamp,
    reply_to: Option<CommentId>,
}

#[derive(Serialize)]
struct Comments(Vec<Comment>);

impl<'a> FromIterator<(&'a CommentId, &'a thread::Comment)> for Comments {
    fn from_iter<I: IntoIterator<Item = (&'a CommentId, &'a thread::Comment)>>(iter: I) -> Self {
        let mut comments = Vec::new();

        for (_, comment) in iter {
            comments.push(Comment {
                author: Author {
                    id: comment.author(),
                },
                body: comment.body().to_owned(),
                reactions: [],
                timestamp: comment.timestamp(),
                reply_to: comment.reply_to(),
            });
        }

        Comments(comments)
    }
}

//! Utilities for building JSON responses of our API.

use std::path::Path;
use std::str;

use serde::Serialize;
use serde_json::{json, Value};

use radicle::cob::issue::{Issue, IssueId};
use radicle::cob::patch::{Patch, PatchId};
use radicle::cob::thread::{self, CommentId};
use radicle::cob::{ActorId, Author, Timestamp};
use radicle::git::RefString;
use radicle::storage::{git, refs, ReadRepository};
use radicle_surf::blob::Blob;
use radicle_surf::tree::Tree;
use radicle_surf::{Commit, Oid, Stats};

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
pub(crate) fn blob<T: AsRef<[u8]>>(blob: &Blob<T>, path: &str) -> Value {
    let mut response = json!({
        "binary": blob.is_binary(),
        "name": name_in_path(path),
        "path": path,
        "lastCommit": commit(blob.commit())
    });

    if !blob.is_binary() {
        match str::from_utf8(blob.content()) {
            Ok(content) => response["content"] = content.into(),
            Err(err) => return json!({ "error": err.to_string() }),
        }
    }

    response
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
        "title": issue.title(),
        "state": issue.state(),
        "assignees": issue.assigned().collect::<Vec<_>>(),
        "discussion": issue.comments().collect::<Comments>(),
        "tags": issue.tags().collect::<Vec<_>>(),
    })
}

/// Returns JSON for a `patch`.
pub(crate) fn patch(id: PatchId, patch: Patch, repo: &git::Repository) -> Value {
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
                "base": rev.base,
                "oid": rev.oid,
                "refs": get_refs(repo, patch.author().id(), &rev.oid).unwrap_or(vec![]),
                "merges": rev.merges().collect::<Vec<_>>(),
                "discussions": rev.discussion.comments().collect::<Comments>(),
                "timestamp": rev.timestamp,
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

fn get_refs(
    repo: &git::Repository,
    id: &ActorId,
    head: &Oid,
) -> Result<Vec<RefString>, refs::Error> {
    let remote = repo.remote(id)?;
    let refs = remote
        .refs
        .iter()
        .filter_map(|(name, o)| {
            if o == head {
                Some(name.to_owned())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    Ok(refs)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Comment {
    id: CommentId,
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

        for (id, comment) in iter {
            comments.push(Comment {
                id: id.to_owned(),
                author: comment.author().into(),
                body: comment.body().to_owned(),
                reactions: [],
                timestamp: comment.timestamp(),
                reply_to: comment.reply_to(),
            });
        }

        Comments(comments)
    }
}

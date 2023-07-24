//! Utilities for building JSON responses of our API.

use std::path::Path;
use std::str;

use serde::Serialize;
use serde_json::{json, Value};

use radicle::cob::issue::{Issue, IssueId};
use radicle::cob::patch::Merge;
use radicle::cob::patch::Review;
use radicle::cob::patch::{Patch, PatchId};
use radicle::cob::thread;
use radicle::cob::thread::CommentId;
use radicle::cob::{ActorId, Author, Reaction, Timestamp};
use radicle::git::RefString;
use radicle::node::{Alias, AliasStore};
use radicle::prelude::NodeId;
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
      "parents": commit.parents,
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
pub(crate) fn issue(id: IssueId, issue: Issue, aliases: &impl AliasStore) -> Value {
    json!({
        "id": id.to_string(),
        "author": author(&issue.author(), aliases.alias(issue.author().id())),
        "title": issue.title(),
        "state": issue.state(),
        "assignees": issue.assigned().collect::<Vec<_>>(),
        "discussion": issue
          .comments()
          .map(|(id, comment)| Comment::new(id, comment, aliases))
          .collect::<Vec<_>>(),
        "tags": issue.tags().collect::<Vec<_>>(),
    })
}

/// Returns JSON for a `patch`.
pub(crate) fn patch(
    id: PatchId,
    patch: Patch,
    repo: &git::Repository,
    aliases: &impl AliasStore,
) -> Value {
    json!({
        "id": id.to_string(),
        "author": author(patch.author(), aliases.alias(patch.author().id())),
        "title": patch.title(),
        "state": patch.state(),
        "target": patch.target(),
        "tags": patch.tags().collect::<Vec<_>>(),
        "merges": patch.merges().map(|(nid, m)| merge(m, nid, aliases.alias(nid))).collect::<Vec<_>>(),
        "reviewers": patch.reviewers().collect::<Vec<_>>(),
        "revisions": patch.revisions().map(|(id, rev)| {
            json!({
                "id": id,
                "author": author(rev.author(), aliases.alias(rev.author().id())),
                "description": rev.description(),
                "base": rev.base(),
                "oid": rev.head(),
                "refs": get_refs(repo, patch.author().id(), &rev.head()).unwrap_or(vec![]),
                "discussions": rev.discussion().comments()
                  .map(|(id, comment)| Comment::new(id, comment,  aliases))
                  .collect::<Vec<_>>(),
                "timestamp": rev.timestamp().as_secs().to_string(),
                "reviews": rev.reviews().map(|(nid, _review)| review(nid, aliases.alias(nid), _review)).collect::<Vec<_>>(),
            })
        }).collect::<Vec<_>>(),
    })
}

/// Returns JSON for an `author` and fills in `alias` when present.
fn author(author: &Author, alias: Option<Alias>) -> Value {
    match alias {
        Some(alias) => json!({
            "id": author.id,
            "alias": alias,
        }),
        None => json!(author),
    }
}

/// Returns JSON for a patch `Merge` and fills in `alias` when present.
fn merge(merge: &Merge, nid: &NodeId, alias: Option<Alias>) -> Value {
    match alias {
        Some(alias) => json!({
            "author": {
                "id": nid,
                "alias": alias,
            },
            "commit": merge.commit,
            "timestamp": merge.timestamp.as_secs().to_string(),
            "revision": merge.revision,
        }),
        None => json!({
            "author": {
                "id": nid,
            },
            "commit": merge.commit,
            "timestamp": merge.timestamp.as_secs().to_string(),
            "revision": merge.revision,
        }),
    }
}

/// Returns JSON for a patch `Review` and fills in `alias` when present.
fn review(nid: &NodeId, alias: Option<Alias>, review: &Review) -> Value {
    match alias {
        Some(alias) => json!({
            "author": {
                "id": nid,
                "alias": alias,
            },
            "verdict": review.verdict(),
            "summary": review.summary(),
            "comments": review.comments().collect::<Vec<_>>(),
            "timestamp": review.timestamp().as_secs().to_string(),
        }),
        None => json!({
            "author": {
                "id": nid,
            },
            "verdict": review.verdict(),
            "summary": review.summary(),
            "comments": review.comments().collect::<Vec<_>>(),
            "timestamp": review.timestamp().as_secs().to_string(),
        }),
    }
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
struct Comment<'a> {
    id: CommentId,
    author: Value,
    body: &'a str,
    reactions: Vec<(&'a ActorId, &'a Reaction)>,
    #[serde(with = "radicle::serde_ext::localtime::time")]
    timestamp: Timestamp,
    reply_to: Option<CommentId>,
}

impl<'a> Comment<'a> {
    fn new(id: &'a CommentId, comment: &'a thread::Comment, aliases: &impl AliasStore) -> Self {
        let comment_author = Author::new(comment.author());
        Self {
            id: *id,
            author: author(&comment_author, aliases.alias(comment_author.id())),
            body: comment.body(),
            reactions: comment.reactions().collect::<Vec<_>>(),
            timestamp: comment.timestamp(),
            reply_to: comment.reply_to(),
        }
    }
}

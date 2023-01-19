//! Utilities for building JSON responses of our API.

use radicle_surf::{
    object::{Blob, Tree},
    Commit, Stats,
};
use serde_json::json;

/// Returns JSON of a commit.
pub(crate) fn commit(commit: &Commit) -> serde_json::Value {
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

/// Returns JSON for a blob with a given `path`.
pub(crate) fn blob(blob: &Blob, path: &str) -> serde_json::Value {
    json!({
        "binary": blob.is_binary(),
        "content": blob.content(),
        "name": name_in_path(path),
        "path": path,
        "lastCommit": commit(blob.commit())
    })
}

/// Returns JSON for a tree with a given `path` and `stats`.
pub(crate) fn tree(tree: &Tree, path: &str, stats: &Stats) -> serde_json::Value {
    let prefix = std::path::Path::new(path);
    let entries = tree
        .entries()
        .iter()
        .map(|entry| {
            json!({
                "path": prefix.join(entry.name()),
                "name": entry.name(),
                "lastCommit": serde_json::Value::Null,
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

/// Returns the name part of a path string.
fn name_in_path(path: &str) -> &str {
    match path.rsplit('/').next() {
        Some(name) => name,
        None => path,
    }
}

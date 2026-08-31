//! Bidirectional synchronization between internal Radicle issues and Markdown
//! files.
//!
//! The command is split into focused submodules:
//!
//! - [`markdown`] owns the canonical file format (rendering and parsing).
//! - [`export`] writes internal issues out as Markdown files.
//! - [`import`] applies Markdown files back into internal issue state.
//! - [`id_map`] persists external-id to object-id mappings between imports.

mod export;
mod id_map;
mod import;
mod markdown;
#[cfg(test)]
mod test_support;

pub(super) use self::export::export;
pub(super) use self::import::import;

use std::io::Write as _;
use std::path::{Path, PathBuf};

use radicle::cob;
use radicle::cob::issue;
use radicle::cob::store::access::WriteAs;
use radicle::storage;

/// Typed handle for the issue cache shared by export and import.
pub(super) type IssueCache<'a, S> =
    issue::Cache<'a, storage::git::Repository, WriteAs<'a, S>, cob::cache::StoreWriter>;

#[derive(Debug, Clone)]
pub(super) struct ExportOptions {
    pub(super) path: Option<PathBuf>,
    pub(super) dry_run: bool,
}

#[derive(Debug, Clone)]
pub(super) struct ImportOptions {
    pub(super) path: Option<PathBuf>,
    pub(super) dry_run: bool,
    pub(super) force: bool,
}

#[derive(Debug, Default)]
struct Summary {
    changed: usize,
    unchanged: usize,
    conflicted: usize,
    failed: usize,
}

pub(super) fn resolve_issue_dir(
    repo_root: &Path,
    configured_dir: &Path,
    cli_dir: Option<&Path>,
) -> anyhow::Result<PathBuf> {
    let relative = cli_dir.unwrap_or(configured_dir);
    if relative.is_absolute() {
        anyhow::bail!(
            "absolute issue directory paths are not supported in v1: '{}'",
            relative.display()
        );
    }
    Ok(repo_root.join(relative))
}

fn write_atomic(path: &Path, content: &str) -> anyhow::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("path '{}' has no parent", path.display()))?;
    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;

    tmp.write_all(content.as_bytes())?;
    tmp.flush()?;
    tmp.persist(path).map_err(|err| err.error)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::resolve_issue_dir;

    #[test]
    fn resolve_issue_dir_uses_default_relative_path() {
        let root = Path::new("/tmp/repo");
        let resolved = resolve_issue_dir(root, Path::new("issues"), None).unwrap();

        assert_eq!(resolved, PathBuf::from("/tmp/repo/issues"));
    }

    #[test]
    fn resolve_issue_dir_uses_cli_override() {
        let root = Path::new("/tmp/repo");
        let resolved =
            resolve_issue_dir(root, Path::new("issues"), Some(Path::new("meta/issues"))).unwrap();

        assert_eq!(resolved, PathBuf::from("/tmp/repo/meta/issues"));
    }

    #[test]
    fn resolve_issue_dir_rejects_absolute_path() {
        let root = Path::new("/tmp/repo");
        let err = resolve_issue_dir(root, Path::new("/var/issues"), None).unwrap_err();

        assert!(
            err.to_string()
                .contains("absolute issue directory paths are not supported"),
            "{err:?}"
        );
    }
}

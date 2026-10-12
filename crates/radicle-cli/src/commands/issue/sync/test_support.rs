//! Shared fixtures for the issue sync submodule tests.

use std::fs;
use std::path::{Path, PathBuf};

use radicle::Profile;
use radicle::crypto::Seed;
use radicle::issue::cache::Issues as _;
use radicle::node::Alias;
use radicle::profile::{Home, Signer};
use radicle::storage::{self, ReadStorage as _};
use radicle::test::fixtures;

use super::markdown::{COMMENT_CLOSE_MARKER, COMMENT_OPEN_MARKER};
use super::{ExportOptions, ImportOptions, IssueCache};
use crate::terminal as term;

pub(super) const ISSUES_DIR: &str = "issues";

pub(super) struct Workspace {
    _tmp: tempfile::TempDir,
    pub(super) profile: Profile,
    pub(super) signer: Signer,
    pub(super) repo: storage::git::Repository,
    pub(super) repo_root: PathBuf,
}

impl Workspace {
    pub(super) fn new(alias: &'static str, seed_byte: u8) -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let home = Home::new(tmp.path().join("home")).unwrap();
        let profile =
            Profile::init(home, Alias::new(alias), None, Seed::new([seed_byte; 32])).unwrap();
        let signer = profile.signer().unwrap();
        let working = tmp.path().join("working");
        let (rid, _, _, _) = fixtures::project(&working, &profile.storage, &signer).unwrap();
        let repo = profile.storage.repository(rid).unwrap();

        Self {
            _tmp: tmp,
            profile,
            signer,
            repo,
            repo_root: working,
        }
    }

    pub(super) fn issues(&self) -> IssueCache<'_, Signer> {
        term::cob::issues_mut(&self.profile, &self.repo, &self.signer).unwrap()
    }

    pub(super) fn issue_dir(&self) -> PathBuf {
        self.repo_root.join(ISSUES_DIR)
    }

    pub(super) fn owner_dir(&self) -> PathBuf {
        self.issue_dir().join("thyseus")
    }

    pub(super) fn exported_files(&self) -> Vec<String> {
        let mut names = fs::read_dir(self.owner_dir())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().to_string())
            .collect::<Vec<_>>();
        names.sort();
        names
    }

    pub(super) fn internal_issue_count(&self) -> usize {
        let issues = self.issues();
        issues
            .list()
            .unwrap()
            .map(|entry| entry.is_ok())
            .filter(|ok| *ok)
            .count()
    }
}

pub(super) fn write_markdown_file(
    root: &Path,
    segments: &[&str],
    id: &str,
    title: &str,
) -> PathBuf {
    write_markdown_file_with_comments(root, segments, id, title, &[])
}

pub(super) fn write_markdown_file_with_comments(
    root: &Path,
    segments: &[&str],
    id: &str,
    title: &str,
    comments: &[&str],
) -> PathBuf {
    let path = segments
        .iter()
        .fold(root.to_path_buf(), |acc, s| acc.join(s));
    fs::create_dir_all(path.parent().unwrap()).unwrap();

    let mut contents = format!(
        "---\n\
        id: \"{id}\"\n\
        title: \"{title}\"\n\
        state: \"open\"\n\
        author: \"did:key:z6Mktest\"\n\
        assignees: []\n\
        labels: []\n\
        created: \"2026-01-01T00:00:00+00:00\"\n\
        updated: \"2026-01-01T00:00:00+00:00\"\n\
        ---\n\n\
        Imported body\n"
    );

    if !comments.is_empty() {
        contents.push_str("\n## Comments\n\n");
        for body in comments {
            contents.push_str(&format!(
                "{COMMENT_OPEN_MARKER}\n{body}\n{COMMENT_CLOSE_MARKER}\n\n"
            ));
        }
    }

    fs::write(&path, contents).unwrap();
    path
}

pub(super) fn export_options() -> ExportOptions {
    ExportOptions {
        path: None,
        dry_run: false,
    }
}

pub(super) fn import_options(force: bool) -> ImportOptions {
    ImportOptions {
        path: None,
        dry_run: false,
        force,
    }
}

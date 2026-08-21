//! Export internal issues to Markdown files under the issue directory.

use std::fs;
use std::io;
use std::path::Path;

use anyhow::Context as _;

use radicle::Profile;
use radicle::cob::issue;
use radicle::crypto;
use radicle::issue::cache::Issues as _;
use radicle::node::AliasStore as _;

use crate::terminal as term;

use super::IssueCache;
use super::markdown::MarkdownIssue;
use super::{ExportOptions, Summary, resolve_issue_dir, write_atomic};

pub(in crate::commands::issue) fn export<S: crypto::Signer>(
    profile: &Profile,
    repo_root: &Path,
    configured_dir: &Path,
    options: ExportOptions,
    issues: &IssueCache<'_, S>,
) -> anyhow::Result<()> {
    let issue_dir = resolve_issue_dir(repo_root, configured_dir, options.path.as_deref())?;
    if !options.dry_run {
        fs::create_dir_all(&issue_dir).with_context(|| {
            format!(
                "failed to create issue export directory '{}'",
                issue_dir.display()
            )
        })?;
    }

    let mut all = Vec::new();
    let mut summary = Summary::default();

    for entry in issues.list()? {
        match entry {
            Ok(entry) => all.push(entry),
            Err(err) => {
                summary.failed += 1;
                term::warning(format!("failed to load issue for export: {err}"));
            }
        }
    }

    all.sort_by(|(left, _), (right, _)| left.cmp(right));

    for (id, issue) in all {
        let markdown = MarkdownIssue::from_issue(&id, &issue);
        let owner = owner_directory_name(profile, &issue);
        let file = issue_dir.join(owner).join(markdown.file_name());
        let rendered = markdown.render();

        match fs::read_to_string(&file) {
            Ok(existing) if existing == rendered => {
                summary.unchanged += 1;
            }
            Ok(_) => {
                summary.conflicted += 1;
                term::warning(format!(
                    "conflict: markdown file '{}' diverges from internal issue state",
                    file.display()
                ));
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                if !options.dry_run {
                    if let Some(parent) = file.parent() {
                        if let Err(err) = fs::create_dir_all(parent) {
                            summary.failed += 1;
                            term::warning(format!(
                                "failed to create export directory '{}': {err}",
                                parent.display()
                            ));
                            continue;
                        }
                    }
                    if let Err(err) = write_atomic(&file, &rendered) {
                        summary.failed += 1;
                        term::warning(format!("failed to write '{}': {err}", file.display()));
                        continue;
                    }
                }
                summary.changed += 1;
            }
            Err(err) => {
                summary.failed += 1;
                term::warning(format!("failed to read '{}': {err}", file.display()));
            }
        }
    }

    term::info!(
        "Export summary: exported={} unchanged={} conflicted={} failed={}",
        summary.changed,
        summary.unchanged,
        summary.conflicted,
        summary.failed,
    );

    if summary.conflicted > 0 || summary.failed > 0 {
        anyhow::bail!("export completed with conflicts or failures");
    }
    Ok(())
}

fn owner_directory_name(profile: &Profile, issue: &issue::Issue) -> String {
    let author = issue.author();
    let id = author.id;

    let owner = if id == profile.did() {
        profile.config.alias().to_string()
    } else {
        profile
            .alias(id.as_key())
            .map(|alias| alias.to_string())
            .unwrap_or_else(|| id.to_string())
    };

    slugify_owner(owner.as_str())
}

fn slugify_owner(owner: &str) -> String {
    let mut slug = String::with_capacity(owner.len());
    let mut prev_hyphen = false;

    for c in owner.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
            prev_hyphen = false;
        } else if !slug.is_empty() && !prev_hyphen {
            slug.push('-');
            prev_hyphen = true;
        }
    }

    while slug.ends_with('-') {
        slug.pop();
    }

    if slug.is_empty() {
        "unknown-owner".to_owned()
    } else {
        slug
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use radicle::cob::Title;

    use super::super::markdown::{COMMENT_CLOSE_MARKER, COMMENT_OPEN_MARKER, COMMENTS_HEADER};
    use super::super::test_support::{ISSUES_DIR, Workspace, export_options};
    use super::{export, slugify_owner};

    #[test]
    fn export_groups_files_by_owner_and_is_idempotent() {
        let ws = Workspace::new("thyseus", 1);
        let mut issues = ws.issues();
        issues
            .create(
                Title::new("My first issue").unwrap(),
                "Body one",
                &[],
                &[],
                [],
            )
            .unwrap();
        issues
            .create(
                Title::new("Second task!").unwrap(),
                "Body two",
                &[],
                &[],
                [],
            )
            .unwrap();

        export(
            &ws.profile,
            &ws.repo_root,
            Path::new(ISSUES_DIR),
            export_options(),
            &issues,
        )
        .unwrap();

        let files = ws.exported_files();
        assert_eq!(files.len(), 2);
        assert!(
            files
                .iter()
                .any(|name| name.ends_with("-my-first-issue.md")),
            "{files:?}"
        );
        assert!(
            files.iter().any(|name| name.ends_with("-second-task.md")),
            "{files:?}"
        );

        export(
            &ws.profile,
            &ws.repo_root,
            Path::new(ISSUES_DIR),
            export_options(),
            &issues,
        )
        .unwrap();
        assert_eq!(ws.exported_files(), files);
    }

    #[test]
    fn export_reports_conflict_and_preserves_divergent_file() {
        let ws = Workspace::new("thyseus", 2);
        let mut issues = ws.issues();
        issues
            .create(
                Title::new("Original title").unwrap(),
                "Original body",
                &[],
                &[],
                [],
            )
            .unwrap();

        export(
            &ws.profile,
            &ws.repo_root,
            Path::new(ISSUES_DIR),
            export_options(),
            &issues,
        )
        .unwrap();

        let file = ws.owner_dir().join(ws.exported_files().remove(0));
        let divergent = fs::read_to_string(&file)
            .unwrap()
            .replace("\"Original title\"", "\"Renamed title\"");
        fs::write(&file, &divergent).unwrap();

        let err = export(
            &ws.profile,
            &ws.repo_root,
            Path::new(ISSUES_DIR),
            export_options(),
            &issues,
        )
        .unwrap_err();
        assert!(err.to_string().contains("conflicts"), "{err:?}");
        assert_eq!(fs::read_to_string(&file).unwrap(), divergent);
    }

    #[test]
    fn export_appends_comments_oldest_first() {
        let ws = Workspace::new("thyseus", 7);
        let mut issues = ws.issues();
        let mut issue = issues
            .create(
                Title::new("Discussed issue").unwrap(),
                "Issue description",
                &[],
                &[],
                [],
            )
            .unwrap();
        let root = *issue.root().0;
        issue.comment("First comment", root, []).unwrap();
        issue.comment("Second comment", root, []).unwrap();
        drop(issue);

        export(
            &ws.profile,
            &ws.repo_root,
            Path::new(ISSUES_DIR),
            export_options(),
            &issues,
        )
        .unwrap();

        let file = ws.owner_dir().join(ws.exported_files().remove(0));
        let raw = fs::read_to_string(&file).unwrap();

        assert!(raw.contains(COMMENTS_HEADER), "{raw}");
        assert_eq!(raw.matches(COMMENT_OPEN_MARKER).count(), 2, "{raw}");
        assert_eq!(raw.matches(COMMENT_CLOSE_MARKER).count(), 2, "{raw}");
        let section = raw.find(COMMENTS_HEADER).unwrap();
        assert!(raw.find("Issue description").unwrap() < section, "{raw}");

        let first = raw.find("First comment").unwrap();
        let second = raw.find("Second comment").unwrap();
        assert!(first < second, "{raw}");

        export(
            &ws.profile,
            &ws.repo_root,
            Path::new(ISSUES_DIR),
            export_options(),
            &issues,
        )
        .unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), raw);
    }

    #[test]
    fn owner_slug_preserves_simple_aliases() {
        assert_eq!(slugify_owner("thyseus"), "thyseus");
    }
}

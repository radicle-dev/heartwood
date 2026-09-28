//! Apply Markdown issue files into internal issue storage.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::Context as _;

use radicle::cob;
use radicle::cob::common::Label;
use radicle::cob::issue::{self, State};
use radicle::crypto;
use radicle::issue::cache::Issues as _;
use radicle::prelude::Did;

use crate::terminal as term;

use super::IssueCache;
use super::id_map::{ID_MAP_FILE_NAME, load_id_map, save_id_map};
use super::markdown::{MarkdownIssue, parse_state};
use super::{ImportOptions, Summary, resolve_issue_dir};

pub(in crate::commands::issue) fn import<S: crypto::Signer>(
    repo_root: &Path,
    configured_dir: &Path,
    options: ImportOptions,
    issues: &mut IssueCache<'_, S>,
) -> anyhow::Result<()> {
    let issue_dir = resolve_issue_dir(repo_root, configured_dir, options.path.as_deref())?;
    if !issue_dir.exists() {
        anyhow::bail!(
            "issue import directory '{}' does not exist",
            issue_dir.display()
        );
    }

    let mut entries = collect_files_recursively(&issue_dir)?;
    entries.sort();

    let mut seen = HashSet::<String>::new();
    let mut id_map = load_id_map(&issue_dir)?;
    let mut id_map_changed = false;
    let mut created_mappings = Vec::<(String, String)>::new();
    let mut summary = Summary::default();

    for path in entries {
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == ID_MAP_FILE_NAME)
        {
            continue;
        }
        let Some(ext) = path.extension().and_then(|ext| ext.to_str()) else {
            summary.failed += 1;
            term::warning(format!("rejected non-markdown file '{}'", path.display()));
            continue;
        };
        if ext != "md" {
            summary.failed += 1;
            term::warning(format!("rejected non-markdown file '{}'", path.display()));
            continue;
        }

        let raw = match fs::read_to_string(&path) {
            Ok(raw) => raw,
            Err(err) => {
                summary.failed += 1;
                term::warning(format!("failed to read '{}': {err}", path.display()));
                continue;
            }
        };

        let parsed = match MarkdownIssue::parse(&path, &raw) {
            Ok(parsed) => parsed,
            Err(err) => {
                summary.failed += 1;
                term::warning(err.to_string());
                continue;
            }
        };

        let external_id = parsed.id.clone();
        if !seen.insert(external_id.clone()) {
            summary.failed += 1;
            term::warning(format!(
                "duplicate issue id '{}' in import files",
                external_id
            ));
            continue;
        }

        let desired = match DesiredIssue::from_markdown(parsed) {
            Ok(desired) => desired,
            Err(err) => {
                summary.failed += 1;
                term::warning(err.to_string());
                continue;
            }
        };

        let resolution = match resolve_internal_issue_id(issues, &id_map, external_id.as_str()) {
            Ok(resolution) => resolution,
            Err(err) => {
                summary.failed += 1;
                term::warning(format!(
                    "failed to resolve internal issue id for '{}': {err}",
                    external_id
                ));
                continue;
            }
        };

        let id = match resolution {
            ResolvedIssueId::Found(id) => Some(id),
            ResolvedIssueId::MissingExternal => None,
            ResolvedIssueId::MissingClaimed(claimed) => {
                if !options.force {
                    summary.conflicted += 1;
                    term::warning(format!(
                        "conflict: markdown id '{}' references Radicle issue {} which is not present locally; replicate the repository's issue data (eg. 'rad sync') or rerun with --force to create a distinct local issue",
                        external_id, claimed
                    ));
                    continue;
                }
                if options.dry_run {
                    summary.changed += 1;
                    term::info!(
                        "Would create distinct local issue for Radicle id '{}' (--force)",
                        external_id
                    );
                    continue;
                }
                None
            }
        };

        let Some(id) = id else {
            if options.dry_run {
                summary.changed += 1;
                continue;
            }

            match create_issue_from_markdown(issues, &desired) {
                Ok(new_id) => {
                    let new_id = new_id.to_string();
                    summary.changed += 1;

                    if new_id != external_id {
                        id_map.insert(external_id.clone(), new_id.clone());
                        id_map_changed = true;
                        created_mappings.push((external_id.clone(), new_id));
                    }
                }
                Err(err) => {
                    summary.failed += 1;
                    term::warning(format!(
                        "failed to create issue from '{}' : {err}",
                        path.display()
                    ));
                }
            }
            continue;
        };

        let Some(current) = issues.get(&id)? else {
            summary.failed += 1;
            term::warning(format!(
                "issue '{}' could not be loaded from internal storage",
                id
            ));
            continue;
        };

        if desired.matches_issue(&current) {
            summary.unchanged += 1;
            continue;
        }

        if !options.force {
            summary.conflicted += 1;
            term::warning(format!(
                "conflict: internal issue '{}' diverges from markdown file id '{}'; rerun with --force to overwrite",
                id, external_id
            ));
            continue;
        }

        if options.dry_run {
            summary.changed += 1;
            continue;
        }

        match apply_issue_updates(issues, &id, &desired) {
            Ok(()) => summary.changed += 1,
            Err(err) => {
                summary.failed += 1;
                term::warning(format!("failed to import issue '{}': {err}", id));
            }
        }
    }

    if !options.dry_run && id_map_changed {
        if let Err(err) = save_id_map(&issue_dir, &id_map) {
            summary.failed += 1;
            term::warning(format!(
                "failed to write issue id mapping file '{}': {err}",
                issue_dir.join(ID_MAP_FILE_NAME).display()
            ));
        }
    }

    for (external, internal) in created_mappings {
        term::info!("Issue ID mapping: '{}' -> '{}'", external, internal);
    }

    term::info!(
        "Import summary: imported={} unchanged={} conflicted={} failed={}",
        summary.changed,
        summary.unchanged,
        summary.conflicted,
        summary.failed,
    );

    if summary.conflicted > 0 || summary.failed > 0 {
        anyhow::bail!("import completed with conflicts or failures");
    }
    Ok(())
}

fn collect_files_recursively(root: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let entries = fs::read_dir(&dir)
            .with_context(|| format!("failed to read issue import directory '{}'", dir.display()))?
            .collect::<Result<Vec<_>, _>>()?;

        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.is_file() {
                files.push(path);
            }
        }
    }

    Ok(files)
}

/// Outcome of resolving a markdown issue id against local state.
#[derive(Debug, Clone, Copy)]
enum ResolvedIssueId {
    /// A local issue exists for this file.
    Found(cob::ObjectId),
    /// No local issue exists, but the file claims the identity of a Radicle
    /// object (either directly or through the id map) that is absent locally.
    /// Creating a new issue would fork that identity.
    MissingClaimed(cob::ObjectId),
    /// No local issue exists and the id is external (not a Radicle object id);
    /// creating a fresh local issue is unambiguous.
    MissingExternal,
}

fn resolve_internal_issue_id<S: crypto::Signer>(
    issues: &IssueCache<'_, S>,
    id_map: &BTreeMap<String, String>,
    external_id: &str,
) -> anyhow::Result<ResolvedIssueId> {
    if let Some(mapped) = id_map.get(external_id) {
        let mapped_id = cob::ObjectId::from_str(mapped).with_context(|| {
            format!(
                "mapping file '{}' contains invalid object id '{}' for external id '{}'",
                ID_MAP_FILE_NAME, mapped, external_id
            )
        })?;
        if issues.get(&mapped_id)?.is_some() {
            return Ok(ResolvedIssueId::Found(mapped_id));
        }

        return Ok(ResolvedIssueId::MissingClaimed(mapped_id));
    }

    match cob::ObjectId::from_str(external_id) {
        Ok(id) if issues.get(&id)?.is_some() => Ok(ResolvedIssueId::Found(id)),
        Ok(id) => Ok(ResolvedIssueId::MissingClaimed(id)),
        Err(_) => Ok(ResolvedIssueId::MissingExternal),
    }
}

fn create_issue_from_markdown<S: crypto::Signer>(
    issues: &mut IssueCache<'_, S>,
    desired: &DesiredIssue,
) -> anyhow::Result<cob::ObjectId> {
    let labels = desired.labels.iter().cloned().collect::<Vec<_>>();
    let assignees = desired.assignees.iter().cloned().collect::<Vec<_>>();

    let mut created = issues.create(
        cob::Title::from_str(desired.title.as_str())?,
        desired.description.as_str(),
        labels.as_slice(),
        assignees.as_slice(),
        [],
    )?;

    if desired.state != State::Open {
        created.lifecycle(desired.state)?;
    }

    let root = *created.root().0;
    for body in &desired.comments {
        created.comment(body.as_str(), root, [])?;
    }

    Ok(*created.id())
}

fn apply_issue_updates<S: crypto::Signer>(
    issues: &mut IssueCache<'_, S>,
    id: &cob::ObjectId,
    desired: &DesiredIssue,
) -> anyhow::Result<()> {
    let mut current = issues.get_mut(id)?;

    if current.title() != desired.title {
        current.edit(cob::Title::from_str(desired.title.as_str())?)?;
    }
    if current.description() != desired.description {
        current.edit_description(desired.description.as_str(), [])?;
    }
    if current.state() != &desired.state {
        current.lifecycle(desired.state)?;
    }

    let current_assignees = current.assignees().cloned().collect::<BTreeSet<_>>();
    if current_assignees != desired.assignees {
        current.assign(desired.assignees.iter().cloned())?;
    }

    let current_labels = current.labels().cloned().collect::<BTreeSet<_>>();
    if current_labels != desired.labels {
        current.label(desired.labels.iter().cloned())?;
    }

    let existing_comments = current
        .comments()
        .skip(1)
        .map(|(_, comment)| comment.body().to_owned())
        .collect::<HashSet<_>>();
    let root = *current.root().0;
    for body in &desired.comments {
        if existing_comments.contains(body) {
            continue;
        }
        current.comment(body.as_str(), root, [])?;
    }

    Ok(())
}

#[derive(Debug, Clone)]
struct DesiredIssue {
    title: String,
    description: String,
    state: State,
    assignees: BTreeSet<Did>,
    labels: BTreeSet<Label>,
    comments: Vec<String>,
}

impl DesiredIssue {
    fn from_markdown(markdown: MarkdownIssue) -> anyhow::Result<Self> {
        let assignees = markdown
            .assignees
            .iter()
            .map(|did| {
                did.parse::<Did>().with_context(|| {
                    format!(
                        "invalid assignee DID '{}' in issue file id '{}'",
                        did, markdown.id
                    )
                })
            })
            .collect::<Result<BTreeSet<_>, _>>()?;

        let labels = markdown
            .labels
            .iter()
            .map(|label| {
                label.parse::<Label>().with_context(|| {
                    format!(
                        "invalid label '{}' in issue file id '{}'",
                        label, markdown.id
                    )
                })
            })
            .collect::<Result<BTreeSet<_>, _>>()?;

        Ok(Self {
            title: markdown.title,
            description: markdown.body,
            state: parse_state(markdown.state.as_str())?,
            assignees,
            labels,
            comments: markdown
                .comments
                .into_iter()
                .map(|comment| comment.body)
                .collect(),
        })
    }

    fn matches_issue(&self, issue: &issue::Issue) -> bool {
        let assignees = issue.assignees().cloned().collect::<BTreeSet<_>>();
        let labels = issue.labels().cloned().collect::<BTreeSet<_>>();
        let comments = issue
            .comments()
            .skip(1)
            .map(|(_, comment)| comment.body().to_owned())
            .collect::<Vec<_>>();

        issue.title() == self.title
            && issue.description() == self.description
            && issue.state() == &self.state
            && assignees == self.assignees
            && labels == self.labels
            && comments == self.comments
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use radicle::cob::Title;
    use radicle::issue::cache::Issues as _;

    use super::super::export::export;
    use super::super::id_map::{ID_MAP_FILE_NAME, load_id_map};
    use super::super::markdown::MarkdownIssue;
    use super::super::test_support::{
        ISSUES_DIR, Workspace, export_options, import_options, write_markdown_file,
        write_markdown_file_with_comments,
    };
    use super::{collect_files_recursively, import};

    #[test]
    fn import_creates_missing_issues_with_id_mapping() {
        let ws = Workspace::new("thyseus", 3);
        write_markdown_file(
            &ws.issue_dir(),
            &["thyseus", "2026-01-01-task.md"],
            "task-42",
            "Imported task",
        );

        let mut issues = ws.issues();
        import(
            &ws.repo_root,
            Path::new(ISSUES_DIR),
            import_options(false),
            &mut issues,
        )
        .unwrap();

        assert_eq!(ws.internal_issue_count(), 1);
        let map_path = ws.issue_dir().join(ID_MAP_FILE_NAME);
        let map_raw = fs::read_to_string(&map_path).unwrap();
        assert!(map_raw.contains("\"task-42\""), "{map_raw}");

        import(
            &ws.repo_root,
            Path::new(ISSUES_DIR),
            import_options(false),
            &mut issues,
        )
        .unwrap();
        assert_eq!(ws.internal_issue_count(), 1);
    }

    #[test]
    fn import_conflict_policy_requires_force_for_overwrite() {
        let ws = Workspace::new("thyseus", 4);
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

        let err = import(
            &ws.repo_root,
            Path::new(ISSUES_DIR),
            import_options(false),
            &mut issues,
        )
        .unwrap_err();
        assert!(err.to_string().contains("conflict"), "{err:?}");
        assert_eq!(
            issues.list().unwrap().next().unwrap().unwrap().1.title(),
            "Original title"
        );

        import(
            &ws.repo_root,
            Path::new(ISSUES_DIR),
            import_options(true),
            &mut issues,
        )
        .unwrap();
        assert_eq!(
            issues.list().unwrap().next().unwrap().unwrap().1.title(),
            "Renamed title"
        );
    }

    #[test]
    fn import_continues_after_invalid_file_and_fails() {
        let ws = Workspace::new("thyseus", 5);
        write_markdown_file(
            &ws.issue_dir(),
            &["thyseus", "2026-01-01-valid.md"],
            "task-a",
            "Valid task",
        );
        let invalid = ws.issue_dir().join("thyseus").join("2026-01-02-invalid.md");
        fs::write(
            &invalid,
            "---\nid: \"task-b\"\nstate: \"open\"\n---\n\nMissing title\n",
        )
        .unwrap();

        let mut issues = ws.issues();
        let err = import(
            &ws.repo_root,
            Path::new(ISSUES_DIR),
            import_options(false),
            &mut issues,
        )
        .unwrap_err();
        assert!(err.to_string().contains("failures"), "{err:?}");
        assert_eq!(ws.internal_issue_count(), 1);
    }

    #[test]
    fn import_rejects_duplicate_ids_across_files() {
        let ws = Workspace::new("thyseus", 6);
        write_markdown_file(
            &ws.issue_dir(),
            &["thyseus", "2026-01-01-first.md"],
            "dup-1",
            "First",
        );
        write_markdown_file(
            &ws.issue_dir(),
            &["thyseus", "2026-01-02-second.md"],
            "dup-1",
            "Second",
        );

        let mut issues = ws.issues();
        let err = import(
            &ws.repo_root,
            Path::new(ISSUES_DIR),
            import_options(false),
            &mut issues,
        )
        .unwrap_err();
        assert!(err.to_string().contains("failures"), "{err:?}");
        assert_eq!(ws.internal_issue_count(), 1);
    }

    #[test]
    fn import_conflicts_when_radicle_issue_missing_locally() {
        let ws = Workspace::new("thyseus", 12);
        let claimed = "0123456789012345678901234567890123456789";
        write_markdown_file(
            &ws.issue_dir(),
            &["thyseus", "2026-01-01-remote.md"],
            claimed,
            "Remote issue",
        );

        let mut issues = ws.issues();
        let err = import(
            &ws.repo_root,
            Path::new(ISSUES_DIR),
            import_options(false),
            &mut issues,
        )
        .unwrap_err();
        assert!(err.to_string().contains("conflict"), "{err:?}");
        assert_eq!(ws.internal_issue_count(), 0);

        import(
            &ws.repo_root,
            Path::new(ISSUES_DIR),
            import_options(true),
            &mut issues,
        )
        .unwrap();
        assert_eq!(ws.internal_issue_count(), 1);

        let map = load_id_map(&ws.issue_dir()).unwrap();
        let adopted = map.get(claimed).unwrap();
        assert_ne!(adopted, claimed);
    }

    #[test]
    fn import_creates_issue_with_comments_and_is_idempotent() {
        let ws = Workspace::new("thyseus", 8);
        write_markdown_file_with_comments(
            &ws.issue_dir(),
            &["thyseus", "2026-01-01-talk.md"],
            "talk-1",
            "Imported talk",
            &["Alpha note", "Beta reply"],
        );

        let mut issues = ws.issues();
        import(
            &ws.repo_root,
            Path::new(ISSUES_DIR),
            import_options(false),
            &mut issues,
        )
        .unwrap();

        assert_eq!(ws.internal_issue_count(), 1);
        {
            let (_, issue) = issues.list().unwrap().next().unwrap().unwrap();
            let bodies = issue
                .comments()
                .skip(1)
                .map(|(_, comment)| comment.body().to_owned())
                .collect::<Vec<_>>();
            assert_eq!(
                bodies,
                vec!["Alpha note".to_owned(), "Beta reply".to_owned()]
            );
        }

        import(
            &ws.repo_root,
            Path::new(ISSUES_DIR),
            import_options(false),
            &mut issues,
        )
        .unwrap();
        let (_, issue) = issues.list().unwrap().next().unwrap().unwrap();
        assert_eq!(issue.comments().count(), 3);
    }

    #[test]
    fn export_import_roundtrip_preserves_comment_bodies_and_order() {
        let src = Workspace::new("thyseus", 9);
        let mut issues = src.issues();
        let mut issue = issues
            .create(
                Title::new("Roundtrip").unwrap(),
                "Roundtrip body",
                &[],
                &[],
                [],
            )
            .unwrap();
        let root = *issue.root().0;
        issue.comment("Oldest remark", root, []).unwrap();
        issue.comment("Newest remark", root, []).unwrap();
        drop(issue);

        export(
            &src.profile,
            &src.repo_root,
            Path::new(ISSUES_DIR),
            export_options(),
            &issues,
        )
        .unwrap();

        let dst = Workspace::new("thyseus", 10);
        for file in collect_files_recursively(&src.issue_dir()).unwrap() {
            let relative = file.strip_prefix(src.issue_dir()).unwrap();
            let target = dst.issue_dir().join(relative);
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::copy(&file, &target).unwrap();
        }

        let mut dst_issues = dst.issues();
        import(
            &dst.repo_root,
            Path::new(ISSUES_DIR),
            // The exported file carries the source issue's Radicle object id,
            // which does not exist in this fresh repository; adopting it as a
            // distinct local issue requires explicit --force.
            import_options(true),
            &mut dst_issues,
        )
        .unwrap();

        {
            let (_, imported) = dst_issues.list().unwrap().next().unwrap().unwrap();
            assert_eq!(imported.title(), "Roundtrip");
            assert_eq!(imported.description(), "Roundtrip body");
            let bodies = imported
                .comments()
                .skip(1)
                .map(|(_, comment)| comment.body().to_owned())
                .collect::<Vec<_>>();
            assert_eq!(
                bodies,
                vec!["Oldest remark".to_owned(), "Newest remark".to_owned()]
            );
        }

        for file in collect_files_recursively(&dst.issue_dir()).unwrap() {
            fs::remove_file(file).unwrap();
        }

        export(
            &dst.profile,
            &dst.repo_root,
            Path::new(ISSUES_DIR),
            export_options(),
            &dst_issues,
        )
        .unwrap();

        let exported = dst.owner_dir().join(dst.exported_files().remove(0));
        let raw = fs::read_to_string(&exported).unwrap();
        let parsed = MarkdownIssue::parse(&exported, &raw).unwrap();

        assert_eq!(parsed.body, "Roundtrip body");
        let bodies = parsed
            .comments
            .iter()
            .map(|comment| comment.body.as_str())
            .collect::<Vec<_>>();
        assert_eq!(bodies, vec!["Oldest remark", "Newest remark"]);

        export(
            &dst.profile,
            &dst.repo_root,
            Path::new(ISSUES_DIR),
            export_options(),
            &dst_issues,
        )
        .unwrap();
        assert_eq!(fs::read_to_string(&exported).unwrap(), raw);
    }

    #[test]
    fn import_conflict_when_comments_diverge_requires_force() {
        let ws = Workspace::new("thyseus", 11);
        let mut issues = ws.issues();
        let mut issue = issues
            .create(
                Title::new("Original title").unwrap(),
                "Original body",
                &[],
                &[],
                [],
            )
            .unwrap();
        let root = *issue.root().0;
        issue.comment("Internal note", root, []).unwrap();
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
        let divergent = fs::read_to_string(&file)
            .unwrap()
            .replace("Internal note", "Divergent note");
        fs::write(&file, &divergent).unwrap();

        let err = import(
            &ws.repo_root,
            Path::new(ISSUES_DIR),
            import_options(false),
            &mut issues,
        )
        .unwrap_err();
        assert!(err.to_string().contains("conflict"), "{err:?}");

        import(
            &ws.repo_root,
            Path::new(ISSUES_DIR),
            import_options(true),
            &mut issues,
        )
        .unwrap();
        let (_, issue) = issues.list().unwrap().next().unwrap().unwrap();
        let bodies = issue
            .comments()
            .skip(1)
            .map(|(_, comment)| comment.body().to_owned())
            .collect::<Vec<_>>();
        assert_eq!(
            bodies,
            vec!["Internal note".to_owned(), "Divergent note".to_owned()]
        );
    }

    #[test]
    fn collect_files_recursively_includes_owner_subdirectories() {
        let tmp = tempfile::tempdir().unwrap();
        let owner_dir = tmp.path().join("thyseus");
        fs::create_dir_all(&owner_dir).unwrap();
        fs::write(tmp.path().join("top.md"), "x").unwrap();
        fs::write(owner_dir.join("nested.md"), "y").unwrap();

        let mut files = collect_files_recursively(tmp.path()).unwrap();
        files.sort();

        assert_eq!(files.len(), 2);
        assert!(files.contains(&tmp.path().join("top.md")));
        assert!(files.contains(&owner_dir.join("nested.md")));
    }
}

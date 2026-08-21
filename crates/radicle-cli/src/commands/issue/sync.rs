use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::{fs, io, str::FromStr};

use anyhow::Context as _;
use chrono::{DateTime, Utc};

use radicle::Profile;
use radicle::cob;
use radicle::cob::common::Label;
use radicle::cob::issue::{self, CloseReason, State};
use radicle::cob::store::access::WriteAs;
use radicle::crypto;
use radicle::issue::cache::Issues as _;
use radicle::node::AliasStore as _;
use radicle::prelude::Did;
use radicle::storage;

use crate::terminal as term;

const ID_MAP_FILE_NAME: &str = ".radicle-issue-import-map.json";

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

pub(super) fn export(
    profile: &Profile,
    repo_root: &Path,
    configured_dir: &Path,
    options: ExportOptions,
    issues: &issue::Cache<
        '_,
        storage::git::Repository,
        WriteAs<'_, impl crypto::Signer>,
        cob::cache::StoreWriter,
    >,
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

pub(super) fn import(
    repo_root: &Path,
    configured_dir: &Path,
    options: ImportOptions,
    issues: &mut issue::Cache<
        '_,
        storage::git::Repository,
        WriteAs<'_, impl crypto::Signer>,
        cob::cache::StoreWriter,
    >,
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

        let resolved = match resolve_internal_issue_id(issues, &id_map, external_id.as_str()) {
            Ok(resolved) => resolved,
            Err(err) => {
                summary.failed += 1;
                term::warning(format!(
                    "failed to resolve internal issue id for '{}': {err}",
                    external_id
                ));
                continue;
            }
        };

        let Some(id) = resolved else {
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

fn resolve_internal_issue_id(
    issues: &issue::Cache<
        '_,
        storage::git::Repository,
        WriteAs<'_, impl crypto::Signer>,
        cob::cache::StoreWriter,
    >,
    id_map: &BTreeMap<String, String>,
    external_id: &str,
) -> anyhow::Result<Option<cob::ObjectId>> {
    if let Some(mapped) = id_map.get(external_id) {
        let mapped_id = cob::ObjectId::from_str(mapped).with_context(|| {
            format!(
                "mapping file '{}' contains invalid object id '{}' for external id '{}'",
                ID_MAP_FILE_NAME, mapped, external_id
            )
        })?;
        if issues.get(&mapped_id)?.is_some() {
            return Ok(Some(mapped_id));
        }
    }

    let Ok(id) = cob::ObjectId::from_str(external_id) else {
        return Ok(None);
    };
    if issues.get(&id)?.is_some() {
        return Ok(Some(id));
    }

    Ok(None)
}

fn create_issue_from_markdown(
    issues: &mut issue::Cache<
        '_,
        storage::git::Repository,
        WriteAs<'_, impl crypto::Signer>,
        cob::cache::StoreWriter,
    >,
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

    Ok(*created.id())
}

fn id_map_path(issue_dir: &Path) -> PathBuf {
    issue_dir.join(ID_MAP_FILE_NAME)
}

fn load_id_map(issue_dir: &Path) -> anyhow::Result<BTreeMap<String, String>> {
    let path = id_map_path(issue_dir);
    if !path.exists() {
        return Ok(BTreeMap::new());
    }

    let raw = fs::read_to_string(&path)
        .with_context(|| format!("failed to read issue id mapping file '{}'", path.display()))?;
    let map = serde_json::from_str::<BTreeMap<String, String>>(&raw)
        .with_context(|| format!("failed to parse issue id mapping file '{}'", path.display()))?;

    Ok(map)
}

fn save_id_map(issue_dir: &Path, map: &BTreeMap<String, String>) -> anyhow::Result<()> {
    let mut content = serde_json::to_string_pretty(map)?;
    content.push('\n');
    write_atomic(id_map_path(issue_dir).as_path(), &content)
}

fn apply_issue_updates(
    issues: &mut issue::Cache<
        '_,
        storage::git::Repository,
        WriteAs<'_, impl crypto::Signer>,
        cob::cache::StoreWriter,
    >,
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

    Ok(())
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

#[derive(Debug, Clone)]
struct DesiredIssue {
    title: String,
    description: String,
    state: State,
    assignees: BTreeSet<Did>,
    labels: BTreeSet<Label>,
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
        })
    }

    fn matches_issue(&self, issue: &issue::Issue) -> bool {
        let assignees = issue.assignees().cloned().collect::<BTreeSet<_>>();
        let labels = issue.labels().cloned().collect::<BTreeSet<_>>();

        issue.title() == self.title
            && issue.description() == self.description
            && issue.state() == &self.state
            && assignees == self.assignees
            && labels == self.labels
    }
}

#[derive(Debug, Clone)]
struct MarkdownIssue {
    id: String,
    title: String,
    state: String,
    author: String,
    assignees: Vec<String>,
    labels: Vec<String>,
    created: String,
    updated: String,
    body: String,
}

impl MarkdownIssue {
    fn from_issue(id: &cob::ObjectId, issue: &issue::Issue) -> Self {
        let mut assignees = issue
            .assignees()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        assignees.sort();

        let mut labels = issue.labels().map(ToString::to_string).collect::<Vec<_>>();
        labels.sort();

        let created = timestamp_to_rfc3339(issue.timestamp());
        let updated = issue
            .comments()
            .map(|(_, comment)| comment.timestamp())
            .max()
            .map(timestamp_to_rfc3339)
            .unwrap_or_else(|| created.clone());

        Self {
            id: id.to_string(),
            title: issue.title().to_owned(),
            state: format_state(issue.state()),
            author: issue.author().to_string(),
            assignees,
            labels,
            created,
            updated,
            body: issue.description().to_owned(),
        }
    }

    fn file_name(&self) -> String {
        let date = DateTime::parse_from_rfc3339(self.created.as_str())
            .map(|time| time.with_timezone(&Utc).format("%Y-%m-%d").to_string())
            .unwrap_or_else(|_| "1970-01-01".to_owned());
        let slug = slugify_title(self.title.as_str());

        format!("{date}-{slug}.md")
    }

    fn render(&self) -> String {
        let mut out = String::new();

        out.push_str("---\n");
        out.push_str(&format!("id: {}\n", quote_yaml_scalar(self.id.as_str())));
        out.push_str(&format!(
            "title: {}\n",
            quote_yaml_scalar(self.title.as_str())
        ));
        out.push_str(&format!(
            "state: {}\n",
            quote_yaml_scalar(self.state.as_str())
        ));
        out.push_str(&format!(
            "author: {}\n",
            quote_yaml_scalar(self.author.as_str())
        ));
        write_yaml_list(&mut out, "assignees", &self.assignees);
        write_yaml_list(&mut out, "labels", &self.labels);
        out.push_str(&format!(
            "created: {}\n",
            quote_yaml_scalar(self.created.as_str())
        ));
        out.push_str(&format!(
            "updated: {}\n",
            quote_yaml_scalar(self.updated.as_str())
        ));
        out.push_str("---\n\n");
        out.push_str(self.body.as_str());
        if !self.body.ends_with('\n') {
            out.push('\n');
        }

        out
    }

    fn parse(path: &Path, raw: &str) -> anyhow::Result<Self> {
        let mut lines = raw.lines().enumerate();
        let Some((_, first)) = lines.next() else {
            anyhow::bail!("failed to parse '{}': file is empty", path.display());
        };
        if first.trim() != "---" {
            anyhow::bail!(
                "failed to parse '{}': front matter must start with '---'",
                path.display()
            );
        }

        let mut front_matter = Vec::new();
        let mut body_lines = Vec::new();
        let mut in_front_matter = true;
        for (line_number, line) in lines {
            if in_front_matter {
                if line.trim() == "---" {
                    in_front_matter = false;
                    continue;
                }
                front_matter.push((line_number + 1, line.to_owned()));
            } else {
                body_lines.push(line.to_owned());
            }
        }

        if in_front_matter {
            anyhow::bail!(
                "failed to parse '{}': missing front matter terminator '---'",
                path.display()
            );
        }

        let mut fields = HashMap::<String, FrontMatterValue>::new();
        let mut current_list_key: Option<String> = None;

        for (line_number, line) in front_matter {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            if let Some(item) = trimmed.strip_prefix("- ") {
                let Some(key) = current_list_key.as_ref() else {
                    anyhow::bail!(
                        "failed to parse '{}': unexpected list item at line {}",
                        path.display(),
                        line_number
                    );
                };
                if let Some(FrontMatterValue::List(values)) = fields.get_mut(key) {
                    values.push(parse_yaml_scalar(item).with_context(|| {
                        format!(
                            "failed to parse '{}': invalid list item at line {}",
                            path.display(),
                            line_number
                        )
                    })?);
                }
                continue;
            }

            current_list_key = None;

            let Some((key, value)) = trimmed.split_once(':') else {
                anyhow::bail!(
                    "failed to parse '{}': expected key-value pair at line {}",
                    path.display(),
                    line_number
                );
            };
            let key = key.trim().to_owned();
            let value = value.trim();

            if value.is_empty() {
                if key == "assignees" || key == "labels" {
                    fields.insert(key.clone(), FrontMatterValue::List(Vec::new()));
                    current_list_key = Some(key);
                }
                continue;
            }

            if (key == "assignees" || key == "labels") && value == "[]" {
                fields.insert(key, FrontMatterValue::List(Vec::new()));
                continue;
            }

            if key == "id"
                || key == "title"
                || key == "state"
                || key == "author"
                || key == "created"
                || key == "updated"
            {
                fields.insert(
                    key,
                    FrontMatterValue::Scalar(parse_yaml_scalar(value).with_context(|| {
                        format!(
                            "failed to parse '{}': invalid scalar at line {}",
                            path.display(),
                            line_number
                        )
                    })?),
                );
            }
        }

        let id = get_required_scalar(&fields, "id", path)?;
        let title = get_required_scalar(&fields, "title", path)?;
        let state = get_required_scalar(&fields, "state", path)?;
        let body = if matches!(body_lines.first(), Some(line) if line.is_empty()) {
            body_lines[1..].join("\n")
        } else {
            body_lines.join("\n")
        };

        Ok(Self {
            id,
            title,
            state,
            author: get_optional_scalar(&fields, "author").unwrap_or_default(),
            assignees: get_list(&fields, "assignees"),
            labels: get_list(&fields, "labels"),
            created: get_optional_scalar(&fields, "created").unwrap_or_default(),
            updated: get_optional_scalar(&fields, "updated").unwrap_or_default(),
            body,
        })
    }
}

#[derive(Debug, Clone)]
enum FrontMatterValue {
    Scalar(String),
    List(Vec<String>),
}

fn get_required_scalar(
    fields: &HashMap<String, FrontMatterValue>,
    key: &str,
    path: &Path,
) -> anyhow::Result<String> {
    let Some(value) = fields.get(key) else {
        anyhow::bail!(
            "failed to parse '{}': required front matter key '{}' is missing",
            path.display(),
            key
        );
    };
    match value {
        FrontMatterValue::Scalar(value) => Ok(value.clone()),
        FrontMatterValue::List(_) => anyhow::bail!(
            "failed to parse '{}': front matter key '{}' must be a scalar",
            path.display(),
            key
        ),
    }
}

fn get_optional_scalar(fields: &HashMap<String, FrontMatterValue>, key: &str) -> Option<String> {
    fields.get(key).and_then(|value| match value {
        FrontMatterValue::Scalar(value) => Some(value.clone()),
        FrontMatterValue::List(_) => None,
    })
}

fn get_list(fields: &HashMap<String, FrontMatterValue>, key: &str) -> Vec<String> {
    fields
        .get(key)
        .and_then(|value| match value {
            FrontMatterValue::List(values) => Some(values.clone()),
            FrontMatterValue::Scalar(_) => None,
        })
        .unwrap_or_default()
}

fn parse_state(value: &str) -> anyhow::Result<State> {
    match value {
        "open" => Ok(State::Open),
        "closed" => Ok(State::Closed {
            reason: CloseReason::Other,
        }),
        "solved" => Ok(State::Closed {
            reason: CloseReason::Solved,
        }),
        other => anyhow::bail!("invalid issue state '{other}' (expected: open, closed, solved)"),
    }
}

fn format_state(state: &State) -> String {
    match state {
        State::Open => "open".to_owned(),
        State::Closed {
            reason: CloseReason::Other,
        } => "closed".to_owned(),
        State::Closed {
            reason: CloseReason::Solved,
        } => "solved".to_owned(),
    }
}

fn timestamp_to_rfc3339(timestamp: cob::common::Timestamp) -> String {
    let time = std::time::UNIX_EPOCH + std::time::Duration::from_secs(timestamp.as_secs());
    DateTime::<Utc>::from(time).to_rfc3339()
}

fn quote_yaml_scalar(value: &str) -> String {
    serde_json::to_string(value).expect("serializing scalar to JSON string cannot fail")
}

fn parse_yaml_scalar(value: &str) -> anyhow::Result<String> {
    if value.starts_with('"') {
        Ok(serde_json::from_str::<String>(value)?)
    } else {
        Ok(value.to_owned())
    }
}

fn write_yaml_list(out: &mut String, key: &str, values: &[String]) {
    if values.is_empty() {
        out.push_str(&format!("{key}: []\n"));
        return;
    }

    out.push_str(&format!("{key}:\n"));
    for value in values {
        out.push_str("  - ");
        out.push_str(quote_yaml_scalar(value).as_str());
        out.push('\n');
    }
}

fn slugify_title(title: &str) -> String {
    let mut slug = String::with_capacity(title.len());
    let mut prev_hyphen = false;

    for c in title.chars() {
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
        "issue".to_owned()
    } else {
        slug
    }
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
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::{Path, PathBuf};

    use super::{
        ExportOptions, ID_MAP_FILE_NAME, ImportOptions, MarkdownIssue, WriteAs, cob,
        collect_files_recursively, export, import, issue, load_id_map, resolve_issue_dir,
        save_id_map, slugify_owner,
    };
    use crate::terminal as term;
    use radicle::Profile;
    use radicle::cob::Title;
    use radicle::crypto::Seed;
    use radicle::issue::cache::Issues as _;
    use radicle::node::Alias;
    use radicle::profile::{Home, Signer};
    use radicle::storage::{self, ReadStorage as _};
    use radicle::test::fixtures;

    const ISSUES_DIR: &str = "issues";

    struct Workspace {
        _tmp: tempfile::TempDir,
        profile: Profile,
        signer: Signer,
        repo: storage::git::Repository,
        repo_root: std::path::PathBuf,
    }

    impl Workspace {
        fn new(alias: &'static str, seed_byte: u8) -> Self {
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

        fn issues(
            &self,
        ) -> issue::Cache<'_, storage::git::Repository, WriteAs<'_, Signer>, cob::cache::StoreWriter>
        {
            term::cob::issues_mut(&self.profile, &self.repo, &self.signer).unwrap()
        }
        fn issue_dir(&self) -> PathBuf {
            self.repo_root.join(ISSUES_DIR)
        }

        fn owner_dir(&self) -> PathBuf {
            self.issue_dir().join("thyseus")
        }

        fn exported_files(&self) -> Vec<String> {
            let mut names = fs::read_dir(self.owner_dir())
                .unwrap()
                .map(|entry| entry.unwrap().file_name().to_string_lossy().to_string())
                .collect::<Vec<_>>();
            names.sort();
            names
        }

        fn internal_issue_count(&self) -> usize {
            let issues = self.issues();
            issues
                .list()
                .unwrap()
                .map(|entry| entry.is_ok())
                .filter(|ok| *ok)
                .count()
        }
    }

    fn write_markdown_file(root: &Path, segments: &[&str], id: &str, title: &str) -> PathBuf {
        let path = segments
            .iter()
            .fold(root.to_path_buf(), |acc, s| acc.join(s));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            format!(
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
            ),
        )
        .unwrap();
        path
    }

    fn export_options() -> ExportOptions {
        ExportOptions {
            path: None,
            dry_run: false,
        }
    }

    fn import_options(force: bool) -> ImportOptions {
        ImportOptions {
            path: None,
            dry_run: false,
            force,
        }
    }

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

    #[test]
    fn markdown_issue_roundtrip_parses_required_fields() {
        let raw = r#"---
id: "0123456789012345678901234567890123456789"
title: "Title"
state: "open"
author: "did:key:z6Mktest"
assignees: []
labels:
  - "bug"
created: "2026-01-01T00:00:00+00:00"
updated: "2026-01-01T00:00:00+00:00"
---

Body
"#;

        let parsed = MarkdownIssue::parse(
            Path::new("/tmp/repo/issues/0123456789012345678901234567890123456789.md"),
            raw,
        )
        .unwrap();

        assert_eq!(parsed.id, "0123456789012345678901234567890123456789");
        assert_eq!(parsed.title, "Title");
        assert_eq!(parsed.state, "open");
        assert_eq!(parsed.labels, vec!["bug".to_owned()]);
        assert_eq!(parsed.body, "Body");
    }

    #[test]
    fn markdown_issue_filename_uses_date_and_slug() {
        let issue = MarkdownIssue {
            id: "deadbeef".to_owned(),
            title: "Ticket 7: Add unit and integration tests for import-export core logic"
                .to_owned(),
            state: "open".to_owned(),
            author: "did:key:z6Mktest".to_owned(),
            assignees: vec![],
            labels: vec![],
            created: "2026-08-21T00:00:00+00:00".to_owned(),
            updated: "2026-08-21T00:00:00+00:00".to_owned(),
            body: "Body".to_owned(),
        };

        assert_eq!(
            issue.file_name(),
            "2026-08-21-ticket-7-add-unit-and-integration-tests-for-import-export-core-logic.md"
        );
    }

    #[test]
    fn markdown_parser_does_not_require_filename_to_match_id() {
        let raw = r#"---
id: "deadbeef"
title: "Title"
state: "open"
author: "did:key:z6Mktest"
assignees: []
labels: []
created: "2026-01-01T00:00:00+00:00"
updated: "2026-01-01T00:00:00+00:00"
---

Body
"#;

        let parsed =
            MarkdownIssue::parse(Path::new("/tmp/repo/issues/2026-01-01-title.md"), raw).unwrap();
        assert_eq!(parsed.id, "deadbeef");
    }

    #[test]
    fn load_id_map_returns_empty_for_missing_file() {
        let tmp = tempfile::tempdir().unwrap();
        let map = load_id_map(tmp.path()).unwrap();

        assert!(map.is_empty());
    }

    #[test]
    fn save_id_map_roundtrips() {
        let tmp = tempfile::tempdir().unwrap();
        let mut map = BTreeMap::new();
        map.insert(
            "external-one".to_owned(),
            "1111111111111111111111111111111111111111".to_owned(),
        );

        save_id_map(tmp.path(), &map).unwrap();
        let loaded = load_id_map(tmp.path()).unwrap();

        assert_eq!(loaded, map);
        assert!(tmp.path().join(ID_MAP_FILE_NAME).exists());
        assert!(
            fs::read_to_string(tmp.path().join(ID_MAP_FILE_NAME))
                .unwrap()
                .contains("external-one")
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

    #[test]
    fn owner_slug_preserves_simple_aliases() {
        assert_eq!(slugify_owner("thyseus"), "thyseus");
    }
}

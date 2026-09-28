//! Canonical Markdown representation of Radicle issues.
//!
//! One issue is rendered as YAML front matter plus a Markdown body. Discussion
//! comments are appended below the description, oldest first, delimited by
//! machine-parseable markers. This module owns the file format: rendering from
//! internal state and parsing back into [`MarkdownIssue`].

use std::collections::HashMap;
use std::path::Path;

use anyhow::Context as _;
use chrono::{DateTime, Utc};

use radicle::cob;
use radicle::cob::issue::{self, CloseReason, State};

pub(super) const COMMENTS_HEADER: &str = "## Comments";
pub(super) const COMMENT_OPEN_MARKER: &str = "<!-- radicle:comment -->";
pub(super) const COMMENT_CLOSE_MARKER: &str = "<!-- /radicle:comment -->";

#[derive(Debug, Clone)]
pub(super) struct MarkdownComment {
    pub(super) body: String,
}

#[derive(Debug, Clone)]
pub(super) struct MarkdownIssue {
    pub(super) id: String,
    pub(super) title: String,
    pub(super) state: String,
    pub(super) author: String,
    pub(super) assignees: Vec<String>,
    pub(super) labels: Vec<String>,
    pub(super) created: String,
    pub(super) updated: String,
    pub(super) body: String,
    pub(super) comments: Vec<MarkdownComment>,
}

impl MarkdownIssue {
    pub(super) fn from_issue(id: &cob::ObjectId, issue: &issue::Issue) -> Self {
        let mut assignees = issue
            .assignees()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        assignees.sort();

        let mut labels = issue.labels().map(ToString::to_string).collect::<Vec<_>>();
        labels.sort();

        let created = timestamp_to_rfc3339(issue.timestamp());

        let mut comments = issue
            .comments()
            .skip(1)
            .map(|(_, comment)| {
                (
                    comment.timestamp(),
                    MarkdownComment {
                        body: comment.body().to_owned(),
                    },
                )
            })
            .collect::<Vec<_>>();
        comments.sort_by_key(|(timestamp, _)| *timestamp);

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
            comments: comments.into_iter().map(|(_, comment)| comment).collect(),
        }
    }

    pub(super) fn file_name(&self) -> String {
        let date = DateTime::parse_from_rfc3339(self.created.as_str())
            .map(|time| time.with_timezone(&Utc).format("%Y-%m-%d").to_string())
            .unwrap_or_else(|_| "1970-01-01".to_owned());
        let slug = slugify_title(self.title.as_str());

        format!("{date}-{slug}.md")
    }

    pub(super) fn render(&self) -> String {
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

        if !self.comments.is_empty() {
            out.push('\n');
            out.push_str(COMMENTS_HEADER);
            out.push_str("\n\n");
            for comment in &self.comments {
                out.push_str(COMMENT_OPEN_MARKER);
                out.push('\n');
                out.push_str(comment.body.trim_end_matches('\n'));
                out.push('\n');
                out.push_str(COMMENT_CLOSE_MARKER);
                out.push_str("\n\n");
            }
        }

        out
    }

    pub(super) fn parse(path: &Path, raw: &str) -> anyhow::Result<Self> {
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
        let raw_body = if matches!(body_lines.first(), Some(line) if line.is_empty()) {
            body_lines.get(1..).unwrap_or_default().join("\n")
        } else {
            body_lines.join("\n")
        };
        let (body, comments) = split_comments_from_body(&raw_body, path)?;

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
            comments,
        })
    }
}

fn split_comments_from_body(
    raw_body: &str,
    path: &Path,
) -> anyhow::Result<(String, Vec<MarkdownComment>)> {
    let lines = raw_body.lines().collect::<Vec<_>>();
    let mut description_lines = Vec::new();
    let mut comments = Vec::new();

    let mut index = 0;
    while let Some(&line) = lines.get(index) {
        if line.trim() == COMMENTS_HEADER {
            let mut lookahead = index + 1;
            while lines.get(lookahead).is_some_and(|l| l.trim().is_empty()) {
                lookahead += 1;
            }
            let starts_section = lines
                .get(lookahead)
                .is_some_and(|l| l.trim() == COMMENT_OPEN_MARKER);
            if starts_section {
                index = lookahead;
                while lines
                    .get(index)
                    .is_some_and(|l| l.trim() == COMMENT_OPEN_MARKER)
                {
                    index += 1;

                    let mut comment_lines = Vec::new();
                    while let Some(l) = lines.get(index) {
                        if l.trim() == COMMENT_CLOSE_MARKER {
                            break;
                        }
                        comment_lines.push(*l);
                        index += 1;
                    }
                    if index >= lines.len() {
                        anyhow::bail!(
                            "failed to parse '{}': unterminated comment block (missing '{}')",
                            path.display(),
                            COMMENT_CLOSE_MARKER
                        );
                    }
                    index += 1;
                    comments.push(MarkdownComment {
                        body: comment_lines.join("\n"),
                    });

                    while lines.get(index).is_some_and(|l| l.trim().is_empty()) {
                        index += 1;
                    }
                }
                break;
            }
        }

        description_lines.push(line);
        index += 1;
    }

    let description = description_lines.join("\n");
    Ok((description.trim_end_matches('\n').to_owned(), comments))
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

pub(super) fn parse_state(value: &str) -> anyhow::Result<State> {
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

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{COMMENT_CLOSE_MARKER, MarkdownIssue, slugify_title};

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
            comments: Vec::new(),
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
    fn markdown_parser_splits_comments_from_description() {
        let raw = r#"---
id: "abc123"
title: "Title"
state: "open"
author: "did:key:z6Mktest"
assignees: []
labels: []
created: "2026-01-01T00:00:00+00:00"
updated: "2026-01-03T00:00:00+00:00"
---

First paragraph.

Second paragraph.

## Comments

<!-- radicle:comment -->
Line one.
Line two.
<!-- /radicle:comment -->

<!-- radicle:comment -->
Beta reply.
<!-- /radicle:comment -->
"#;

        let parsed = MarkdownIssue::parse(Path::new("/tmp/repo/issues/x.md"), raw).unwrap();

        assert_eq!(parsed.body, "First paragraph.\n\nSecond paragraph.");
        assert_eq!(parsed.comments.len(), 2);
        assert_eq!(parsed.comments[0].body, "Line one.\nLine two.");
        assert_eq!(parsed.comments[1].body, "Beta reply.");
    }

    #[test]
    fn markdown_parser_rejects_unterminated_comment_block() {
        let raw = r#"---
id: "abc123"
title: "Title"
state: "open"
---

Body.

## Comments

<!-- radicle:comment -->
Never closed.
"#;

        let err = MarkdownIssue::parse(Path::new("/tmp/repo/issues/x.md"), raw).unwrap_err();
        assert!(err.to_string().contains(COMMENT_CLOSE_MARKER), "{err:?}");
    }

    #[test]
    fn slugify_title_produces_file_safe_slugs() {
        assert_eq!(slugify_title("Hello, World!"), "hello-world");
        assert_eq!(slugify_title("---"), "issue");
    }
}

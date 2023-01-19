use radicle::git;

use crate::terminal as term;

/// The user supplied `Patch` description.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Message {
    /// Prompt user to write comment in editor.
    Edit,
    /// Don't leave a comment.
    Blank,
    /// Use the following string as comment.
    Text(String),
}

impl Message {
    /// Get the `Message` as a string according to the method.
    pub fn get(self, help: &str) -> String {
        let comment = match self {
            Message::Edit => term::Editor::new()
                .require_save(true)
                .trim_newlines(true)
                .extension(".markdown")
                .edit(help)
                .unwrap(),
            Message::Blank => None,
            Message::Text(c) => Some(c),
        };
        let comment = comment.unwrap_or_default().replace(help, "");
        let comment = comment.trim();

        comment.to_owned()
    }

    pub fn append(&mut self, arg: &str) {
        if let Message::Text(v) = self {
            v.extend(["\n\n", arg]);
        } else {
            *self = Message::Text(arg.into());
        };
    }
}

impl Default for Message {
    fn default() -> Self {
        Self::Edit
    }
}

/// List the given commits in a table.
pub fn list_commits(commits: &[git::raw::Commit]) -> anyhow::Result<()> {
    let mut table = term::Table::default();

    for commit in commits {
        let message = commit
            .summary_bytes()
            .unwrap_or_else(|| commit.message_bytes());
        table.push([
            term::format::secondary(term::format::oid(commit.id())),
            term::format::italic(String::from_utf8_lossy(message)),
        ]);
    }
    table.render();

    Ok(())
}

/// Print commits ahead and behind.
pub fn print_commits_ahead_behind(
    repo: &git::raw::Repository,
    left: git::raw::Oid,
    right: git::raw::Oid,
) -> anyhow::Result<()> {
    let (ahead, behind) = repo.graph_ahead_behind(left, right)?;

    term::info!(
        "{} commit(s) ahead, {} commit(s) behind",
        term::format::positive(ahead),
        if behind > 0 {
            term::format::negative(behind)
        } else {
            term::format::dim(behind)
        }
    );
    Ok(())
}

/// Print title and description in a text box.
pub fn print_title_desc(title: &str, description: &str) {
    let title_pretty = &term::format::dim(format!("╭─ {title} ───────"));
    term::print(title_pretty);
    term::blank();

    if description.is_empty() {
        term::print(term::format::italic("No description provided."));
    } else {
        term::markdown(description);
    }

    term::blank();
    term::print(term::format::dim(format!(
        "╰{}",
        "─".repeat(term::text_width(title_pretty) - 1)
    )));
}

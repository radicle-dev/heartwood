use std::io;

use radicle::git;

use crate::terminal as term;
use crate::terminal::Element;

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
    pub fn get(self, help: &str) -> std::io::Result<String> {
        let comment = match self {
            Message::Edit => {
                if term::is_terminal(&io::stderr()) {
                    term::Editor::new().extension("markdown").edit(help)?
                } else {
                    Some(help.to_owned())
                }
            }
            Message::Blank => None,
            Message::Text(c) => Some(c),
        };
        let comment = comment.unwrap_or_default();
        let comment = comment.trim();

        Ok(comment.to_owned())
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

pub const PATCH_MSG: &str = r#"
<!--
Please enter a patch message for your changes. An empty
message aborts the patch proposal.

The first line is the patch title. The patch description
follows, and must be separated with a blank line, just
like a commit message. Markdown is supported in the title
and description.
-->
"#;

/// Combine the title and description fields to display to the user.
#[inline]
pub fn message(title: &str, description: &str) -> String {
    format!("{title}\n\n{description}").trim().to_string()
}

/// Get the Patch title and description from the command line arguments, or request it from the
/// user.
///
/// The user can bail out if an empty title is entered.
pub fn get_message(
    message: term::patch::Message,
    default_msg: &str,
) -> io::Result<(String, String)> {
    let display_msg = default_msg.trim_end();

    let message = message.get(&format!("{display_msg}\n{PATCH_MSG}"))?;
    let message = message.replace(PATCH_MSG.trim(), ""); // Delete help message.

    let (title, description) = message.split_once('\n').unwrap_or((&message, ""));
    let (title, description) = (title.trim().to_string(), description.trim().to_string());

    if title.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "a patch title must be provided",
        ));
    }

    Ok((title, description))
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
            term::format::italic(String::from_utf8_lossy(message).to_string()),
        ]);
    }
    table.print();

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

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_get_message() {
        let res = get_message(
            Message::Text("title\n\ndescription".to_string()),
            "default text",
        )
        .unwrap();
        assert_eq!(("title".to_string(), "description".to_string()), res);

        let res = get_message(
            Message::Text("title\ndescription\nsecond description".to_string()),
            "default text",
        )
        .unwrap();
        assert_eq!(
            (
                "title".to_string(),
                "description\nsecond description".to_string()
            ),
            res
        );

        let res = get_message(
            Message::Text(" title \ndescription  \n \n ".to_string()),
            "default text",
        )
        .unwrap();
        assert_eq!(("title".to_string(), "description".to_string()), res);
    }
}

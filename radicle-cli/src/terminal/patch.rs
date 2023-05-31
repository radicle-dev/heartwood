use std::fmt;
use std::fmt::Write;
use std::io;
use std::io::IsTerminal as _;

use thiserror::Error;

use radicle::cob;
use radicle::cob::patch;
use radicle::git;

use crate::terminal as term;
use crate::terminal::Element;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Fmt(#[from] fmt::Error),
    #[error("git: {0}")]
    Git(#[from] git::raw::Error),
    #[error("i/o error: {0}")]
    Io(#[from] io::Error),
    #[error("invalid utf-8 string")]
    InvalidUtf8,
}

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
                if io::stderr().is_terminal() {
                    term::Editor::new().extension("markdown").edit(help)?
                } else {
                    Some(help.to_owned())
                }
            }
            Message::Blank => None,
            Message::Text(c) => Some(c),
        };
        let comment = comment.unwrap_or_default();
        let comment = term::format::strip_comments(&comment);
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

const REVISION_MSG: &str = r#"
<!--
Please enter a comment for your patch update. Leaving this
blank is also okay.
-->
"#;

/// Combine the title and description fields to display to the user.
#[inline]
pub fn message(title: &str, description: &str) -> String {
    format!("{title}\n\n{description}").trim().to_string()
}

/// Create a helpful default `Patch` message out of one or more commit messages.
fn message_from_commits(name: &str, commits: Vec<git::raw::Commit>) -> Result<String, Error> {
    let mut commits = commits.into_iter().rev();
    let count = commits.len();
    let Some(commit) = commits.next() else {
        return Ok(String::default());
    };
    let commit_msg = commit.message().ok_or(Error::InvalidUtf8)?.to_string();

    if count == 1 {
        return Ok(commit_msg);
    }

    // Many commits
    let mut msg = String::new();
    writeln!(&mut msg, "<!--")?;
    writeln!(
        &mut msg,
        "This {name} is the combination of {count} commits.",
    )?;
    writeln!(&mut msg, "This is the first commit message:")?;
    writeln!(&mut msg, "-->")?;
    writeln!(&mut msg)?;
    writeln!(&mut msg, "{commit_msg}")?;
    writeln!(&mut msg)?;

    for (i, commit) in commits.enumerate() {
        let commit_msg = commit.message().ok_or(Error::InvalidUtf8)?;
        let commit_num = i + 2;

        writeln!(&mut msg, "<!--")?;
        writeln!(&mut msg, "This is commit message #{commit_num}:")?;
        writeln!(&mut msg, "-->")?;
        writeln!(&mut msg)?;
        writeln!(&mut msg, "{commit_msg}")?;
        writeln!(&mut msg)?;
    }

    Ok(msg.trim().to_string())
}

/// Return commits between the merge base and a head.
pub fn patch_commits<'a>(
    repo: &'a git::raw::Repository,
    base: &git::Oid,
    head: &git::Oid,
) -> Result<Vec<git::raw::Commit<'a>>, git::raw::Error> {
    let mut commits = Vec::new();
    let mut revwalk = repo.revwalk()?;
    revwalk.push_range(&format!("{base}..{head}"))?;

    for rev in revwalk {
        let commit = repo.find_commit(rev?)?;
        commits.push(commit);
    }
    Ok(commits)
}

/// The message shown in the editor when creating a `Patch`.
fn create_display_message(
    repo: &git::raw::Repository,
    base: &git::Oid,
    head: &git::Oid,
) -> Result<String, Error> {
    let commits = patch_commits(repo, base, head)?;
    if commits.is_empty() {
        return Ok(PATCH_MSG.trim_start().to_string());
    }

    let summary = message_from_commits("patch", commits)?;
    Ok(format!("{summary}\n{PATCH_MSG}"))
}

/// Get the Patch title and description from the command line arguments, or request it from the
/// user.
///
/// The user can bail out if an empty title is entered.
pub fn get_create_message(
    message: term::patch::Message,
    repo: &git::raw::Repository,
    base: &git::Oid,
    head: &git::Oid,
) -> Result<(String, String), Error> {
    let display_msg = create_display_message(repo, base, head)?;
    let message = message.get(&display_msg)?;

    let (title, description) = message.split_once('\n').unwrap_or((&message, ""));
    let (title, description) = (title.trim().to_string(), description.trim().to_string());

    if title.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "a patch title must be provided",
        )
        .into());
    }

    Ok((title, description))
}

/// The message shown in the editor when editing a `Patch`.
fn edit_display_message(title: &str, description: &str) -> String {
    format!("{}\n\n{}\n{PATCH_MSG}", title, description)
        .trim_start()
        .to_string()
}

/// Get a patch edit message.
pub fn get_edit_message(
    patch_message: term::patch::Message,
    patch: &cob::patch::Patch,
) -> io::Result<(String, String)> {
    let display_msg = edit_display_message(patch.title(), patch.description());
    let patch_message = patch_message.get(&display_msg)?;
    let patch_message = patch_message.replace(PATCH_MSG.trim(), ""); // Delete help message.

    let (title, description) = patch_message
        .split_once('\n')
        .unwrap_or((&patch_message, ""));
    let (title, description) = (title.trim().to_string(), description.trim().to_string());

    if title.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "a patch title must be provided",
        ));
    }

    Ok((title, description))
}

/// The message shown in the editor when updating a `Patch`.
fn update_display_message(
    repo: &git::raw::Repository,
    last_rev_head: &git::Oid,
    head: &git::Oid,
) -> Result<String, Error> {
    if !repo.graph_descendant_of(**head, **last_rev_head)? {
        return Ok(REVISION_MSG.trim_start().to_string());
    }

    let commits = patch_commits(repo, last_rev_head, head)?;
    if commits.is_empty() {
        return Ok(REVISION_MSG.trim_start().to_string());
    }

    let summary = message_from_commits("patch", commits)?;
    Ok(format!("{summary}\n{REVISION_MSG}"))
}

/// Get a patch update message.
pub fn get_update_message(
    message: term::patch::Message,
    repo: &git::raw::Repository,
    latest: &patch::Revision,
    head: &git::Oid,
) -> Result<String, Error> {
    let display_msg = update_display_message(repo, &latest.head(), head)?;
    let message = message.get(&display_msg)?;
    let message = message.trim();

    Ok(message.to_owned())
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
    use radicle::git::refname;
    use radicle::test::fixtures;
    use std::path;

    fn commit(
        repo: &git::raw::Repository,
        branch: &git::RefStr,
        parent: &git::Oid,
        msg: &str,
    ) -> git::Oid {
        let sig = git::raw::Signature::new(
            "anonymous",
            "anonymous@radicle.xyz",
            &git::raw::Time::new(0, 0),
        )
        .unwrap();
        let head = repo.find_commit(**parent).unwrap();
        let tree =
            git::write_tree(path::Path::new("README"), "Hello World!\n".as_bytes(), repo).unwrap();

        let branch = git::refs::branch(branch);
        let commit = git::commit(repo, &head, &branch, msg, &sig, &tree).unwrap();

        commit.id().into()
    }

    #[test]
    fn test_create_display_message() {
        let tmpdir = tempfile::tempdir().unwrap();
        let (repo, commit_0) = fixtures::repository(&tmpdir);
        let commit_0 = commit_0.into();
        let commit_1 = commit(&repo, &refname!("feature"), &commit_0, "commit 1");
        let commit_2 = commit(&repo, &refname!("feature"), &commit_1, "commit 2");

        let res = create_display_message(&repo, &commit_0, &commit_0).unwrap();
        assert_eq!(
            "\
            <!--\n\
            Please enter a patch message for your changes. An empty\n\
            message aborts the patch proposal.\n\
            \n\
            The first line is the patch title. The patch description\n\
            follows, and must be separated with a blank line, just\n\
            like a commit message. Markdown is supported in the title\n\
            and description.\n\
            -->\n\
            ",
            res
        );

        let res = create_display_message(&repo, &commit_0, &commit_1).unwrap();
        assert_eq!(
            "\
            commit 1\n\
            \n\
            <!--\n\
            Please enter a patch message for your changes. An empty\n\
            message aborts the patch proposal.\n\
            \n\
            The first line is the patch title. The patch description\n\
            follows, and must be separated with a blank line, just\n\
            like a commit message. Markdown is supported in the title\n\
            and description.\n\
            -->\n\
            ",
            res
        );

        let res = create_display_message(&repo, &commit_0, &commit_2).unwrap();
        assert_eq!(
            "\
            <!--\n\
            This patch is the combination of 2 commits.\n\
            This is the first commit message:\n\
            -->\n\
            \n\
            commit 1\n\
            \n\
            <!--\n\
            This is commit message #2:\n\
            -->\n\
            \n\
            commit 2\n\
            \n\
            <!--\n\
            Please enter a patch message for your changes. An empty\n\
            message aborts the patch proposal.\n\
            \n\
            The first line is the patch title. The patch description\n\
            follows, and must be separated with a blank line, just\n\
            like a commit message. Markdown is supported in the title\n\
            and description.\n\
            -->\n\
            ",
            res
        );
    }

    #[test]
    fn test_edit_display_message() {
        let res = edit_display_message("title", "The patch description.");
        assert_eq!(
            "\
            title\n\
            \n\
            The patch description.\n\
            \n\
            <!--\n\
            Please enter a patch message for your changes. An empty\n\
            message aborts the patch proposal.\n\
            \n\
            The first line is the patch title. The patch description\n\
            follows, and must be separated with a blank line, just\n\
            like a commit message. Markdown is supported in the title\n\
            and description.\n\
            -->\n\
            ",
            res
        );
    }

    #[test]
    fn test_update_display_message() {
        let tmpdir = tempfile::tempdir().unwrap();
        let (repo, commit_0) = fixtures::repository(&tmpdir);
        let commit_0 = commit_0.into();

        let commit_1 = commit(&repo, &refname!("feature"), &commit_0, "commit 1");
        let commit_2 = commit(&repo, &refname!("feature"), &commit_1, "commit 2");
        let commit_squashed = commit(
            &repo,
            &refname!("squashed-feature"),
            &commit_0,
            "commit squashed",
        );

        let res = update_display_message(&repo, &commit_1, &commit_1).unwrap();
        assert_eq!(
            "\
            <!--\n\
            Please enter a comment for your patch update. Leaving this\n\
            blank is also okay.\n\
            -->\n\
            ",
            res
        );

        let res = update_display_message(&repo, &commit_1, &commit_2).unwrap();
        assert_eq!(
            "\
            commit 2\n\
            \n\
            <!--\n\
            Please enter a comment for your patch update. Leaving this\n\
            blank is also okay.\n\
            -->\n\
            ",
            res
        );

        let res = update_display_message(&repo, &commit_1, &commit_squashed).unwrap();
        assert_eq!(
            "\
            <!--\n\
            Please enter a comment for your patch update. Leaving this\n\
            blank is also okay.\n\
            -->\n\
            ",
            res
        );
    }
}

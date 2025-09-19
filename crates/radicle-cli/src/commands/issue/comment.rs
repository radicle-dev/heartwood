use radicle::cob::thread;
use radicle::storage::WriteRepository;
use radicle::Profile;
use radicle::{cob, git, issue, storage};

use crate::git::Rev;
use crate::terminal as term;
use crate::terminal::patch::Message;
use crate::terminal::Element as _;

pub(super) fn comment(
    profile: &Profile,
    repo: &storage::git::Repository,
    issues: &mut issue::Cache<
        issue::Issues<'_, storage::git::Repository>,
        cob::cache::Store<cob::cache::Write>,
    >,
    id: Rev,
    message: Message,
    reply_to: Option<Rev>,
    quiet: bool,
) -> Result<(), anyhow::Error> {
    let reply_to = reply_to
        .map(|rev| rev.resolve::<git::Oid>(repo.raw()))
        .transpose()?;
    let signer = term::signer(profile)?;
    let issue_id = id.resolve::<cob::ObjectId>(&repo.backend)?;
    let mut issue = issues.get_mut(&issue_id)?;
    let (root_comment_id, _) = issue.root();
    let body = prompt_comment(message, issue.thread(), reply_to, None)?;
    let comment_id = issue.comment(body, reply_to.unwrap_or(*root_comment_id), vec![], &signer)?;
    if quiet {
        term::print(comment_id);
    } else {
        let comment = issue.thread().comment(&comment_id).unwrap();
        term::comment::widget(&comment_id, comment, profile).print();
    }
    Ok(())
}

pub(super) fn edit(
    profile: &Profile,
    repo: &storage::git::Repository,
    issues: &mut issue::Cache<
        issue::Issues<'_, storage::git::Repository>,
        cob::cache::Store<cob::cache::Write>,
    >,
    id: Rev,
    message: Message,
    comment_id: Rev,
    quiet: bool,
) -> Result<(), anyhow::Error> {
    let signer = term::signer(profile)?;
    let issue_id = id.resolve::<cob::ObjectId>(&repo.backend)?;
    let comment_id = comment_id.resolve(&repo.backend)?;
    let mut issue = issues.get_mut(&issue_id)?;
    let comment = issue
        .thread()
        .comment(&comment_id)
        .ok_or(anyhow::anyhow!("comment '{comment_id}' not found"))?;
    let body = prompt_comment(
        message,
        issue.thread(),
        comment.reply_to(),
        Some(comment.body()),
    )?;
    issue.edit_comment(comment_id, body, vec![], &signer)?;
    if quiet {
        term::print(comment_id);
    } else {
        let comment = issue.thread().comment(&comment_id).unwrap();
        term::comment::widget(&comment_id, comment, profile).print();
    }
    Ok(())
}

/// Get a comment from the user, by prompting.
fn prompt_comment(
    message: Message,
    thread: &thread::Thread,
    mut reply_to: Option<git::Oid>,
    edit: Option<&str>,
) -> anyhow::Result<String> {
    let (chase, missing) = {
        let mut chase = Vec::with_capacity(thread.len());
        let mut missing = None;
        while let Some(id) = reply_to {
            if let Some(comment) = thread.comment(&id) {
                chase.push(comment);
                reply_to = comment.reply_to();
            } else {
                missing = reply_to;
                break;
            }
        }

        (chase, missing)
    };

    let quotes = if chase.is_empty() {
        ""
    } else {
        "Quotes (lines starting with '>') will be preserved. Please remove those that you do not intend to keep.\n"
    };

    let mut buffer = term::format::html::commented(format!("HTML comments, such as this one, are deleted before posting.\n{quotes}Saving an empty file aborts the operation.").as_str());
    buffer.push('\n');

    for comment in chase.iter().rev() {
        buffer.reserve(2);
        buffer.push('\n');
        comment_quoted(comment, &mut buffer);
    }

    if let Some(id) = missing {
        buffer.push('\n');
        buffer.push_str(
            term::format::html::commented(
                format!("The comment with ID {id} that was replied to could not be found.")
                    .as_str(),
            )
            .as_str(),
        );
    }

    if let Some(edit) = edit {
        if !chase.is_empty() {
            buffer.push_str(
                "\n<!-- The contents of the comment you are editing follow below this line. -->\n",
            );
        }

        buffer.reserve(2 + edit.len());
        buffer.push('\n');
        buffer.push_str(edit);
    }

    let body = message.get(&buffer)?;
    if body.is_empty() {
        anyhow::bail!("aborting operation due to empty comment");
    }

    Ok(body)
}

fn comment_quoted(comment: &thread::Comment, buffer: &mut String) {
    let body = comment.body();
    let lines = body.lines();
    let hint = {
        let (lower, upper) = lines.size_hint();
        upper.unwrap_or(lower)
    };

    buffer.push_str(format!("{} wrote:\n", comment.author()).as_str());
    buffer.reserve(body.len() + hint * 2);

    for line in lines {
        buffer.push('>');
        if !line.is_empty() {
            buffer.push(' ');
        }

        buffer.push_str(line);
        buffer.push('\n');
    }
}

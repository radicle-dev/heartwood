mod args;
mod cache;

use anyhow::Context as _;

use radicle::cob::common::Label;
use radicle::cob::issue::{CloseReason, State};
use radicle::cob::{issue, thread, Title};

use radicle::crypto;
use radicle::git::Oid;
use radicle::issue::cache::Issues as _;
use radicle::node::device::Device;
use radicle::node::NodeId;
use radicle::prelude::Did;
use radicle::profile;
use radicle::storage;
use radicle::storage::{WriteRepository, WriteStorage};
use radicle::Profile;
use radicle::{cob, Node};

pub use args::Args;
use args::{Assigned, Command, StateArg};

use crate::git::Rev;
use crate::node;
use crate::terminal as term;
use crate::terminal::args::Error;
use crate::terminal::format::Author;
use crate::terminal::issue::Format;
use crate::terminal::patch::Message;
use crate::terminal::Element;

pub fn run(args: Args, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    let rid = if let Some(rid) = args.repo {
        rid
    } else {
        radicle::rad::cwd().map(|(_, rid)| rid)?
    };
    let repo = profile.storage.repository_mut(rid)?;

    let command = args.command.unwrap_or_default();
    let announce = !args.no_announce
        && matches!(
            &command,
            Command::Open { .. }
                | Command::React { .. }
                | Command::State { .. }
                | Command::Delete { .. }
                | Command::Assign { .. }
                | Command::Label { .. }
                | Command::Edit { .. }
                // TODO(erikli): Remove special handling for `--edit` and
                // make it also announce.
                | Command::Comment { edit: None, .. }
        );

    let mut issues = term::cob::issues_mut(&profile, &repo)?;

    match command {
        Command::Edit {
            id,
            title,
            description,
        } => {
            let signer = term::signer(&profile)?;
            let issue = edit(&mut issues, &repo, id, title, description, &signer)?;
            if !args.quiet {
                term::issue::show(&issue, issue.id(), Format::Header, args.verbose, &profile)?;
            }
        }
        Command::Open {
            ref title,
            ref description,
            ref labels,
            ref assignees,
        } => {
            let signer = term::signer(&profile)?;
            open(
                title.clone(),
                description.clone(),
                labels.to_vec(),
                assignees.to_vec(),
                args.verbose,
                args.quiet,
                &mut issues,
                &signer,
                &profile,
            )?;
        }
        Command::Comment {
            id,
            message,
            reply_to,
            edit: None,
        } => {
            let reply_to = reply_to
                .map(|rev| rev.resolve::<radicle::git::Oid>(repo.raw()))
                .transpose()?;

            let signer = term::signer(&profile)?;
            let issue_id = id.resolve::<cob::ObjectId>(&repo.backend)?;
            let mut issue = issues.get_mut(&issue_id)?;
            let (root_comment_id, _) = issue.root();
            let body = prompt_comment(message, issue.thread(), reply_to, None)?;
            let comment_id =
                issue.comment(body, reply_to.unwrap_or(*root_comment_id), vec![], &signer)?;

            if args.quiet {
                term::print(comment_id);
            } else {
                let comment = issue.thread().comment(&comment_id).unwrap();
                term::comment::widget(&comment_id, comment, &profile).print();
            }
        }
        Command::Comment {
            id,
            message,
            reply_to: None,
            edit: Some(comment_id),
        } => {
            let signer = term::signer(&profile)?;
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

            if args.quiet {
                term::print(comment_id);
            } else {
                let comment = issue.thread().comment(&comment_id).unwrap();
                term::comment::widget(&comment_id, comment, &profile).print();
            }
        }
        Command::Comment { .. } => {
            unreachable!("the argument '--reply-to' cannot be used with '--edit'");
        }
        Command::Show { id } => {
            let format = if args.header {
                term::issue::Format::Header
            } else {
                term::issue::Format::Full
            };

            let id = id.resolve(&repo.backend)?;
            let issue = issues
                .get(&id)
                .map_err(|e| Error::WithHint {
                    err: e.into(),
                    hint: "reset the cache with `rad issue cache` and try again",
                })?
                .context("No issue with the given ID exists")?;
            term::issue::show(&issue, &id, format, args.verbose, &profile)?;
        }
        Command::State { id, target_state } => {
            let to: StateArg = target_state.into();
            let id = id.resolve(&repo.backend)?;
            let signer = term::signer(&profile)?;
            let mut issue = issues.get_mut(&id)?;
            let state = to.into();
            issue.lifecycle(state, &signer)?;

            if !args.quiet {
                let success =
                    |status| term::success!("Issue {} is now {status}", term::format::cob(&id));
                match state {
                    State::Closed { reason } => match reason {
                        CloseReason::Other => success("closed"),
                        CloseReason::Solved => success("solved"),
                    },
                    State::Open => success("open"),
                };
            }
        }
        Command::React {
            id,
            reaction,
            comment_id,
        } => {
            let id = id.resolve(&repo.backend)?;
            if let Ok(mut issue) = issues.get_mut(&id) {
                let signer = term::signer(&profile)?;
                let comment_id = match comment_id {
                    Some(cid) => cid.resolve(&repo.backend)?,
                    None => *term::io::comment_select(&issue).map(|(cid, _)| cid)?,
                };
                let reaction = match reaction {
                    Some(reaction) => reaction,
                    None => term::io::reaction_select()?,
                };
                // SAFETY: reaction is never None here.
                issue.react(comment_id, reaction, true, &signer)?;
            }
        }
        Command::Assign { id, add, delete } => {
            let signer = term::signer(&profile)?;
            let id = id.resolve(&repo.backend)?;
            let Ok(mut issue) = issues.get_mut(&id) else {
                anyhow::bail!("Issue `{id}` not found");
            };
            let assignees = issue
                .assignees()
                .filter(|did| !delete.contains(did))
                .chain(add.iter())
                .cloned()
                .collect::<Vec<_>>();
            issue.assign(assignees, &signer)?;
        }
        Command::Label { id, add, delete } => {
            let id = id.resolve(&repo.backend)?;
            let Ok(mut issue) = issues.get_mut(&id) else {
                anyhow::bail!("Issue `{id}` not found");
            };
            let labels = issue
                .labels()
                .filter(|did| !delete.contains(did))
                .chain(add.iter())
                .cloned()
                .collect::<Vec<_>>();
            let signer = term::signer(&profile)?;
            issue.label(labels, &signer)?;
        }
        Command::List(list_args) => {
            let assigned = list_args.assigned.clone();
            list(issues, &assigned, &list_args.into(), &profile, args.verbose)?;
        }
        Command::Delete { id } => {
            let id = id.resolve(&repo.backend)?;
            let signer = term::signer(&profile)?;
            issues.remove(&id, &signer)?;
        }
        Command::Cache { id, storage } => {
            let mode = if storage {
                cache::CacheMode::Storage
            } else {
                let issue_id = id.map(|id| id.resolve(&repo.backend)).transpose()?;
                issue_id.map_or(cache::CacheMode::Repository { repository: &repo }, |id| {
                    cache::CacheMode::Issue {
                        id,
                        repository: &repo,
                    }
                })
            };
            cache::run(mode, &profile)?;
        }
    }

    if announce {
        let mut node = Node::new(profile.socket());
        node::announce(
            &repo,
            node::SyncSettings::default(),
            node::SyncReporting::default(),
            &mut node,
            &profile,
        )?;
    }

    Ok(())
}

fn list<C>(
    cache: C,
    assigned: &Option<Assigned>,
    state: &Option<State>,
    profile: &profile::Profile,
    verbose: bool,
) -> anyhow::Result<()>
where
    C: issue::cache::Issues,
{
    if cache.is_empty()? {
        term::print(term::format::italic("Nothing to show."));
        return Ok(());
    }

    let assignee = match assigned {
        Some(Assigned::Me) => Some(*profile.id()),
        Some(Assigned::Peer(id)) => Some((*id).into()),
        None => None,
    };

    let mut all = cache
        .list()?
        .filter_map(|result| {
            let (id, issue) = match result {
                Ok((id, issue)) => (id, issue),
                Err(e) => {
                    // Skip issues that failed to load.
                    log::error!(target: "cli", "Issue load error: {e}");
                    return None;
                }
            };

            if let Some(a) = assignee {
                if !issue.assignees().any(|v| v == &Did::from(a)) {
                    return None;
                }
            }

            if let Some(s) = state {
                if s != issue.state() {
                    return None;
                }
            }

            Some((id, issue))
        })
        .collect::<Vec<_>>();

    all.sort_by(|(id1, i1), (id2, i2)| {
        let by_timestamp = i2.timestamp().cmp(&i1.timestamp());
        let by_id = id1.cmp(id2);

        by_timestamp.then(by_id)
    });

    let mut table = term::Table::new(term::table::TableOptions::bordered());
    table.header([
        term::format::dim(String::from("●")).into(),
        term::format::bold(String::from("ID")).into(),
        term::format::bold(String::from("Title")).into(),
        term::format::bold(String::from("Author")).into(),
        term::Line::blank(),
        term::format::bold(String::from("Labels")).into(),
        term::format::bold(String::from("Assignees")).into(),
        term::format::bold(String::from("Opened")).into(),
    ]);
    table.divider();

    table.extend(all.into_iter().map(|(id, issue)| {
        let assigned: String = issue
            .assignees()
            .map(|did| {
                let (alias, _) = Author::new(did.as_key(), profile, verbose).labels();

                alias.content().to_owned()
            })
            .collect::<Vec<_>>()
            .join(", ");

        let mut labels = issue.labels().map(|t| t.to_string()).collect::<Vec<_>>();
        labels.sort();

        let author = issue.author().id;
        let (alias, did) = Author::new(&author, profile, verbose).labels();

        mk_issue_row(id, issue, assigned, labels, alias, did)
    }));

    table.print();

    Ok(())
}

fn mk_issue_row(
    id: cob::ObjectId,
    issue: issue::Issue,
    assigned: String,
    labels: Vec<String>,
    alias: radicle_term::Label,
    did: radicle_term::Label,
) -> [radicle_term::Line; 8] {
    [
        match issue.state() {
            State::Open => term::format::positive("●").into(),
            State::Closed { .. } => term::format::negative("●").into(),
        },
        term::format::tertiary(term::format::cob(&id))
            .to_owned()
            .into(),
        term::format::default(issue.title().to_owned()).into(),
        alias.into(),
        did.into(),
        term::format::secondary(labels.join(", ")).into(),
        if assigned.is_empty() {
            term::format::dim(String::default()).into()
        } else {
            term::format::primary(assigned.to_string()).dim().into()
        },
        term::format::timestamp(issue.timestamp())
            .dim()
            .italic()
            .into(),
    ]
}

fn open<R, G>(
    title: Option<Title>,
    description: Option<String>,
    labels: Vec<Label>,
    assignees: Vec<Did>,
    verbose: bool,
    quiet: bool,
    cache: &mut issue::Cache<issue::Issues<'_, R>, cob::cache::StoreWriter>,
    signer: &Device<G>,
    profile: &Profile,
) -> anyhow::Result<()>
where
    R: WriteRepository + cob::Store<Namespace = NodeId>,
    G: crypto::signature::Signer<crypto::Signature>,
{
    let (title, description) = if let (Some(t), Some(d)) = (title.as_ref(), description.as_ref()) {
        (t.to_owned(), d.to_owned())
    } else if let Some((t, d)) = term::issue::get_title_description(title, description)? {
        (t, d)
    } else {
        anyhow::bail!("aborting issue creation due to empty title or description");
    };
    let issue = cache.create(
        title,
        description,
        labels.as_slice(),
        assignees.as_slice(),
        [],
        signer,
    )?;

    if !quiet {
        term::issue::show(&issue, issue.id(), Format::Header, verbose, profile)?;
    }
    Ok(())
}

fn edit<'a, 'g, R, G>(
    issues: &'g mut issue::Cache<issue::Issues<'a, R>, cob::cache::StoreWriter>,
    repo: &storage::git::Repository,
    id: Rev,
    title: Option<Title>,
    description: Option<String>,
    signer: &Device<G>,
) -> anyhow::Result<issue::IssueMut<'a, 'g, R, cob::cache::StoreWriter>>
where
    R: WriteRepository + cob::Store<Namespace = NodeId>,
    G: crypto::signature::Signer<crypto::Signature>,
{
    let id = id.resolve(&repo.backend)?;
    let mut issue = issues.get_mut(&id)?;
    let (root, _) = issue.root();
    let comment_id = *root;

    if title.is_some() || description.is_some() {
        // Editing by command line arguments.
        issue.transaction("Edit", signer, |tx| {
            if let Some(t) = title {
                tx.edit(t)?;
            }
            if let Some(d) = description {
                tx.edit_comment(comment_id, d, vec![])?;
            }
            Ok(())
        })?;
        return Ok(issue);
    }

    // Editing via the editor.
    let Some((title, description)) = term::issue::get_title_description(
        title.and(Title::new(issue.title()).ok()),
        Some(description.unwrap_or(issue.description().to_owned())),
    )?
    else {
        return Ok(issue);
    };

    issue.transaction("Edit", signer, |tx| {
        tx.edit(title)?;
        tx.edit_comment(comment_id, description, vec![])?;

        Ok(())
    })?;

    Ok(issue)
}

/// Get a comment from the user, by prompting.
pub fn prompt_comment(
    message: Message,
    thread: &thread::Thread,
    mut reply_to: Option<Oid>,
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

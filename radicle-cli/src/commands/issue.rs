#![allow(clippy::or_fun_call)]
use std::ffi::OsString;
use std::io;
use std::str::FromStr;

use anyhow::{anyhow, Context as _};

use radicle::cob::common::{Reaction, Tag};
use radicle::cob::issue;
use radicle::cob::issue::{CloseReason, Issues, State};
use radicle::cob::thread;
use radicle::crypto::Signer;
use radicle::node::{AliasStore, Handle};
use radicle::prelude::Did;
use radicle::profile;
use radicle::storage;
use radicle::storage::{WriteRepository, WriteStorage};
use radicle::{cob, Node};
use radicle_term::table::TableOptions;
use radicle_term::{Paint, Table, VStack};

use crate::git::Rev;
use crate::terminal as term;
use crate::terminal::args::{string, Args, Error, Help};
use crate::terminal::format::Author;
use crate::terminal::Element;

pub const HELP: Help = Help {
    name: "issue",
    description: "Manage issues",
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad issue [<option>...]
    rad issue delete <issue-id> [<option>...]
    rad issue edit <issue-id> [<option>...]
    rad issue list [--assigned <did>] [--closed | --open | --solved] [<option>...]
    rad issue open [--title <title>] [--description <text>] [--tag <tag>] [<option>...]
    rad issue react <issue-id> [--emoji <char>] [--to <comment>] [<option>...]
    rad issue show <issue-id> [<option>...]
    rad issue state <issue-id> [--closed | --open | --solved] [<option>...]

Options

    --no-announce     Don't announce issue to peers
    --quiet, -q       Don't print anything
    --help            Print help
"#,
};

#[derive(serde::Deserialize, serde::Serialize, Debug)]
pub struct Metadata {
    title: String,
    tags: Vec<Tag>,
    assignees: Vec<Did>,
}

#[derive(Default, Debug, PartialEq, Eq)]
pub enum OperationName {
    Edit,
    Open,
    Delete,
    #[default]
    List,
    React,
    Show,
    State,
}

/// Command line Peer argument.
#[derive(Default, Debug, PartialEq, Eq)]
pub enum Assigned {
    #[default]
    Me,
    Peer(Did),
}

#[derive(Debug, PartialEq, Eq)]
pub enum Operation {
    Edit {
        id: Rev,
        title: Option<String>,
        description: Option<String>,
    },
    Open {
        title: Option<String>,
        description: Option<String>,
        tags: Vec<Tag>,
    },
    Show {
        id: Rev,
    },
    State {
        id: Rev,
        state: State,
    },
    Delete {
        id: Rev,
    },
    React {
        id: Rev,
        reaction: Reaction,
        comment_id: Option<thread::CommentId>,
    },
    List {
        assigned: Option<Assigned>,
        state: Option<State>,
    },
}

#[derive(Debug)]
pub struct Options {
    pub op: Operation,
    pub announce: bool,
    pub quiet: bool,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut op: Option<OperationName> = None;
        let mut id: Option<Rev> = None;
        let mut assigned: Option<Assigned> = None;
        let mut title: Option<String> = None;
        let mut reaction: Option<Reaction> = None;
        let mut comment_id: Option<thread::CommentId> = None;
        let mut description: Option<String> = None;
        let mut state: Option<State> = None;
        let mut tags = Vec::new();
        let mut announce = true;
        let mut quiet = false;

        while let Some(arg) = parser.next()? {
            match arg {
                Long("help") => {
                    return Err(Error::Help.into());
                }
                Long("closed") if op.is_none() || op == Some(OperationName::List) => {
                    state = Some(State::Closed {
                        reason: CloseReason::Other,
                    });
                }
                Long("open") if op.is_none() || op == Some(OperationName::List) => {
                    state = Some(State::Open);
                }
                Long("solved") if op.is_none() || op == Some(OperationName::List) => {
                    state = Some(State::Closed {
                        reason: CloseReason::Solved,
                    });
                }
                Long("title") if op == Some(OperationName::Open) => {
                    title = Some(parser.value()?.to_string_lossy().into());
                }
                Long("tag") if op == Some(OperationName::Open) => {
                    let val = parser.value()?;
                    let name = term::args::string(&val);
                    let tag = Tag::new(name)?;

                    tags.push(tag);
                }
                Long("closed") if op == Some(OperationName::State) => {
                    state = Some(State::Closed {
                        reason: CloseReason::Other,
                    });
                }
                Long("open") if op == Some(OperationName::State) => {
                    state = Some(State::Open);
                }
                Long("solved") if op == Some(OperationName::State) => {
                    state = Some(State::Closed {
                        reason: CloseReason::Solved,
                    });
                }
                Long("emoji") if op == Some(OperationName::React) => {
                    if let Some(emoji) = parser.value()?.to_str() {
                        reaction =
                            Some(Reaction::from_str(emoji).map_err(|_| anyhow!("invalid emoji"))?);
                    }
                }
                Long("to") if op == Some(OperationName::React) => {
                    let oid: String = parser.value()?.to_string_lossy().into();
                    comment_id = Some(oid.parse()?);
                }
                Long("description") if op == Some(OperationName::Open) => {
                    description = Some(parser.value()?.to_string_lossy().into());
                }
                Long("assigned") | Short('a') if assigned.is_none() => {
                    if let Ok(val) = parser.value() {
                        let peer = term::args::did(&val)?;
                        assigned = Some(Assigned::Peer(peer));
                    } else {
                        assigned = Some(Assigned::Me);
                    }
                }
                Long("no-announce") => {
                    announce = false;
                }
                Long("quiet") | Short('q') => {
                    quiet = true;
                }
                Value(val) if op.is_none() => match val.to_string_lossy().as_ref() {
                    "c" | "show" => op = Some(OperationName::Show),
                    "d" | "delete" => op = Some(OperationName::Delete),
                    "e" | "edit" => op = Some(OperationName::Edit),
                    "l" | "list" => op = Some(OperationName::List),
                    "o" | "open" => op = Some(OperationName::Open),
                    "r" | "react" => op = Some(OperationName::React),
                    "s" | "state" => op = Some(OperationName::State),

                    unknown => anyhow::bail!("unknown operation '{}'", unknown),
                },
                Value(val) if op.is_some() => {
                    let val = string(&val);
                    id = Some(Rev::from(val));
                }
                _ => {
                    return Err(anyhow!(arg.unexpected()));
                }
            }
        }

        let op = match op.unwrap_or_default() {
            OperationName::Edit => Operation::Edit {
                id: id.ok_or_else(|| anyhow!("an issue must be provided"))?,
                title,
                description,
            },
            OperationName::Open => Operation::Open {
                title,
                description,
                tags,
            },
            OperationName::Show => Operation::Show {
                id: id.ok_or_else(|| anyhow!("an issue must be provided"))?,
            },
            OperationName::State => Operation::State {
                id: id.ok_or_else(|| anyhow!("an issue must be provided"))?,
                state: state.ok_or_else(|| anyhow!("a state operation must be provided"))?,
            },
            OperationName::React => Operation::React {
                id: id.ok_or_else(|| anyhow!("an issue must be provided"))?,
                reaction: reaction.ok_or_else(|| anyhow!("a reaction emoji must be provided"))?,
                comment_id,
            },
            OperationName::Delete => Operation::Delete {
                id: id.ok_or_else(|| anyhow!("an issue to remove must be provided"))?,
            },
            OperationName::List => Operation::List { assigned, state },
        };

        Ok((
            Options {
                op,
                announce,
                quiet,
            },
            vec![],
        ))
    }
}

pub fn run(options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    let signer = term::signer(&profile)?;
    let (_, rid) = radicle::rad::cwd()?;
    let repo = profile.storage.repository_mut(rid)?;
    let announce = options.announce
        && matches!(
            &options.op,
            Operation::Open { .. }
                | Operation::React { .. }
                | Operation::State { .. }
                | Operation::Delete { .. }
        );

    let mut node = Node::new(profile.socket());
    let mut issues = Issues::open(&repo)?;

    match options.op {
        Operation::Edit {
            id,
            title,
            description,
        } => {
            edit(&mut issues, &signer, &repo, id, title, description)?;
        }
        Operation::Open {
            title: Some(title),
            description: Some(description),
            tags,
        } => {
            let issue = issues.create(title, description, tags.as_slice(), &[], &signer)?;
            if !options.quiet {
                show_issue(&issue, issue.id())?;
            }
        }
        Operation::Show { id } => {
            let id = id.resolve(&repo.backend)?;
            let issue = issues
                .get(&id)?
                .context("No issue with the given ID exists")?;
            show_issue(&issue, &id)?;
        }
        Operation::State { id, state } => {
            let id = id.resolve(&repo.backend)?;
            let mut issue = issues.get_mut(&id)?;
            issue.lifecycle(state, &signer)?;
        }
        Operation::React {
            id,
            reaction,
            comment_id,
        } => {
            let id = id.resolve(&repo.backend)?;
            if let Ok(mut issue) = issues.get_mut(&id) {
                let comment_id = comment_id.unwrap_or_else(|| {
                    let (comment_id, _) = term::io::comment_select(&issue).unwrap();
                    *comment_id
                });
                issue.react(comment_id, reaction, &signer)?;
            }
        }
        Operation::Open {
            ref title,
            ref description,
            ref tags,
        } => {
            open(
                title.clone(),
                description.clone(),
                tags.to_vec(),
                &options,
                &mut issues,
                &signer,
            )?;
        }
        Operation::List { assigned, state } => {
            list(&issues, &assigned, &state, &profile)?;
        }
        Operation::Delete { id } => {
            let id = id.resolve(&repo.backend)?;
            issues.remove(&id, &signer)?;
        }
    }

    if announce {
        match node.announce_refs(rid) {
            Ok(()) => {}
            Err(e) if e.is_connection_err() => {
                term::warning("Could not announce issue refs: node is not running");
            }
            Err(e) => return Err(e.into()),
        }
    }

    Ok(())
}

fn list<R: WriteRepository + cob::Store>(
    issues: &Issues<R>,
    assigned: &Option<Assigned>,
    state: &Option<State>,
    profile: &profile::Profile,
) -> anyhow::Result<()> {
    if issues.is_empty()? {
        term::print(term::format::italic("Nothing to show."));
        return Ok(());
    }

    let assignee = match assigned {
        Some(Assigned::Me) => Some(*profile.id()),
        Some(Assigned::Peer(id)) => Some((*id).into()),
        None => None,
    };

    let mut all = Vec::new();
    for result in issues.all()? {
        let Ok((id, issue, _)) = result else {
            // Skip issues that failed to load.
            continue;
        };

        if let Some(a) = assignee {
            if !issue.assigned().any(|v| v == Did::from(a)) {
                continue;
            }
        }
        if let Some(s) = state {
            if s != issue.state() {
                continue;
            }
        }
        all.push((id, issue))
    }

    all.sort_by(|(id1, i1), (id2, i2)| {
        let by_timestamp = i2.timestamp().cmp(&i1.timestamp());
        let by_id = id1.cmp(id2);

        by_timestamp.then(by_id)
    });

    let mut table = term::Table::new(term::table::TableOptions::bordered());
    table.push([
        term::format::dim(String::from("●")).into(),
        term::format::bold(String::from("ID")).into(),
        term::format::bold(String::from("Title")).into(),
        term::format::bold(String::from("Author")).into(),
        term::format::bold(String::new()).into(),
        term::format::bold(String::from("Tags")).into(),
        term::format::bold(String::from("Assignees")).into(),
        term::format::bold(String::from("Opened")).into(),
    ]);
    table.divider();

    let aliases = profile.aliases();

    for (id, issue) in all {
        let assigned: String = issue
            .assigned()
            .map(|ref p| {
                if let Some(alias) = aliases.alias(p) {
                    format!("{alias} ({})", term::format::did(p))
                } else {
                    term::format::did(p).to_string()
                }
            })
            .collect::<Vec<_>>()
            .join(", ");

        let mut tags = issue.tags().map(|t| t.to_string()).collect::<Vec<_>>();
        tags.sort();

        let author = issue.author().id;
        let alias = aliases.alias(&author);
        let display = Author::new(&author, alias, profile);

        table.push([
            match issue.state() {
                State::Open => term::format::positive("●").into(),
                State::Closed { .. } => term::format::negative("●").into(),
            },
            term::format::tertiary(term::format::cob(&id))
                .to_owned()
                .into(),
            term::format::default(issue.title().to_owned()).into(),
            term::format::did(&issue.author().id).dim().into(),
            display.alias(),
            term::format::secondary(tags.join(", ")).into(),
            if assigned.is_empty() {
                term::format::dim(String::default()).into()
            } else {
                term::format::default(assigned.to_string()).into()
            },
            term::format::timestamp(&issue.timestamp())
                .dim()
                .italic()
                .into(),
        ]);
    }
    table.print();

    Ok(())
}

/// Get Issue meta-data and description from the user through the editor.
fn prompt_issue(
    title: &str,
    description: &str,
    tags: &[Tag],
    assignees: &[Did],
) -> anyhow::Result<Option<(Metadata, String)>> {
    let title = if title.is_empty() {
        "Enter a title"
    } else {
        title
    };
    let description = if description.is_empty() {
        "<!--\n\
        Enter a description...\n\
        -->"
    } else {
        description
    };

    let meta = Metadata {
        title: title.to_string(),
        tags: tags.to_vec(),
        assignees: assignees.to_vec(),
    };
    let yaml = serde_yaml::to_string(&meta)?;
    let doc = format!("{yaml}---\n\n{description}");

    let Some(text) = term::Editor::new().edit(&doc)? else {
        return Ok(None);
    };

    let mut meta = String::new();
    let mut frontmatter = false;
    let mut lines = text.lines();

    while let Some(line) = lines.by_ref().next() {
        if line.trim() == "---" {
            if frontmatter {
                break;
            } else {
                frontmatter = true;
                continue;
            }
        }
        if frontmatter {
            meta.push_str(line);
            meta.push('\n');
        }
    }

    let mut meta: Metadata =
        serde_yaml::from_str(&meta).context("failed to parse yaml front-matter")?;

    meta.title = meta.title.trim().to_string();
    if meta.title.is_empty() || meta.title == "~" || meta.title == "null" {
        // '~' and 'null' are YAML's string values for null and unexpectedly replace empty fields
        // for String.
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "an issue title must be provided and may not be '~' or 'null'",
        )
        .into());
    }

    let description: String = lines.collect::<Vec<&str>>().join("\n");
    let description = term::format::strip_comments(&description);
    if description.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "an issue description must be provided",
        )
        .into());
    }

    Ok(Some((meta, description)))
}

fn open<R: WriteRepository + cob::Store, G: Signer>(
    title: Option<String>,
    description: Option<String>,
    tags: Vec<Tag>,
    options: &Options,
    issues: &mut Issues<R>,
    signer: &G,
) -> anyhow::Result<()> {
    let Some((meta, description)) = prompt_issue(
        &title.unwrap_or_default(),
        &description.unwrap_or_default(),
        &tags,
        &[],
    )? else {
        return Ok(());
    };

    let issue = issues.create(
        &meta.title,
        description.trim(),
        meta.tags.as_slice(),
        meta.assignees
            .into_iter()
            .map(cob::ActorId::from)
            .collect::<Vec<_>>()
            .as_slice(),
        signer,
    )?;
    if !options.quiet {
        show_issue(&issue, issue.id())?;
    }

    Ok(())
}

fn edit<R: WriteRepository + cob::Store, G: radicle::crypto::Signer>(
    issues: &mut issue::Issues<R>,
    signer: &G,
    repo: &storage::git::Repository,
    id: Rev,
    title: Option<String>,
    description: Option<String>,
) -> anyhow::Result<()> {
    let id = id.resolve(&repo.backend)?;
    let mut issue = issues.get_mut(&id)?;
    let (desc_id, issue_desc) = issue.description();
    let desc_id = *desc_id;

    if title.is_some() || description.is_some() {
        // Editing by command line arguments
        issue.transaction("Edit", signer, |tx| {
            if let Some(t) = title {
                tx.edit(t)?;
            }
            if let Some(d) = description {
                tx.edit_comment(desc_id, d)?;
            }

            Ok(())
        })?;
        return Ok(());
    }

    // Editing by editor
    let tags: Vec<_> = issue.tags().cloned().collect();
    let assigned: Vec<_> = issue.assigned().collect();

    let Some((meta, description)) = prompt_issue(
        issue.title(),
        issue_desc,
        &tags,
        &assigned,
    )? else {
        return Ok(());
    };

    issue.transaction("Edit", signer, |tx| {
        tx.edit(meta.title)?;
        tx.edit_comment(desc_id, description)?;

        let add: Vec<_> = meta
            .tags
            .iter()
            .filter(|t| !tags.contains(t))
            .cloned()
            .collect();
        let remove: Vec<_> = tags
            .iter()
            .filter(|t| !meta.tags.contains(t))
            .cloned()
            .collect();
        tx.tag(add, remove)?;

        let assign: Vec<_> = meta
            .assignees
            .iter()
            .filter(|t| !assigned.contains(t))
            .cloned()
            .map(cob::ActorId::from)
            .collect();
        let unassign: Vec<_> = assigned
            .iter()
            .filter(|t| !meta.assignees.contains(t))
            .cloned()
            .map(cob::ActorId::from)
            .collect();
        tx.assign(assign, unassign)?;

        Ok(())
    })?;

    show_issue(&issue, &id)?;

    Ok(())
}

fn show_issue(issue: &issue::Issue, id: &cob::ObjectId) -> anyhow::Result<()> {
    let tags: Vec<String> = issue.tags().cloned().map(|t| t.into()).collect();
    let assignees: Vec<String> = issue
        .assigned()
        .map(|a| term::format::did(&a).to_string())
        .collect();

    let mut attrs = Table::<2, Paint<String>>::new(TableOptions {
        spacing: 2,
        ..TableOptions::default()
    });

    attrs.push([
        term::format::tertiary("Title".to_owned()),
        term::format::bold(issue.title().to_owned()),
    ]);

    attrs.push([
        term::format::tertiary("Issue".to_owned()),
        term::format::bold(id.to_string()),
    ]);

    if !tags.is_empty() {
        attrs.push([
            term::format::tertiary("Tags".to_owned()),
            term::format::secondary(tags.join(", ")),
        ]);
    }

    if !assignees.is_empty() {
        attrs.push([
            term::format::tertiary("Assignees".to_owned()),
            term::format::dim(assignees.join(", ")),
        ]);
    }

    attrs.push([
        term::format::tertiary("Status".to_owned()),
        match issue.state() {
            issue::State::Open => term::format::positive("open".to_owned()),
            issue::State::Closed { reason } => term::format::default(format!(
                "{} {}",
                term::format::negative("closed"),
                term::format::default(format!("as {reason}"))
            )),
        },
    ]);

    let (_, description) = issue.description();
    let widget = VStack::default()
        .border(Some(term::colors::FAINT))
        .child(attrs)
        .children(if !description.is_empty() {
            vec![
                term::Label::blank().boxed(),
                term::textarea(term::format::dim(description)).boxed(),
            ]
        } else {
            vec![]
        });

    widget.print();

    Ok(())
}

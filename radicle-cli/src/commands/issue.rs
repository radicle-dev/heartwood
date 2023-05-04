#![allow(clippy::or_fun_call)]
use std::ffi::OsString;
use std::str::FromStr;

use anyhow::{anyhow, Context as _};

use radicle::cob::common::{Reaction, Tag};
use radicle::cob::issue;
use radicle::cob::issue::{CloseReason, Issues, State};
use radicle::crypto::Signer;
use radicle::node::Handle;
use radicle::prelude::Did;
use radicle::profile;
use radicle::storage::WriteStorage;
use radicle::{cob, Node};
use radicle_term::table::TableOptions;
use radicle_term::{Paint, Table, VStack};

use crate::git::Rev;
use crate::terminal as term;
use crate::terminal::args::{string, Args, Error, Help};
use crate::terminal::Element;

pub const HELP: Help = Help {
    name: "issue",
    description: "Manage issues",
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad issue [<option>...]
    rad issue delete <issue-id> [<option>...]
    rad issue list [--assigned <did>] [<option>...]
    rad issue open [--title <title>] [--description <text>] [--tag <tag>] [<option>...]
    rad issue react <issue-id> [--emoji <char>] [<option>...]
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
    },
    List {
        assigned: Option<Assigned>,
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
            },
            OperationName::Delete => Operation::Delete {
                id: id.ok_or_else(|| anyhow!("an issue to remove must be provided"))?,
            },
            OperationName::List => Operation::List { assigned },
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
        Operation::Open {
            title: Some(title),
            description: Some(description),
            tags,
        } => {
            let issue = issues.create(title, description, tags.as_slice(), &[], &signer)?;
            if !options.quiet {
                show_issue(&issue)?;
            }
        }
        Operation::Show { id } => {
            let id = id.resolve(&repo.backend)?;
            let issue = issues
                .get(&id)?
                .context("No issue with the given ID exists")?;
            show_issue(&issue)?;
        }
        Operation::State { id, state } => {
            let id = id.resolve(&repo.backend)?;
            let mut issue = issues.get_mut(&id)?;
            issue.lifecycle(state, &signer)?;
        }
        Operation::React { id, reaction } => {
            let id = id.resolve(&repo.backend)?;
            if let Ok(mut issue) = issues.get_mut(&id) {
                let (comment_id, _) = term::io::comment_select(&issue).unwrap();
                issue.react(*comment_id, reaction, &signer)?;
            }
        }
        Operation::Open {
            ref title,
            ref description,
            ref tags,
        } => {
            open(
                &mut issues,
                &signer,
                &options,
                title.clone(),
                description.clone(),
                tags.to_vec(),
            )?;
        }
        Operation::List { assigned } => {
            list(&issues, &profile, &assigned)?;
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

fn list(
    issues: &Issues,
    profile: &profile::Profile,
    assigned: &Option<Assigned>,
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
        let (id, issue, _) = result?;

        if Some(true) == assignee.map(|a| !issue.assigned().any(|v| v == Did::from(a))) {
            continue;
        }

        all.push((id, issue))
    }

    all.sort_by(|(id1, i1), (id2, i2)| {
        let by_timestamp = i2.timestamp().cmp(&i1.timestamp());
        let by_id = id1.cmp(id2);

        by_timestamp.then(by_id)
    });

    let mut t = term::Table::new(term::table::TableOptions::bordered());
    t.push([
        term::format::dim(String::from("●")),
        term::format::bold(String::from("ID")),
        term::format::bold(String::from("Title")),
        term::format::bold(String::from("Author")),
        term::format::bold(String::from("Tags")),
        term::format::bold(String::from("Assignees")),
        term::format::bold(String::from("Opened")),
    ]);
    t.divider();

    for (id, issue) in all {
        let assigned: String = issue
            .assigned()
            .map(|p| term::format::did(&p).to_string())
            .collect::<Vec<_>>()
            .join(", ");

        let mut tags = issue.tags().map(|t| t.to_string()).collect::<Vec<_>>();
        tags.sort();

        t.push([
            match issue.state() {
                State::Open => term::format::positive("●").into(),
                State::Closed { .. } => term::format::negative("●").into(),
            },
            term::format::tertiary(term::format::cob(&id)).to_owned(),
            term::format::default(issue.title().to_owned()),
            term::format::did(&issue.author().id).dim(),
            term::format::secondary(tags.join(", ")),
            if assigned.is_empty() {
                term::format::dim(String::default())
            } else {
                term::format::default(assigned.to_string())
            },
            term::format::timestamp(&issue.timestamp()).dim().italic(),
        ]);
    }
    t.print();

    Ok(())
}

fn open<G: Signer>(
    issues: &mut Issues,
    signer: &G,
    options: &Options,
    title: Option<String>,
    description: Option<String>,
    tags: Vec<Tag>,
) -> anyhow::Result<()> {
    let meta = Metadata {
        title: title.unwrap_or("Enter a title".to_owned()),
        tags,
        assignees: vec![],
    };
    let yaml = serde_yaml::to_string(&meta)?;
    let doc = format!(
        "{}---\n\n{}",
        yaml,
        description.unwrap_or("Enter a description...".to_owned())
    );

    if let Some(text) = term::Editor::new().edit(&doc)? {
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

        let description: String = lines.collect::<Vec<&str>>().join("\n");
        let meta: Metadata =
            serde_yaml::from_str(&meta).context("failed to parse yaml front-matter")?;

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
            show_issue(&issue)?;
        }
    }

    Ok(())
}

fn show_issue(issue: &issue::Issue) -> anyhow::Result<()> {
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

    let description = issue.description().unwrap_or_default();
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

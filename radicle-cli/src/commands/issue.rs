#![allow(clippy::or_fun_call)]
use std::ffi::OsString;
use std::str::FromStr;

use anyhow::{anyhow, Context as _};

use radicle::cob::common::{Reaction, Tag};
use radicle::cob::issue;
use radicle::cob::issue::{CloseReason, Issues, State};
use radicle::node::Handle;
use radicle::prelude::Did;
use radicle::storage::WriteStorage;
use radicle::{cob, Node};

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

        Ok((Options { op, announce }, vec![]))
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
            show_issue(&issue)?;
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
            title,
            description,
            tags,
        } => {
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
                    &signer,
                )?;
                show_issue(&issue)?;
            }
        }
        Operation::List { assigned } => {
            let assignee = match assigned {
                Some(Assigned::Me) => Some(*profile.id()),
                Some(Assigned::Peer(id)) => Some(id.into()),
                None => None,
            };

            let mut t = term::Table::new(term::table::TableOptions::default());
            for result in issues.all()? {
                let (id, issue, _) = result?;
                let assigned: Vec<_> = issue.assigned().collect();

                if Some(true) == assignee.map(|a| !assigned.contains(&Did::from(a))) {
                    continue;
                }

                let assigned: String = assigned
                    .iter()
                    .map(|p| p.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                t.push([
                    id.to_string(),
                    format!("{:?}", issue.title()),
                    if assigned.is_empty() {
                        String::from("❲unassigned❳")
                    } else {
                        assigned.to_string()
                    },
                ]);
            }
            t.print();
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

fn show_issue(issue: &issue::Issue) -> anyhow::Result<()> {
    let tags: Vec<String> = issue.tags().cloned().map(|t| t.into()).collect();
    let assignees: Vec<String> = issue.assigned().map(|a| a.to_string()).collect();

    term::info!("title: {}", issue.title());
    term::info!("state: {}", issue.state());
    term::info!("tags: [{}]", tags.join(", "));
    term::info!("assignees: [{}]", assignees.join(", "));
    term::blank();
    term::info!("{}", issue.description().unwrap_or_default());

    Ok(())
}

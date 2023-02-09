#![allow(clippy::or_fun_call)]
use std::ffi::OsString;
use std::str::FromStr;

use anyhow::{anyhow, Context as _};

use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};

use radicle::cob;
use radicle::cob::common::{Reaction, Tag};
use radicle::cob::issue;
use radicle::cob::issue::{CloseReason, IssueId, Issues, State};
use radicle::storage::WriteStorage;

pub const HELP: Help = Help {
    name: "issue",
    description: "Manage issues",
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad issue
    rad issue delete <id>
    rad issue list [--assigned <key>]
    rad issue open [--title <title>] [--description <text>]
    rad issue react <id> [--emoji <char>]
    rad issue show <id>
    rad issue state <id> [--closed | --open | --solved]

Options

    --help      Print help
"#,
};

#[derive(serde::Deserialize, serde::Serialize, Debug)]
pub struct Metadata {
    title: String,
    labels: Vec<Tag>,
    assignees: Vec<cob::ActorId>,
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
    Peer(cob::ActorId),
}

#[derive(Debug, PartialEq, Eq)]
pub enum Operation {
    Open {
        title: Option<String>,
        description: Option<String>,
    },
    Show {
        id: IssueId,
    },
    State {
        id: IssueId,
        state: State,
    },
    Delete {
        id: IssueId,
    },
    React {
        id: IssueId,
        reaction: Reaction,
    },
    List {
        assigned: Option<Assigned>,
    },
}

#[derive(Debug)]
pub struct Options {
    pub op: Operation,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut op: Option<OperationName> = None;
        let mut id: Option<IssueId> = None;
        let mut assigned: Option<Assigned> = None;
        let mut title: Option<String> = None;
        let mut reaction: Option<Reaction> = None;
        let mut description: Option<String> = None;
        let mut state: Option<State> = None;

        while let Some(arg) = parser.next()? {
            match arg {
                Long("help") => {
                    return Err(Error::Help.into());
                }
                Long("title") if op == Some(OperationName::Open) => {
                    title = Some(parser.value()?.to_string_lossy().into());
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
                        let val = val.to_string_lossy();
                        let Ok(peer) = cob::ActorId::from_str(&val) else {
                            return Err(anyhow!("invalid peer ID '{}'", val));
                        };
                        assigned = Some(Assigned::Peer(peer));
                    } else {
                        assigned = Some(Assigned::Me);
                    }
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
                    let val = val
                        .to_str()
                        .ok_or_else(|| anyhow!("issue id specified is not UTF-8"))?;

                    id = Some(
                        IssueId::from_str(val)
                            .map_err(|_| anyhow!("invalid issue id '{}'", val))?,
                    );
                }
                _ => {
                    return Err(anyhow!(arg.unexpected()));
                }
            }
        }

        let op = match op.unwrap_or_default() {
            OperationName::Open => Operation::Open { title, description },
            OperationName::Show => Operation::Show {
                id: id.ok_or_else(|| anyhow!("an issue id must be provided"))?,
            },
            OperationName::State => Operation::State {
                id: id.ok_or_else(|| anyhow!("an issue id must be provided"))?,
                state: state.ok_or_else(|| anyhow!("a state operation must be provided"))?,
            },
            OperationName::React => Operation::React {
                id: id.ok_or_else(|| anyhow!("an issue id must be provided"))?,
                reaction: reaction.ok_or_else(|| anyhow!("a reaction emoji must be provided"))?,
            },
            OperationName::Delete => Operation::Delete {
                id: id.ok_or_else(|| anyhow!("an issue id to remove must be provided"))?,
            },
            OperationName::List => Operation::List { assigned },
        };

        Ok((Options { op }, vec![]))
    }
}

pub fn run(options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    let signer = term::signer(&profile)?;
    let storage = &profile.storage;
    let (_, id) = radicle::rad::cwd()?;
    let repo = storage.repository(id)?;
    let mut issues = Issues::open(*signer.public_key(), &repo)?;

    match options.op {
        Operation::Open {
            title: Some(title),
            description: Some(description),
        } => {
            issues.create(title, description, &[], &[], &signer)?;
        }
        Operation::Show { id } => {
            let issue = issues
                .get(&id)?
                .context("No issue with the given ID exists")?;
            show_issue(&issue)?;
        }
        Operation::State { id, state } => {
            let mut issue = issues.get_mut(&id)?;
            issue.lifecycle(state, &signer)?;
        }
        Operation::React { id, reaction } => {
            if let Ok(mut issue) = issues.get_mut(&id) {
                let comment_id = term::comment_select(&issue).unwrap();
                issue.react(comment_id, reaction, &signer)?;
            }
        }
        Operation::Open { title, description } => {
            let meta = Metadata {
                title: title.unwrap_or("Enter a title".to_owned()),
                labels: vec![],
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

                issues.create(
                    &meta.title,
                    description.trim(),
                    meta.labels.as_slice(),
                    meta.assignees.as_slice(),
                    &signer,
                )?;
            }
        }
        Operation::List { assigned } => {
            let assignee = match assigned {
                Some(Assigned::Me) => Some(*profile.id()),
                Some(Assigned::Peer(id)) => Some(id),
                None => None,
            };

            let mut t = term::Table::new(term::table::TableOptions::default());
            for result in issues.all()? {
                let (id, issue, _) = result?;
                let assigned: Vec<_> = issue.assigned().collect();

                if Some(true) == assignee.map(|a| !assigned.contains(&&a)) {
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
                    assigned.to_string(),
                ]);
            }
            t.render();
        }
        Operation::Delete { id } => {
            issues.remove(&id)?;
        }
    }

    Ok(())
}

fn show_issue(issue: &issue::Issue) -> anyhow::Result<()> {
    term::info!("title: {}", issue.title());
    term::info!("state: {}", issue.state());

    let tags: Vec<String> = issue.tags().cloned().map(|t| t.into()).collect();
    term::info!("tags: {}", tags.join(", "));

    let assignees: Vec<String> = issue.assigned().map(|a| a.to_string()).collect();
    term::info!("assignees: {}", assignees.join(", "));

    term::info!("{}", issue.description().unwrap_or(""));
    Ok(())
}

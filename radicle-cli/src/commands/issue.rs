#![allow(clippy::or_fun_call)]
use std::ffi::OsString;
use std::str::FromStr;

use anyhow::{anyhow, Context as _};

use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};

use radicle::cob::common::{Reaction, Tag};
use radicle::cob::issue::{CloseReason, IssueId, Issues, Status};
use radicle::storage::WriteStorage;

pub const HELP: Help = Help {
    name: "issue",
    description: "Manage issues",
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad issue new [--title <title>] [--description <text>]
    rad issue state <id> [--closed | --open | --solved]
    rad issue delete <id>
    rad issue react <id> [--emoji <char>]
    rad issue list

Options

    --help      Print help
"#,
};

#[derive(serde::Deserialize, serde::Serialize, Debug)]
pub struct Metadata {
    title: String,
    labels: Vec<Tag>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum OperationName {
    Create,
    State,
    React,
    Delete,
    List,
}

impl Default for OperationName {
    fn default() -> Self {
        Self::List
    }
}

#[derive(Debug)]
pub enum Operation {
    Create {
        title: Option<String>,
        description: Option<String>,
    },
    State {
        id: IssueId,
        state: Status,
    },
    Delete {
        id: IssueId,
    },
    React {
        id: IssueId,
        reaction: Reaction,
    },
    List,
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
        let mut title: Option<String> = None;
        let mut reaction: Option<Reaction> = None;
        let mut description: Option<String> = None;
        let mut state: Option<Status> = None;

        while let Some(arg) = parser.next()? {
            match arg {
                Long("help") => {
                    return Err(Error::Help.into());
                }
                Long("title") if op == Some(OperationName::Create) => {
                    title = Some(parser.value()?.to_string_lossy().into());
                }
                Long("closed") if op == Some(OperationName::State) => {
                    state = Some(Status::Closed {
                        reason: CloseReason::Other,
                    });
                }
                Long("open") if op == Some(OperationName::State) => {
                    state = Some(Status::Open);
                }
                Long("solved") if op == Some(OperationName::State) => {
                    state = Some(Status::Closed {
                        reason: CloseReason::Solved,
                    });
                }
                Long("reaction") if op == Some(OperationName::React) => {
                    if let Some(emoji) = parser.value()?.to_str() {
                        reaction =
                            Some(Reaction::from_str(emoji).map_err(|_| anyhow!("invalid emoji"))?);
                    }
                }
                Long("description") if op == Some(OperationName::Create) => {
                    description = Some(parser.value()?.to_string_lossy().into());
                }
                Value(val) if op.is_none() => match val.to_string_lossy().as_ref() {
                    "n" | "new" => op = Some(OperationName::Create),
                    "s" | "state" => op = Some(OperationName::State),
                    "d" | "delete" => op = Some(OperationName::Delete),
                    "l" | "list" => op = Some(OperationName::List),
                    "r" | "react" => op = Some(OperationName::React),

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
            OperationName::Create => Operation::Create { title, description },
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
            OperationName::List => Operation::List,
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
        Operation::Create {
            title: Some(title),
            description: Some(description),
        } => {
            issues.create(title, description, &[], &signer)?;
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
        Operation::Create { title, description } => {
            let meta = Metadata {
                title: title.unwrap_or("Enter a title".to_owned()),
                labels: vec![],
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
                    &signer,
                )?;
            }
        }
        Operation::List => {
            for result in issues.all()? {
                let (id, issue) = result?;
                println!("{} {}", id, issue.title());
            }
        }
        Operation::Delete { id } => {
            issues.remove(&id)?;
        }
    }

    Ok(())
}

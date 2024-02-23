use std::ffi::OsString;

use anyhow::anyhow;
use clap::{CommandFactory, FromArgMatches, Parser, Subcommand, ValueHint};
use clap_complete::dynamic::shells::CompleteCommand;
use radicle::issue::Issues;
use radicle::storage::WriteStorage;

use crate::terminal as term;
use crate::commands::rad_issue;
use crate::git::Rev;

#[derive(Parser, Debug)]
#[command(name = "rad")]
#[command(about = "The rad CLI", long_about = None)]
struct CliArgs {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Issue(IssueArgs),
}

#[derive(Parser, Debug)]
struct IssueArgs {
    #[command(subcommand)]
    command: Option<IssueCommands>,
}

#[derive(Subcommand, Debug)]
enum IssueCommands {
    List,
    Show {
        #[clap(value_hint = ISSUE_ID_HINT)]
        id: String,
    },
}

const ISSUE_ID_HINT: ValueHint = ValueHint::Dynamic(get_issue_ids);

fn get_issue_ids(input: &str) -> Option<Vec<String>> {
    let profile = term::profile().ok()?;

    let (_, rid) = radicle::rad::cwd().ok()?;
    let repo = profile.storage.repository_mut(rid).ok()?;
    let issues = Issues::open(&repo).ok()?;

    let completions = issues.all().ok()?
        .filter_map(|issue| {
            if let Ok((id, _)) = issue {
                let id = id.to_string();
                if id.starts_with(input) {
                    return Some(String::from(id.split_at(8).0));
                }
            }
            None
        })
        .collect::<Vec<_>>();

    Some(completions)
}

pub fn completer(args: Vec<OsString>) -> () {
    let cmd = CliArgs::command();
    let mut cmd = CompleteCommand::augment_subcommands(cmd);
    let matches = cmd.clone().get_matches();

    if let Ok(completions) = CompleteCommand::from_arg_matches(&matches) {
        completions.complete(&mut cmd);
    } else {
        dbg!("COMPLETER FAILED: args: {:?}, matches: {:?}", args, matches);
    }
}

pub fn to_issue_options() -> anyhow::Result<rad_issue::Options> {
    let args = CliArgs::parse();

    let options = match args {
        CliArgs { command } => match command {
            Some(Commands::Issue(IssueArgs {
                command: subcommand,
            })) => match subcommand {
                Some(IssueCommands::List) => Some(rad_issue::Options {
                    op: rad_issue::Operation::List {
                        assigned: None,
                        state: None,
                    },
                    announce: true,
                    quiet: false,
                }),
                Some(IssueCommands::Show { id }) => Some(rad_issue::Options {
                    op: rad_issue::Operation::Show {
                        id: Rev::from(id),
                        format: term::issue::Format::Full,
                        debug: false,
                    },
                    announce: true,
                    quiet: false,
                }),
                _ => None,
            },
            _ => None,
        },
    };

    options.ok_or(anyhow!("Command not implemented FIXME!"))
}

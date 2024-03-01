use std::ffi::OsString;

use clap::{Command, FromArgMatches, Subcommand};
use clap_complete::dynamic::shells::CompleteCommand;
use radicle::identity::Did;
use radicle::issue::Issues;
use radicle::storage::{ReadRepository, ReadStorage};

pub fn get_assignee_did_hints(input: &str) -> Option<Vec<String>> {
    let (_, rid) = radicle::rad::cwd().ok()?;
    radicle::Profile::load()
        .ok()
        .and_then(|profile| profile.storage.repository(rid).ok())
        .and_then(|repo| {
            Issues::open(&repo).ok().and_then(|issues| {
                issues
                    .all()
                    .map(|issues| {
                        issues
                            .flat_map(|issue| {
                                issue.map_or(vec![], |(_, issue)| {
                                    issue.assignees().cloned().collect::<Vec<_>>()
                                })
                            })
                            .filter_map(|did| {
                                let did = did.to_human();
                                did.starts_with(input).then(|| String::from(did))
                            })
                            .collect::<Vec<_>>()
                    })
                    .ok()
            })
        })
}

pub fn get_issue_id_hints(input: &str) -> Option<Vec<String>> {
    let (_, rid) = radicle::rad::cwd().ok()?;
    radicle::Profile::load()
        .ok()
        .and_then(|profile| profile.storage.repository(rid).ok())
        .and_then(|repo| {
            Issues::open(&repo).ok().and_then(|issues| {
                issues
                    .all()
                    .map(|issues| {
                        issues
                            .filter_map(|issue| {
                                if let Ok((id, _)) = issue {
                                    let id = id.to_string();
                                    if id.starts_with(input) {
                                        return Some(String::from(id.split_at(8).0));
                                    }
                                }
                                None
                            })
                            .collect::<Vec<_>>()
                    })
                    .ok()
            })
        })
}

pub fn get_did_hints<R: ReadRepository + radicle::cob::Store>(input: &str) -> Option<Vec<String>> {
    let (_, rid) = radicle::rad::cwd().ok()?;
    radicle::Profile::load()
        .ok()
        .and_then(|profile| profile.storage.repository(rid).ok())
        .and_then(|repo| {
            repo.remote_ids()
                .map(|issues| {
                    issues
                        .filter_map(|id| {
                            let id = id.map(|id| Did::from(id).to_human()).ok()?;
                            id.starts_with(input).then_some(id)
                        })
                        .collect::<Vec<_>>()
                })
                .ok()
        })
}

pub fn completer(cmd: Command, args: Vec<OsString>) -> () {
    let mut cmd = CompleteCommand::augment_subcommands(cmd);
    let matches = cmd.clone().get_matches();

    if let Ok(completions) = CompleteCommand::from_arg_matches(&matches) {
        completions.complete(&mut cmd);
    } else {
        dbg!("COMPLETER FAILED: args: {:?}, matches: {:?}", args, matches);
    }
}

// pub fn to_issue_options() -> anyhow::Result<rad_issue::Options> {
//     let args = CliArgs::parse();

//     // Default to List command.
//     // let command = args.command.unwrap_or(Commands::Issue(IssueArgs {
//     //     command: Some(IssueCommands::List(ListArgs {
//     //         assigned: None,
//     //         filter: ListFilter::All,
//     //     })),
//     //     header: false,
//     //     quiet: false,
//     //     no_announce: false,
//     // }));

//     let options = match args {
//         CliArgs { command } => match command {
//             Some(Commands::Issue(IssueArgs {
//                 command: subcommand,
//                 quiet,
//                 no_announce,
//                 header,
//             })) => match subcommand {
//                 Some(IssueCommands::List(args)) => Some(rad_issue::Options {
//                     op: rad_issue::Operation::List {
//                         assigned: args
//                             .assigned
//                             .map(|did| {
//                                 let did =
//                                     term::args::did(&OsString::from(format!("did:key:{did}")))?;
//                                 Ok::<Option<Assigned>, anyhow::Error>(Some(Assigned::Peer(did)))
//                             })
//                             .unwrap_or(Ok(None))?,
//                         state: match args.filter {
//                             ListFilter::All => None,
//                             ListFilter::Open => Some(radicle::cob::issue::State::Open),
//                             ListFilter::Closed => Some(State::Closed {
//                                 reason: CloseReason::Other,
//                             }),
//                             ListFilter::Solved => Some(State::Closed {
//                                 reason: CloseReason::Solved,
//                             }),
//                         },
//                     },
//                     announce: !no_announce,
//                     quiet: quiet,
//                 }),
//                 Some(IssueCommands::Show(args)) => Some(rad_issue::Options {
//                     op: rad_issue::Operation::Show {
//                         id: Rev::from(args.id),
//                         format: if header {
//                             term::issue::Format::Header
//                         } else {
//                             term::issue::Format::Full
//                         },
//                         debug: args.debug,
//                     },
//                     announce: !no_announce,
//                     quiet: quiet,
//                 }),
//                 // Default `issue` subcommand is `list`.
//                 _ => Some(rad_issue::Options {
//                     op: rad_issue::Operation::List {
//                         assigned: None,
//                         state: None,
//                     },
//                     announce: false,
//                     quiet: false,
//                 }),
//             },
//             _ => None,
//         },
//     };

//     let Some(option) = options else {
//         CliArgs::command().write_help(&mut std::io::stdout())?;
//         println!();
//         process::exit(0);
//     };

//     Ok(option)
// }

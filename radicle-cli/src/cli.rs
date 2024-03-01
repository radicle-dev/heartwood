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

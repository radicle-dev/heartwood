#![allow(clippy::or_fun_call)]
use std::ffi::OsString;

use anyhow::{anyhow, Context as _};

use radicle::cob::job::{JobStore, Reason, State};
use radicle::crypto::Signer;
use radicle::node::Handle;
use radicle::storage::{WriteRepository, WriteStorage};
use radicle::{cob, Node};

use crate::git::Rev;
use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};
use crate::terminal::Element;

pub const HELP: Help = Help {
    name: "job",
    description: "Manage job COB: information about automated jobs on repository",
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad job [<option>...]
    rad job trigger <commit-id>
    rad job start <job-id> <run-id> [ <URL> ]
    rad job list
    rad job show <job-id>
    rad job finish <job-id> [--success | --failed ]
    rad job delete <job-id>

Options

    --no-announce     Don't announce job records to peers
    --quiet, -q       Don't print anything
    --help            Print help
"#,
};

#[derive(Default, Debug, PartialEq, Eq)]
pub enum OperationName {
    Trigger,
    Start,
    #[default]
    List,
    Show,
    Finish,
    Delete,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Operation {
    Trigger {
        commit: Rev,
    },
    Start {
        job_id: Rev,
        run_id: String,
        info_url: Option<String>,
    },
    List,
    Show {
        job_id: Rev,
    },
    Finish {
        job_id: Rev,
        reason: Reason,
    },
    Delete {
        job_id: Rev,
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
        let mut commit: Option<Rev> = None;
        let mut job_id: Option<Rev> = None;
        let mut run_id: Option<String> = None;
        let mut info_url: Option<String> = None;
        let mut announce = true;
        let mut quiet = false;
        let mut succeeded = false;
        let mut failed = false;

        while let Some(arg) = parser.next()? {
            match arg {
                Long("help") | Short('h') => {
                    return Err(Error::Help.into());
                }
                Long("no-announce") => {
                    announce = false;
                }
                Long("quiet") | Short('q') => {
                    quiet = true;
                }
                Long("success") | Long("succeeded") | Short('s') => {
                    succeeded = true;
                }
                Long("failure") | Long("failed") | Short('f') => {
                    failed = true;
                }
                Value(val) if op.is_none() => match val.to_string_lossy().as_ref() {
                    "trigger" => op = Some(OperationName::Trigger),
                    "start" => op = Some(OperationName::Start),
                    "list" => op = Some(OperationName::List),
                    "show" => op = Some(OperationName::Show),
                    "finish" => op = Some(OperationName::Finish),
                    "delete" => op = Some(OperationName::Delete),

                    unknown => anyhow::bail!("unknown operation '{}'", unknown),
                },
                Value(val) if commit.is_none() && op == Some(OperationName::Trigger) => {
                    let val = term::args::oid(&val)?;
                    let val = Rev::from(val.to_string());
                    commit = Some(val);
                }
                Value(val)
                    if job_id.is_none()
                        && op.is_some()
                        && matches!(
                            op.as_ref().unwrap(),
                            OperationName::Start
                                | OperationName::Show
                                | OperationName::Finish
                                | OperationName::Delete
                        ) =>
                {
                    let val = term::args::oid(&val)?;
                    let val = Rev::from(val.to_string());
                    job_id = Some(val);
                }
                Value(val)
                    if job_id.is_some()
                        && run_id.is_none()
                        && op.is_some()
                        && matches!(op.as_ref().unwrap(), OperationName::Start) =>
                {
                    run_id = Some(val.to_str().unwrap().to_string());
                }
                Value(val)
                    if job_id.is_some()
                        && run_id.is_some()
                        && op.is_some()
                        && matches!(op.as_ref().unwrap(), OperationName::Start) =>
                {
                    info_url = Some(val.to_str().unwrap().to_string());
                }
                _ => {
                    return Err(anyhow!(arg.unexpected()));
                }
            }
        }

        let op = match op.unwrap_or_default() {
            OperationName::Trigger => Operation::Trigger {
                commit: commit.ok_or_else(|| anyhow!("a commit id remove must be provided"))?,
            },
            OperationName::Start => Operation::Start {
                job_id: job_id.ok_or_else(|| anyhow!("a job id must be provided"))?,
                run_id: run_id.ok_or_else(|| anyhow!("a run id must be provided"))?,
                info_url,
            },
            OperationName::List => Operation::List,
            OperationName::Show => Operation::Show {
                job_id: job_id.ok_or_else(|| anyhow!("a job id must be provided"))?,
            },
            OperationName::Finish => Operation::Finish {
                job_id: job_id.ok_or_else(|| anyhow!("a job id must be provided"))?,
                reason: if !succeeded && !failed {
                    return Err(anyhow!("must give one of --success or --failure"))?;
                } else if succeeded && failed {
                    return Err(anyhow!("must give one of --success or --failure, not both"))?;
                } else if succeeded {
                    Reason::Succeeded
                } else {
                    Reason::Failed
                },
            },
            OperationName::Delete => Operation::Delete {
                job_id: job_id.ok_or_else(|| anyhow!("a job id to remove must be provided"))?,
            },
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
            Operation::Trigger { .. }
                | Operation::Start { .. }
                | Operation::Finish { .. }
                | Operation::Delete { .. }
        );

    let mut node = Node::new(profile.socket());
    let mut ci_store = JobStore::open(&repo)?;

    match options.op {
        Operation::Trigger { commit } => {
            trigger(&commit, &mut ci_store, &repo, &signer, options.quiet)?;
        }
        Operation::Start {
            job_id,
            run_id,
            info_url,
        } => {
            start(&job_id, &run_id, info_url, &mut ci_store, &repo, &signer)?;
        }
        Operation::List => {
            list(&ci_store)?;
        }
        Operation::Show { job_id } => {
            show(&job_id, &ci_store, &repo)?;
        }
        Operation::Finish { job_id, reason } => {
            finish(&job_id, reason, &mut ci_store, &repo, &signer)?;
        }
        Operation::Delete { job_id } => {
            let job_id = job_id.resolve(&repo.backend)?;
            ci_store.remove(&job_id, &signer)?;
        }
    }

    if announce {
        match node.announce_refs(rid) {
            Ok(_) => {}
            Err(e) if e.is_connection_err() => {}
            Err(e) => return Err(e.into()),
        }
    }

    Ok(())
}

fn trigger<R: WriteRepository + cob::Store, G: Signer>(
    commit: &Rev,
    store: &mut JobStore<R>,
    repo: &radicle::storage::git::Repository,
    signer: &G,
    quiet: bool,
) -> anyhow::Result<()> {
    let commit = commit.resolve(&repo.backend)?;
    let job = store.create(commit, signer)?;
    if !quiet {
        term::job::show(&job, job.id())?;
    }
    Ok(())
}

fn start<R: WriteRepository + cob::Store, G: Signer>(
    job_id: &Rev,
    run_id: &str,
    info_url: Option<String>,
    store: &mut JobStore<R>,
    repo: &radicle::storage::git::Repository,
    signer: &G,
) -> anyhow::Result<()> {
    let cob_id = job_id.resolve(&repo.backend)?;
    let mut job = store.get_mut(&cob_id)?;
    job.start(run_id.to_string(), info_url, signer)?;
    Ok(())
}

fn list<R: WriteRepository + cob::Store>(store: &JobStore<R>) -> anyhow::Result<()> {
    if store.is_empty()? {
        term::print(term::format::italic("Nothing to show."));
        return Ok(());
    }

    let mut all = Vec::new();
    for result in store.all()? {
        let Ok((id, ci)) = result else {
            // Skip COBs that failed to load.
            continue;
        };
        all.push((id, ci))
    }

    let mut table = term::Table::new(term::table::TableOptions::bordered());
    table.push([
        term::format::dim(String::from("●")),
        term::format::bold(String::from("ID")),
        term::format::bold(String::from("Commit")),
        term::format::bold(String::from("State")),
    ]);
    table.divider();

    for (id, ci) in all {
        table.push([
            match ci.state() {
                State::Fresh => term::format::positive("●").into(),
                State::Running => term::format::positive("●").into(),
                State::Finished(Reason::Succeeded) => term::format::positive("●").into(),
                State::Finished(Reason::Failed) => term::format::negative("●").into(),
            },
            term::format::tertiary(term::format::cob_long(&id).to_string()),
            term::format::tertiary(term::format::oid(ci.commit()).to_string()),
            term::format::tertiary(term::format::job_state(ci.state()).to_string()),
        ]);
    }
    table.print();

    Ok(())
}

fn show<R: WriteRepository + cob::Store>(
    cob_id: &Rev,
    store: &JobStore<R>,
    repo: &radicle::storage::git::Repository,
) -> anyhow::Result<()> {
    let cob_id = cob_id.resolve(&repo.backend)?;
    let job = store
        .get(&cob_id)?
        .context("No job with the given ID exists")?;
    term::job::show(&job, &cob_id)?;
    Ok(())
}

fn finish<R: WriteRepository + cob::Store, G: Signer>(
    cob_id: &Rev,
    reason: Reason,
    store: &mut JobStore<R>,
    repo: &radicle::storage::git::Repository,
    signer: &G,
) -> anyhow::Result<()> {
    let cob_id = cob_id.resolve(&repo.backend)?;
    let mut job = store.get_mut(&cob_id)?;
    job.finish(reason, signer)?;
    Ok(())
}

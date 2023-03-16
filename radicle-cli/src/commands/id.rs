use std::{ffi::OsString, str::FromStr as _};

use anyhow::{anyhow, Context as _};

use radicle::cob::identity::{self, Proposal, Proposals, Revision, RevisionId};
use radicle::git::Oid;
use radicle::identity::Identity;
use radicle::prelude::{Did, Doc};
use radicle::storage::ReadStorage as _;
use radicle_crypto::Verified;

use crate::git::Rev;
use crate::terminal as term;
use crate::terminal::args::{string, Args, Error, Help};
use crate::terminal::Element;
use crate::terminal::Interactive;

pub const HELP: Help = Help {
    name: "id",
    description: "Manage identity documents",
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad id (update|edit) [--title|-t] [--description|-d]
                         [--delegates <did>] [--threshold <num>]
                         [--no-confirm] [<option>...]
    rad id list [<option>...]
    rad id rebase <id> [--rev <revision-id>] [<option>...]
    rad id show <id> [--rev <revision-id>] [--revisions] [<option>...]
    rad id (accept|reject|close|commit) [--rev <revision-id>] [--no-confirm] [<option>...]

Options
    --help                 Print help
"#,
};

#[derive(serde::Deserialize, serde::Serialize, Debug)]
pub struct Metadata {
    title: String,
    description: String,
    proposed: Doc<Verified>,
}

impl Metadata {
    fn edit(self) -> anyhow::Result<Self> {
        let yaml = serde_yaml::to_string(&self)?;
        match term::Editor::new().edit(yaml)? {
            Some(meta) => Ok(serde_yaml::from_str(&meta).context("failed to parse proposal meta")?),
            None => return Err(anyhow!("Operation aborted!")),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub enum Operation {
    Accept {
        id: Rev,
        rev: Option<RevisionId>,
    },
    Reject {
        id: Rev,
        rev: Option<RevisionId>,
    },
    Edit {
        title: Option<String>,
        description: Option<String>,
        delegates: Vec<Did>,
        threshold: Option<usize>,
    },
    Update {
        id: Rev,
        rev: Option<RevisionId>,
        title: Option<String>,
        description: Option<String>,
        delegates: Vec<Did>,
        threshold: Option<usize>,
    },
    Rebase {
        id: Rev,
        rev: Option<RevisionId>,
    },
    Show {
        id: Rev,
        rev: Option<RevisionId>,
        show_revisions: bool,
    },
    #[default]
    List,
    Commit {
        id: Rev,
        rev: Option<RevisionId>,
    },
    Close {
        id: Rev,
    },
}

#[derive(Default, PartialEq, Eq)]
pub enum OperationName {
    Accept,
    Reject,
    Edit,
    Update,
    Rebase,
    Show,
    #[default]
    List,
    Commit,
    Close,
}

pub struct Options {
    pub op: Operation,
    pub interactive: Interactive,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut op: Option<OperationName> = None;
        let mut id: Option<Rev> = None;
        let mut rev: Option<RevisionId> = None;
        let mut title: Option<String> = None;
        let mut description: Option<String> = None;
        let mut delegates: Vec<Did> = Vec::new();
        let mut threshold: Option<usize> = None;
        let mut interactive = Interactive::Yes;
        let mut show_revisions = false;

        while let Some(arg) = parser.next()? {
            match arg {
                Long("help") => {
                    return Err(Error::Help.into());
                }
                Long("title") if op == Some(OperationName::Edit) => {
                    title = Some(parser.value()?.to_string_lossy().into());
                }
                Long("description") if op == Some(OperationName::Edit) => {
                    description = Some(parser.value()?.to_string_lossy().into());
                }
                Long("no-confirm") => {
                    interactive = Interactive::No;
                }
                Value(val) if op.is_none() => match val.to_string_lossy().as_ref() {
                    "e" | "edit" => op = Some(OperationName::Edit),
                    "u" | "update" => op = Some(OperationName::Update),
                    "rebase" => op = Some(OperationName::Rebase),
                    "l" | "list" => op = Some(OperationName::List),
                    "s" | "show" => op = Some(OperationName::Show),
                    "a" | "accept" => op = Some(OperationName::Accept),
                    "r" | "reject" => op = Some(OperationName::Reject),
                    "commit" => op = Some(OperationName::Commit),
                    "close" => op = Some(OperationName::Close),

                    unknown => anyhow::bail!("unknown operation '{}'", unknown),
                },
                Long("rev") => {
                    let val = String::from(parser.value()?.to_string_lossy());
                    rev = Some(
                        RevisionId::from_str(&val)
                            .map_err(|_| anyhow!("invalid revision '{}'", val))?,
                    );
                }
                Long("delegates") => {
                    let did = term::args::did(&parser.value()?)?;
                    delegates.push(did);
                }
                Long("threshold") => {
                    threshold = Some(parser.value()?.to_string_lossy().parse()?);
                }
                Long("revisions") => {
                    show_revisions = true;
                }
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
            OperationName::Accept => Operation::Accept {
                id: id.ok_or_else(|| anyhow!("a proposal must be provided"))?,
                rev,
            },
            OperationName::Reject => Operation::Reject {
                id: id.ok_or_else(|| anyhow!("a proposal must be provided"))?,
                rev,
            },
            OperationName::Edit => Operation::Edit {
                title,
                description,
                delegates,
                threshold,
            },
            OperationName::Update => Operation::Update {
                id: id.ok_or_else(|| anyhow!("a proposal must be provided"))?,
                rev,
                title,
                description,
                delegates,
                threshold,
            },
            OperationName::Rebase => Operation::Rebase {
                id: id.ok_or_else(|| anyhow!("a proposal must be provided"))?,
                rev,
            },
            OperationName::Show => Operation::Show {
                id: id.ok_or_else(|| anyhow!("a proposal must be provided"))?,
                rev,
                show_revisions,
            },
            OperationName::List => Operation::List,
            OperationName::Commit => Operation::Commit {
                id: id.ok_or_else(|| anyhow!("a proposal must be provided"))?,
                rev,
            },
            OperationName::Close => Operation::Close {
                id: id.ok_or_else(|| anyhow!("a proposal must be provided"))?,
            },
        };
        Ok((Options { op, interactive }, vec![]))
    }
}

pub fn run(options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    let signer = term::signer(&profile)?;
    let storage = &profile.storage;
    let (_, id) = radicle::rad::cwd()?;
    let repo = storage.repository(id)?;
    let mut proposals = Proposals::open(&repo)?;
    let previous = Identity::load(signer.public_key(), &repo)?;

    let interactive = &options.interactive;
    match options.op {
        Operation::Accept { id, rev } => {
            let id = id.resolve(&repo.backend)?;
            let mut proposal = proposals.get_mut(&id)?;
            let (rid, revision) = select(&proposal, rev, &previous, interactive)?;
            warn_out_of_date(revision, &previous);
            let yes = confirm(interactive, "Are you sure you want to accept?");
            if yes {
                let (_, signature) = revision.proposed.sign(&signer)?;
                proposal.accept(rid, signature, &signer)?;
                term::success!("Accepted proposal ✓");
                print(&proposal, &previous, None)?;
            }
        }
        Operation::Reject { id, rev } => {
            let id = id.resolve(&repo.backend)?;
            let mut proposal = proposals.get_mut(&id)?;
            let (rid, revision) = select(&proposal, rev, &previous, interactive)?;
            warn_out_of_date(revision, &previous);
            let yes = confirm(interactive, "Are you sure you want to reject?");
            if yes {
                proposal.reject(rid, &signer)?;
                term::success!("Rejected proposal 👎");
                print(&proposal, &previous, None)?;
            }
        }
        Operation::Edit {
            title,
            description,
            delegates,
            threshold,
        } => {
            let proposed = {
                let mut proposed = previous.doc.clone();
                proposed.threshold = threshold.unwrap_or(proposed.threshold);
                proposed.delegates.extend(delegates);
                proposed
            };

            let meta = Metadata {
                title: title.unwrap_or("Enter a title".to_owned()),
                description: description.unwrap_or("Enter a description".to_owned()),
                proposed,
            };
            let create = if interactive.yes() {
                meta.edit()?
            } else {
                meta
            };
            let proposal = proposals.create(
                create.title,
                create.description,
                previous.current,
                create.proposed,
                &signer,
            )?;
            term::success!(
                "Identity proposal '{}' created 🌱",
                term::format::highlight(proposal.id)
            );
            print(&proposal, &previous, None)?;
        }
        Operation::Update {
            id,
            rev,
            title,
            description,
            delegates,
            threshold,
        } => {
            let id = id.resolve(&repo.backend)?;
            let mut proposal = proposals.get_mut(&id)?;
            let (_, revision) = select(&proposal, rev, &previous, interactive)?;

            let proposed = {
                let mut proposed = revision.proposed.clone();
                proposed.threshold = threshold.unwrap_or(revision.proposed.threshold);
                proposed.delegates.extend(delegates);
                proposed
            };

            let meta = Metadata {
                title: title.unwrap_or(proposal.title().to_string()),
                description: description.unwrap_or(
                    proposal
                        .description()
                        .unwrap_or("Enter a description")
                        .to_string(),
                ),
                proposed,
            };

            let update = if interactive.yes() {
                meta.edit()?
            } else {
                meta
            };
            warn_out_of_date(revision, &previous);
            let yes = confirm(interactive, "Are you sure you want to update?");
            if yes {
                proposal.edit(update.title, update.description, &signer)?;
                let revision = proposal.update(previous.current, update.proposed, &signer)?;
                term::success!(
                    "Identity proposal '{}' updated 🌱",
                    term::format::highlight(proposal.id)
                );
                term::success!(
                    "Revision '{}'",
                    term::format::highlight(revision.to_string())
                );
                print(&proposal, &previous, None)?;
            }
        }
        Operation::Rebase { id, rev } => {
            let id = id.resolve(&repo.backend)?;
            // TODO: it would be nice if rebasing also handled fast-forwards nicely.
            let mut proposal = proposals.get_mut(&id)?;
            let (_, revision) = select(&proposal, rev, &previous, interactive)?;
            let yes = confirm(interactive, "Are you sure you want to rebase?");
            if yes {
                let revision =
                    proposal.update(previous.current, revision.proposed.clone(), &signer)?;
                term::success!(
                    "Identity proposal '{}' rebased 🌱",
                    term::format::highlight(proposal.id)
                );
                term::success!(
                    "Revision '{}'",
                    term::format::highlight(revision.to_string())
                );
                print(&proposal, &previous, None)?;
            }
        }
        Operation::List => {
            let mut t = term::Table::new(term::table::TableOptions::default());
            // Sort the list by the latest timestamped revisions (i.e. latest edits)
            let mut timestamped = Vec::new();
            let mut no_latest = Vec::new();
            for result in proposals.all()? {
                let (id, proposal, _) = result?;
                match proposal.latest() {
                    None => no_latest.push((id, proposal)),
                    Some((_, revision)) => {
                        timestamped.push(((revision.timestamp, id), id, proposal));
                    }
                }
            }
            timestamped
                .sort_by(|((t1, id1), _, _), ((t2, id2), _, _)| t1.cmp(t2).then(id1.cmp(id2)));
            for (id, proposal) in timestamped
                .into_iter()
                .map(|(_, id, p)| (id, p))
                .chain(no_latest.into_iter())
            {
                let state = match proposal.state() {
                    identity::State::Open => term::format::badge_primary("open"),
                    identity::State::Closed => term::format::badge_negative("closed"),
                    identity::State::Committed => term::format::badge_positive("committed"),
                };
                t.push([
                    term::format::yellow(id.to_string()),
                    term::format::italic(format!("{:?}", proposal.title())),
                    state,
                ]);
            }
            t.print();
        }
        Operation::Commit { id, rev } => {
            let id = id.resolve(&repo.backend)?;
            let mut proposal = proposals.get_mut(&id)?;
            let (rid, revision) = commit_select(&proposal, rev, &previous, interactive)?;
            warn_out_of_date(revision, &previous);
            let yes = confirm(interactive, "Are you sure you want to commit?");
            if yes {
                let id = Proposal::commit(&proposal, &rid, signer.public_key(), &repo, &signer)?;
                proposal.commit(&signer)?;
                term::success!("Committed new identity '{}' 🌱", id.current);
                print(&proposal, &previous, None)?;
            }
        }
        Operation::Close { id } => {
            let id = id.resolve(&repo.backend)?;
            let mut proposal = proposals.get_mut(&id)?;
            let yes = confirm(interactive, "Are you sure you want to close?");
            if yes {
                proposal.close(&signer)?;
                term::success!("Closed identity proposal '{}'", id);
                print(&proposal, &previous, None)?;
            }
        }
        Operation::Show {
            id,
            rev,
            show_revisions,
        } => {
            let id = id.resolve(&repo.backend)?;
            let proposal = proposals
                .get(&id)?
                .context("No proposal with the given ID exists")?;

            print(&proposal, &previous, rev.as_ref())?;
            if show_revisions {
                term::header("Revisions");
                for rid in proposal.revisions().map(|(id, _)| id) {
                    println!("{rid}");
                }
            }
        }
    }
    Ok(())
}

fn warn_out_of_date(revision: &Revision, previous: &Identity<Oid>) {
    if revision.current != previous.current {
        term::warning("Revision is out of date");
        term::warning(&format!("{} =/= {}", revision.current, previous.current));
        term::tip!("Consider using 'rad id rebase' to update the proposal to the latest identity");
    }
}

fn confirm(interactive: &Interactive, msg: &str) -> bool {
    if interactive.yes() {
        term::confirm(msg)
    } else {
        true
    }
}

fn select<'a>(
    proposal: &'a Proposal,
    id: Option<RevisionId>,
    previous: &Identity<Oid>,
    interactive: &Interactive,
) -> anyhow::Result<(RevisionId, &'a identity::Revision)> {
    let (id, revision) = match (id, interactive) {
        (None, Interactive::Yes) => {
            let (id, revision) = term::proposal::revision_select(proposal).unwrap();
            (*id, revision)
        }
        (None, Interactive::No) => {
            let (id, revision) = proposal
                .revisions()
                .next()
                .ok_or(anyhow!("No revisions found!"))?;
            (*id, revision)
        }
        (Some(id), _) => {
            let revision = proposal
                .revision(&id)
                .context(format!("No revision found for {id}"))?
                .get()
                .context(format!("Revision {id} was redacted"))?;
            (id, revision)
        }
    };
    if interactive.yes() {
        print_revision(revision, previous)?;
    }
    Ok((id, revision))
}

fn commit_select<'a>(
    proposal: &'a Proposal,
    id: Option<RevisionId>,
    previous: &'a Identity<Oid>,
    interactive: &Interactive,
) -> anyhow::Result<(RevisionId, &'a identity::Revision)> {
    let (id, revision) = match (id, interactive) {
        (None, Interactive::Yes) => {
            let (id, revision) =
                term::proposal::revision_commit_select(proposal, previous).unwrap();
            (*id, revision)
        }
        (None, Interactive::No) => {
            let (id, revision) = proposal
                .revisions()
                .find(|(_, r)| r.is_quorum_reached(previous))
                .ok_or(anyhow!("No revisions with quorum found"))?;
            (*id, revision)
        }
        (Some(id), _) => {
            let revision = proposal
                .revision(&id)
                .context(format!("No revision found for {id}"))?
                .get()
                .context(format!("Revision {id} was redacted"))?;
            (id, revision)
        }
    };
    if interactive.yes() {
        print_revision(revision, previous)?;
    }
    Ok((id, revision))
}

fn print_meta(title: &str, description: Option<&str>, state: &identity::State) {
    term::info!("{}: {}", term::format::bold("title"), title);
    term::info!(
        "{}: {}",
        term::format::bold("description"),
        description.unwrap_or("No description provided")
    );
    term::info!(
        "{}: {}",
        term::format::bold("status"),
        match state {
            identity::State::Open => term::format::badge_primary("open"),
            identity::State::Closed => term::format::badge_negative("closed"),
            identity::State::Committed => term::format::badge_positive("committed"),
        }
    );
}

fn print_revision(revision: &identity::Revision, previous: &Identity<Oid>) -> anyhow::Result<()> {
    term::info!("{}: {}", term::format::bold("author"), revision.author.id());

    term::header("Document Diff");
    print!("{}", term::proposal::diff(revision, previous)?);
    term::blank();

    {
        term::header("Accepted");
        let accepted = revision.accepted();
        let total = accepted.len();
        print!(
            "{}",
            term::format::positive(format!(
                "{}: {}\n{}: {}",
                "total",
                total,
                "keys",
                serde_json::to_string_pretty(&accepted)?
            ))
        );
        term::blank();
    }

    {
        term::header("Rejected");
        let rejected = revision.rejected();
        let total = rejected.len();
        print!(
            "{}",
            term::format::negative(format!(
                "{}: {}\n{}: {}",
                "total",
                total,
                "keys",
                serde_json::to_string_pretty(&rejected)?
            ))
        );
        term::blank();
    }

    term::header("Quorum Reached");
    print!(
        "{}",
        if revision.is_quorum_reached(previous) {
            term::format::positive("👍 yes")
        } else {
            term::format::negative("👎 no")
        }
    );
    term::blank();

    Ok(())
}

fn print(
    proposal: &identity::Proposal,
    previous: &Identity<Oid>,
    rid: Option<&RevisionId>,
) -> anyhow::Result<()> {
    let revision = match rid {
        None => {
            proposal
                .latest()
                .context("No latest proposal revision to show")?
                .1
        }
        Some(rid) => proposal
            .revision(rid)
            .context(format!("No revision found for {rid}"))?
            .get()
            .context(format!("Revision {rid} was redacted"))?,
    };
    print_meta(proposal.title(), proposal.description(), proposal.state());
    print_revision(revision, previous)
}

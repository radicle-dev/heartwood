#[path = "issue/cache.rs"]
mod cache;

use std::str::FromStr;

use anyhow::Context as _;
use clap::{ArgGroup, Parser, Subcommand, ValueHint};

use radicle::cob::common::{Label, Reaction};
use radicle::cob::issue;
use radicle::cob::issue::{CloseReason, State};
use radicle::cob::thread;
use radicle::crypto::Signer;
use radicle::identity::did::DidError;
use radicle::identity::RepoId;
use radicle::issue::cache::Issues as _;
use radicle::issue::Issues;
use radicle::prelude::Did;
use radicle::profile;
use radicle::storage::{self, ReadStorage as _};
use radicle::storage::{ReadRepository, WriteRepository, WriteStorage};
use radicle::Profile;
use radicle::{cob, Node};

use crate::git::Rev;
use crate::node;
use crate::terminal as term;
use crate::terminal::args::Help;
use crate::terminal::format::Author;
use crate::terminal::issue::Format;
use crate::terminal::patch::Message;
use crate::terminal::Element;

pub const HELP: Help = Help {
    name: "issue",
    description: "Manage issues",
    version: env!("RADICLE_VERSION"),
    usage: r#"
Usage

    rad issue [<option>...]
    rad issue delete <issue-id> [<option>...]
    rad issue edit <issue-id> [<option>...]
    rad issue list [--assigned <did>] [--all | --closed | --open | --solved] [<option>...]
    rad issue open [--title <title>] [--description <text>] [--label <label>] [<option>...]
    rad issue react <issue-id> [--emoji <char>] [--to <comment>] [<option>...]
    rad issue assign <issue-id> [--add <did>] [--delete <did>] [<option>...]
    rad issue label <issue-id> [--add <label>] [--delete <label>] [<option>...]
    rad issue comment <issue-id> [--message <message>] [--reply-to <comment-id>] [<option>...]
    rad issue show <issue-id> [<option>...]
    rad issue state <issue-id> [--closed | --open | --solved] [<option>...]
    rad issue cache [<issue-id>] [<option>...]

Assign options

    -a, --add    <did>     Add an assignee to the issue (may be specified multiple times).
    -d, --delete <did>     Delete an assignee from the issue (may be specified multiple times).

    Note: --add takes precedence over --delete

Label options

    -a, --add    <label>   Add a label to the issue (may be specified multiple times).
    -d, --delete <label>   Delete a label from the issue (may be specified multiple times).

    Note: --add takes precedence over --delete

Show options

        --debug                Show the issue as Rust debug output

Options

        --repo <rid>       Operate on the given repository (default: cwd)
        --no-announce      Don't announce issue to peers
        --header           Show only the issue header, hiding the comments
    -q, --quiet            Don't print anything
        --help             Print help
"#,
};

/// Command line Peer argument.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub enum Assigned {
    #[default]
    Me,
    Peer(Did),
}

#[derive(Parser, Debug)]
pub struct IssueArgs {
    #[command(subcommand)]
    command: Option<IssueCommands>,

    /// Don't print anything
    #[arg(short, long)]
    #[clap(global = true)]
    quiet: bool,

    /// Don't announce issue to peers
    #[arg(long)]
    #[arg(value_name = "no-announce")]
    #[clap(global = true)]
    no_announce: bool,

    /// Show only the issue header, hiding the comments
    #[arg(long)]
    #[clap(global = true)]
    header: bool,

    #[arg(long, short)]
    repo: Option<RepoId>,
}

#[derive(Subcommand, Debug)]
enum IssueCommands {
    /// Delete an issue
    Delete {
        #[arg(value_name = "issue-id")]
        #[clap(value_hint = ValueHint::Dynamic(get_issue_id_hints))]
        id: Rev,
    },

    /// Edit an issue
    Edit {
        #[arg(value_name = "issue-id")]
        #[clap(value_hint = ValueHint::Dynamic(get_issue_id_hints))]
        id: Rev,

        #[arg(long, short)]
        title: Option<String>,

        #[arg(long, short)]
        description: Option<String>,
    },

    /// List and filter issues
    List(ListArgs),

    /// Create a new issue
    Open {
        #[arg(long, short)]
        title: Option<String>,

        #[arg(long, short)]
        description: Option<String>,

        #[arg(long)]
        labels: Vec<Label>,

        #[arg(long)]
        assignees: Vec<Did>,
    },

    /// Add a reaction emoji to an issue or comment
    React {
        #[arg(value_name = "issue-id")]
        #[clap(value_hint = ValueHint::Dynamic(get_issue_id_hints))]
        id: Rev,

        #[arg(long = "emoji")]
        #[arg(value_name = "char")]
        reaction: Reaction,

        #[arg(long = "to")]
        #[arg(value_name = "comment")]
        // TODO: Add dynamic hint for comment ids
        comment_id: Option<thread::CommentId>,
    },

    /// Manage assignees of an issue
    Assign {
        #[clap(value_hint = ValueHint::Dynamic(get_issue_id_hints))]
        #[arg(value_name = "issue-id")]
        id: Rev,

        /// Add an assignee to the issue (may be specified multiple times).
        #[arg(long, short)]
        #[arg(value_name = "did")]
        #[arg(action = clap::ArgAction::Append)]
        add: Vec<Did>,

        /// Delete an assignee from the issue (may be specified multiple times).
        #[arg(long, short)]
        #[arg(value_name = "did")]
        #[arg(action = clap::ArgAction::Append)]
        delete: Vec<Did>,
    },

    /// Update labels on an issue
    Label {
        /// The issue to label.
        #[arg(value_name = "issue-id")]
        #[clap(value_hint = ValueHint::Dynamic(get_issue_id_hints))]
        id: Rev,

        /// Add an assignee to the issue (may be specified multiple times).
        #[arg(long, short)]
        #[arg(value_name = "label")]
        #[arg(action = clap::ArgAction::Append)]
        add: Vec<Label>,

        /// Delete an assignee from the issue (may be specified multiple times).
        #[arg(long, short)]
        #[arg(value_name = "label")]
        #[arg(action = clap::ArgAction::Append)]
        delete: Vec<Label>,
    },

    /// Add a comment to an issue.
    Comment {
        #[arg(value_name = "issue-id")]
        #[clap(value_hint = ValueHint::Dynamic(get_issue_id_hints))]
        id: Rev,

        /// Message text.
        #[arg(long, short)]
        #[arg(value_name = "message")]
        message: Message,

        #[arg(long, name = "comment-id")]
        reply_to: Option<Rev>,
    },

    /// Show a specific issue
    Show {
        #[arg(value_name = "issue-id")]
        #[clap(value_hint = ValueHint::Dynamic(get_issue_id_hints))]
        id: Rev,

        /// Show the issue as Rust debug output
        #[arg(long)]
        debug: bool,
    },

    Cache {
        #[arg(value_name = "issue-id")]
        id: Option<Rev>,
    },

    State(StateArgs),
}

#[derive(Parser, Debug)]
struct ListArgs {
    /// List issues assigned to <did> (default: me)
    #[clap(value_hint = ValueHint::Dynamic(get_assignee_did_hints))]
    #[arg(long, name = "did")]
    #[arg(default_missing_value = "me")]
    #[arg(num_args = 0..=1)]
    #[arg(require_equals = true)]
    assigned: Option<Assigned>,

    /// List all issues (default)
    #[arg(long, group = "state")]
    all: bool,

    /// List only open issues
    #[arg(long, group = "state")]
    open: bool,

    /// List only closed issues
    #[arg(long, group = "state")]
    closed: bool,

    /// List only solved issues
    #[arg(long, group = "state")]
    solved: bool,
}

#[derive(Parser, Debug)]
#[clap(group(ArgGroup::new("state").required(true)))]
struct StateArgs {
    #[arg(value_name = "issue-id")]
    #[clap(value_hint = ValueHint::Dynamic(get_issue_id_hints))]
    id: Rev,

    /// Set issue state to open
    #[arg(long, short, group = "state")]
    open: bool,

    /// Set issue state to closed
    #[arg(long, short, group = "state")]
    closed: bool,

    /// Set issue state to solved
    #[arg(long, short, group = "state")]
    solved: bool,
}

impl FromStr for Assigned {
    type Err = DidError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s == "me" {
            Ok(Assigned::Me)
        } else {
            let value = s.parse::<Did>()?;
            Ok(Assigned::Peer(value))
        }
    }
}

fn to_state_filter(args: ListArgs) -> Option<State> {
    if args.open {
        Some(radicle::cob::issue::State::Open)
    } else if args.closed {
        Some(State::Closed {
            reason: CloseReason::Other,
        })
    } else if args.solved {
        Some(State::Closed {
            reason: CloseReason::Solved,
        })
    } else {
        None
    }
}

fn to_state(args: StateArgs) -> State {
    if args.open {
        radicle::cob::issue::State::Open
    } else if args.closed {
        State::Closed {
            reason: CloseReason::Other,
        }
    } else if args.solved {
        State::Closed {
            reason: CloseReason::Solved,
        }
    } else {
        // FIXME:
        unreachable!("State flag needed");
    }
}

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
                                did.starts_with(input).then_some(did)
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

pub fn run(args: IssueArgs, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    let rid = if let Some(rid) = args.repo {
        rid
    } else {
        radicle::rad::cwd().map(|(_, rid)| rid)?
    };
    let repo = profile.storage.repository_mut(rid)?;

    let mut issues = profile.issues_mut(&repo)?;

    if let Some(command) = args.command {
        let announce = !args.no_announce
            && matches!(
                &command,
                IssueCommands::Open { .. }
                    | IssueCommands::React { .. }
                    | IssueCommands::State { .. }
                    | IssueCommands::Delete { .. }
                    | IssueCommands::Assign { .. }
                    | IssueCommands::Label { .. }
            );

        match command {
            IssueCommands::Delete { id } => {
                let id = id.resolve(&repo.backend)?;
                let signer = term::signer(&profile)?;
                issues.remove(&id, &signer)?;
            }
            IssueCommands::Edit {
                id,
                title,
                description,
            } => {
                let signer = term::signer(&profile)?;
                let issue = edit(&mut issues, &repo, id, title, description, &signer)?;
                if !args.quiet {
                    term::issue::show(&issue, issue.id(), Format::Header, &profile)?;
                }
            }
            IssueCommands::List(list_args) => {
                let assigned = list_args.assigned.clone();
                let state = to_state_filter(list_args);
                list(issues, &assigned, &state, &profile)?;
            }
            IssueCommands::Show { id, debug } => {
                let format = if args.header {
                    term::issue::Format::Header
                } else {
                    term::issue::Format::Full
                };

                let id = id.resolve(&repo.backend)?;

                let issue = issues
                    .get(&id)?
                    .context("No issue with the given ID exists")?;
                if debug {
                    println!("{:#?}", issue);
                } else {
                    term::issue::show(&issue, &id, format, &profile)?;
                }
            }
            IssueCommands::State(state_args) => {
                let id = state_args.id.resolve(&repo.backend)?;
                let signer = term::signer(&profile)?;
                let mut issue = issues.get_mut(&id)?;
                issue.lifecycle(to_state(state_args), &signer)?;
            }
            IssueCommands::Assign { id, add, delete } => {
                let id = id.resolve(&repo.backend)?;
                let Ok(mut issue) = issues.get_mut(&id) else {
                    anyhow::bail!("Issue `{id}` not found");
                };
                let assignees = issue
                    .assignees()
                    .filter(|did| !delete.contains(did))
                    .chain(add.iter())
                    .cloned()
                    .collect::<Vec<_>>();
                let signer = term::signer(&profile)?;
                issue.assign(assignees, &signer)?;
            }
            IssueCommands::Comment {
                id,
                message,
                reply_to,
            } => {
                let issue_id = id.resolve::<cob::ObjectId>(&repo.backend)?;
                let mut issue = issues.get_mut(&issue_id)?;
                let (body, reply_to) = prompt_comment(message, reply_to, &issue, &repo)?;
                let signer = term::signer(&profile)?;
                let comment_id = issue.comment(body, reply_to, vec![], &signer)?;

                if args.quiet {
                    term::print(comment_id);
                } else {
                    let comment = issue.thread().comment(&comment_id).unwrap();
                    term::comment::widget(&comment_id, comment, &profile).print();
                }
            }
            IssueCommands::React {
                id,
                comment_id,
                reaction,
            } => {
                let id = id.resolve(&repo.backend)?;
                if let Ok(mut issue) = issues.get_mut(&id) {
                    let comment_id = comment_id.unwrap_or_else(|| {
                        let (comment_id, _) = term::io::comment_select(&issue).unwrap();
                        *comment_id
                    });
                    let signer = term::signer(&profile)?;
                    issue.react(comment_id, reaction, true, &signer)?;
                }
            }
            IssueCommands::Label { id, add, delete } => {
                let id = id.resolve(&repo.backend)?;
                let Ok(mut issue) = issues.get_mut(&id) else {
                    anyhow::bail!("Issue `{id}` not found");
                };
                let labels = issue
                    .labels()
                    .filter(|did| !delete.contains(did))
                    .chain(add.iter())
                    .cloned()
                    .collect::<Vec<_>>();
                let signer = term::signer(&profile)?;
                issue.label(labels, &signer)?;
            }
            IssueCommands::Open {
                ref title,
                ref description,
                ref labels,
                ref assignees,
            } => {
                let signer = term::signer(&profile)?;
                open(
                    title.clone(),
                    description.clone(),
                    labels.to_vec(),
                    assignees.to_vec(),
                    args.quiet,
                    &mut issues,
                    &signer,
                    &profile,
                )?;
            }
            IssueCommands::Cache { id } => {
                let id = id.map(|id| id.resolve(&repo.backend)).transpose()?;
                cache::run(id, &repo, &profile)?;
            }
        }

        if announce {
            let mut node = Node::new(profile.socket());
            node::announce(
                &repo,
                node::SyncSettings::default(),
                node::SyncReporting::default(),
                &mut node,
                &profile,
            )?;
        }
    } else {
        // Default `issue` subcommand is `list`.
        list(issues, &None, &None, &profile)?;
    };

    Ok(())
}

fn list<C>(
    cache: C,
    assigned: &Option<Assigned>,
    state: &Option<State>,
    profile: &profile::Profile,
) -> anyhow::Result<()>
where
    C: issue::cache::Issues,
{
    if cache.is_empty()? {
        term::print(term::format::italic("Nothing to show."));
        return Ok(());
    }

    let assignee = match assigned {
        Some(Assigned::Me) => Some(*profile.id()),
        Some(Assigned::Peer(id)) => Some((*id).into()),
        None => None,
    };

    let mut all = Vec::new();
    let issues = cache.list()?;
    for result in issues {
        let Ok((id, issue)) = result else {
            // Skip issues that failed to load.
            continue;
        };

        if let Some(a) = assignee {
            if !issue.assignees().any(|v| v == &Did::from(a)) {
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
        term::Line::blank(),
        term::format::bold(String::from("Labels")).into(),
        term::format::bold(String::from("Assignees")).into(),
        term::format::bold(String::from("Opened")).into(),
    ]);
    table.divider();

    for (id, issue) in all {
        let assigned: String = issue
            .assignees()
            .map(|did| {
                let (alias, _) = Author::new(did.as_key(), profile).labels();

                alias.content().to_owned()
            })
            .collect::<Vec<_>>()
            .join(", ");

        let mut labels = issue.labels().map(|t| t.to_string()).collect::<Vec<_>>();
        labels.sort();

        let author = issue.author().id;
        let (alias, did) = Author::new(&author, profile).labels();

        table.push([
            match issue.state() {
                State::Open => term::format::positive("●").into(),
                State::Closed { .. } => term::format::negative("●").into(),
            },
            term::format::tertiary(term::format::cob(&id))
                .to_owned()
                .into(),
            term::format::default(issue.title().to_owned()).into(),
            alias.into(),
            did.into(),
            term::format::secondary(labels.join(", ")).into(),
            if assigned.is_empty() {
                term::format::dim(String::default()).into()
            } else {
                term::format::primary(assigned.to_string()).dim().into()
            },
            term::format::timestamp(issue.timestamp())
                .dim()
                .italic()
                .into(),
        ]);
    }
    table.print();

    Ok(())
}

fn open<R, G>(
    title: Option<String>,
    description: Option<String>,
    labels: Vec<Label>,
    assignees: Vec<Did>,
    quiet: bool,
    cache: &mut issue::Cache<issue::Issues<'_, R>, cob::cache::StoreWriter>,
    signer: &G,
    profile: &Profile,
) -> anyhow::Result<()>
where
    R: ReadRepository + WriteRepository + cob::Store,
    G: Signer,
{
    let (title, description) = if let (Some(t), Some(d)) = (title.as_ref(), description.as_ref()) {
        (t.to_owned(), d.to_owned())
    } else if let Some((t, d)) = term::issue::get_title_description(title, description)? {
        (t, d)
    } else {
        anyhow::bail!("aborting issue creation due to empty title or description");
    };
    let issue = cache.create(
        &title,
        description,
        labels.as_slice(),
        assignees.as_slice(),
        [],
        signer,
    )?;

    if !quiet {
        term::issue::show(&issue, issue.id(), Format::Header, profile)?;
    }
    Ok(())
}

fn edit<'a, 'g, R, G>(
    issues: &'g mut issue::Cache<issue::Issues<'a, R>, cob::cache::StoreWriter>,
    repo: &storage::git::Repository,
    id: Rev,
    title: Option<String>,
    description: Option<String>,
    signer: &G,
) -> anyhow::Result<issue::IssueMut<'a, 'g, R, cob::cache::StoreWriter>>
where
    R: WriteRepository + ReadRepository + cob::Store,
    G: radicle::crypto::Signer,
{
    let id = id.resolve(&repo.backend)?;
    let mut issue = issues.get_mut(&id)?;
    let (root, _) = issue.root();
    let root = *root;

    if title.is_some() || description.is_some() {
        // Editing by command line arguments.
        issue.transaction("Edit", signer, |tx| {
            if let Some(t) = title {
                tx.edit(t)?;
            }
            if let Some(d) = description {
                tx.edit_comment(root, d, vec![])?;
            }
            Ok(())
        })?;
        return Ok(issue);
    }

    // Editing via the editor.
    let Some((title, description)) = term::issue::get_title_description(
        Some(title.unwrap_or(issue.title().to_owned())),
        Some(description.unwrap_or(issue.description().to_owned())),
    )?
    else {
        return Ok(issue);
    };

    issue.transaction("Edit", signer, |tx| {
        tx.edit(title)?;
        tx.edit_comment(root, description, vec![])?;

        Ok(())
    })?;

    Ok(issue)
}

/// Get a comment from the user, by prompting.
pub fn prompt_comment<R: WriteRepository + radicle::cob::Store>(
    message: Message,
    reply_to: Option<Rev>,
    issue: &issue::Issue,
    repo: &R,
) -> anyhow::Result<(String, thread::CommentId)> {
    let (root, r) = issue.root();
    let (reply_to, help) = if let Some(rev) = reply_to {
        let id = rev.resolve::<radicle::git::Oid>(repo.raw())?;
        let parent = issue
            .thread()
            .comment(&id)
            .ok_or(anyhow::anyhow!("comment '{rev}' not found"))?;

        (id, parent.body().trim())
    } else {
        (*root, r.body().trim())
    };
    let help = format!("\n{}\n", term::format::html::commented(help));
    let body = message.get(&help)?;

    if body.is_empty() {
        anyhow::bail!("aborting operation due to empty comment");
    }
    Ok((body, reply_to))
}

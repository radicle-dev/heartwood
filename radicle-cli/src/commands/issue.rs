use std::collections::BTreeSet;
use std::ffi::OsString;
use std::str::FromStr;

use anyhow::{anyhow, Context as _};

use radicle::cob::common::{Label, Reaction};
use radicle::cob::issue;
use radicle::cob::issue::{CloseReason, Issues};
use radicle::cob::thread;
use radicle::cob::ObjectId;
use radicle::crypto::Signer;
use radicle::prelude::Did;
use radicle::profile;
use radicle::storage;
use radicle::storage::git::Repository;
use radicle::storage::{WriteRepository, WriteStorage};
use radicle::Profile;
use radicle::{cob, Node};

use crate::git::Rev;
use crate::node;
use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};
use crate::terminal::command::CommandError;
use crate::terminal::format::Author;
use crate::terminal::issue::Format;
use crate::terminal::patch::Message;
use crate::terminal::Element;

use crate::tui::{self, TuiError};

pub const HELP: Help = Help {
    name: "issue",
    description: "Manage issues",
    version: env!("CARGO_PKG_VERSION"),
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

        --no-announce      Don't announce issue to peers
        --header           Show only the issue header, hiding the comments
    -q, --quiet            Don't print anything
        --help             Print help
"#,
};

#[derive(Default, Debug, PartialEq, Eq)]
pub enum OperationName {
    Assign,
    Edit,
    Open,
    Comment,
    Delete,
    Label,
    List,
    React,
    Show,
    State,
    #[default]
    None,
}

/// Command line Peer argument.
#[derive(Default, Debug, PartialEq, Eq)]
pub enum Assigned {
    #[default]
    Me,
    Peer(Did),
}

impl From<Assigned> for tui::issue::Assigned {
    fn from(assigned: Assigned) -> Self {
        match assigned {
            Assigned::Me => tui::issue::Assigned::Me,
            Assigned::Peer(did) => tui::issue::Assigned::Peer(did),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Operation {
    Edit {
        id: Option<Rev>,
        title: Option<String>,
        description: Option<String>,
    },
    Open {
        title: Option<String>,
        description: Option<String>,
        labels: Vec<Label>,
        assignees: Vec<Did>,
    },
    Show {
        id: Option<Rev>,
        format: Format,
        debug: bool,
    },
    Comment {
        id: Option<Rev>,
        message: Message,
        reply_to: Option<Rev>,
    },
    State {
        id: Option<Rev>,
        state: issue::State,
    },
    Delete {
        id: Option<Rev>,
    },
    React {
        id: Option<Rev>,
        reaction: Reaction,
        comment_id: Option<thread::CommentId>,
    },
    Assign {
        id: Option<Rev>,
        opts: AssignOptions,
    },
    Label {
        id: Option<Rev>,
        opts: LabelOptions,
    },
    // List {
    //     assigned: Option<Assigned>,
    //     state: Option<issue::State>,
    // },
    List {
        opts: ListOptions,
    },
    None {
        opts: ListOptions,
    },
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ListOptions {
    pub state: Option<issue::State>,
    pub assigned: Option<Assigned>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct StateOptions {
    pub state: Option<issue::State>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct AssignOptions {
    pub add: BTreeSet<Did>,
    pub delete: BTreeSet<Did>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct LabelOptions {
    pub add: BTreeSet<Label>,
    pub delete: BTreeSet<Label>,
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
        let mut title: Option<String> = None;
        let mut reaction: Option<Reaction> = None;
        let mut comment_id: Option<thread::CommentId> = None;
        let mut description: Option<String> = None;
        let mut labels = Vec::new();
        let mut assignees = Vec::new();
        let mut format = Format::default();
        let mut message = Message::default();
        let mut reply_to = None;
        let mut announce = true;
        let mut quiet = false;
        let mut debug = false;
        let mut assign_opts = AssignOptions::default();
        let mut label_opts = LabelOptions::default();

        let mut list_opts = ListOptions::default();
        let mut state_opts = StateOptions::default();

        while let Some(arg) = parser.next()? {
            match arg {
                Long("help") | Short('h') => {
                    return Err(Error::Help.into());
                }

                // List options.
                Long("all") if op.is_none() || op == Some(OperationName::List) => {
                    list_opts.state = None;
                }
                Long("closed") if op.is_none() || op == Some(OperationName::List) => {
                    list_opts.state = Some(issue::State::Closed {
                        reason: CloseReason::Other,
                    });
                }
                Long("open") if op.is_none() || op == Some(OperationName::List) => {
                    list_opts.state = Some(issue::State::Open);
                }
                Long("solved") if op.is_none() || op == Some(OperationName::List) => {
                    list_opts.state = Some(issue::State::Closed {
                        reason: CloseReason::Solved,
                    });
                }
                Long("assigned") | Short('a') if list_opts.assigned.is_none() => {
                    if let Ok(val) = parser.value() {
                        let peer = term::args::did(&val)?;
                        list_opts.assigned = Some(Assigned::Peer(peer));
                    } else {
                        list_opts.assigned = Some(Assigned::Me);
                    }
                }

                // Open options.
                Long("title") if op == Some(OperationName::Open) => {
                    title = Some(parser.value()?.to_string_lossy().into());
                }
                Short('l') | Long("label") if matches!(op, Some(OperationName::Open)) => {
                    let val = parser.value()?;
                    let name = term::args::string(&val);
                    let label = Label::new(name)?;

                    labels.push(label);
                }
                Long("assign") if op == Some(OperationName::Open) => {
                    let val = parser.value()?;
                    let did = term::args::did(&val)?;

                    assignees.push(did);
                }
                Long("description") if op == Some(OperationName::Open) => {
                    description = Some(parser.value()?.to_string_lossy().into());
                }

                // State options.
                Long("closed") if op == Some(OperationName::State) => {
                    state_opts.state = Some(issue::State::Closed {
                        reason: CloseReason::Other,
                    });
                }
                Long("open") if op == Some(OperationName::State) => {
                    state_opts.state = Some(issue::State::Open);
                }
                Long("solved") if op == Some(OperationName::State) => {
                    state_opts.state = Some(issue::State::Closed {
                        reason: CloseReason::Solved,
                    });
                }

                // React options.
                Long("emoji") if op == Some(OperationName::React) => {
                    if let Some(emoji) = parser.value()?.to_str() {
                        reaction =
                            Some(Reaction::from_str(emoji).map_err(|_| anyhow!("invalid emoji"))?);
                    }
                }
                Long("to") if op == Some(OperationName::React) => {
                    let oid: String = parser.value()?.to_string_lossy().into();
                    comment_id = Some(oid.parse()?);
                }

                // Show options.
                Long("format") if op == Some(OperationName::Show) => {
                    let val = parser.value()?;
                    let val = term::args::string(&val);

                    match val.as_str() {
                        "header" => format = Format::Header,
                        "full" => format = Format::Full,
                        _ => anyhow::bail!("unknown format '{val}'"),
                    }
                }
                Long("debug") if op == Some(OperationName::Show) => {
                    debug = true;
                }

                // Comment options.
                Long("message") | Short('m') if op == Some(OperationName::Comment) => {
                    let val = parser.value()?;
                    let txt = term::args::string(&val);

                    message.append(&txt);
                }
                Long("reply-to") if op == Some(OperationName::Comment) => {
                    let val = parser.value()?;
                    let rev = term::args::rev(&val)?;

                    reply_to = Some(rev);
                }

                // Assign options
                Short('a') | Long("add") if op == Some(OperationName::Assign) => {
                    assign_opts.add.insert(term::args::did(&parser.value()?)?);
                }
                Short('d') | Long("delete") if op == Some(OperationName::Assign) => {
                    assign_opts
                        .delete
                        .insert(term::args::did(&parser.value()?)?);
                }

                // Label options
                Short('a') | Long("add") if matches!(op, Some(OperationName::Label)) => {
                    let val = parser.value()?;
                    let name = term::args::string(&val);
                    let label = Label::new(name)?;

                    label_opts.add.insert(label);
                }
                Short('d') | Long("delete") if matches!(op, Some(OperationName::Label)) => {
                    let val = parser.value()?;
                    let name = term::args::string(&val);
                    let label = Label::new(name)?;

                    label_opts.delete.insert(label);
                }

                // Options.
                Long("no-announce") => {
                    announce = false;
                }
                Long("quiet") | Short('q') => {
                    quiet = true;
                }

                Value(val) if op.is_none() => match val.to_string_lossy().as_ref() {
                    "c" | "comment" => op = Some(OperationName::Comment),
                    "w" | "show" => op = Some(OperationName::Show),
                    "d" | "delete" => op = Some(OperationName::Delete),
                    "e" | "edit" => op = Some(OperationName::Edit),
                    "l" | "list" => op = Some(OperationName::List),
                    "o" | "open" => op = Some(OperationName::Open),
                    "r" | "react" => op = Some(OperationName::React),
                    "s" | "state" => op = Some(OperationName::State),
                    "assign" => op = Some(OperationName::Assign),
                    "label" => op = Some(OperationName::Label),

                    unknown => anyhow::bail!("unknown operation '{}'", unknown),
                },
                Value(val) if op.is_some() => {
                    let val = term::args::rev(&val)?;
                    id = Some(val);
                }
                _ => {
                    return Err(anyhow!(arg.unexpected()));
                }
            }
        }

        let op = match op.unwrap_or_default() {
            OperationName::Edit => Operation::Edit {
                id,
                title,
                description,
            },
            OperationName::Open => Operation::Open {
                title,
                description,
                labels,
                assignees,
            },
            OperationName::Comment => Operation::Comment {
                id,
                message,
                reply_to,
            },
            OperationName::Show => Operation::Show { id, format, debug },
            OperationName::State => Operation::State {
                id,
                state: state_opts
                    .state
                    .ok_or_else(|| anyhow!("a state operation must be provided"))?,
            },
            OperationName::React => Operation::React {
                id,
                reaction: reaction.ok_or_else(|| anyhow!("a reaction emoji must be provided"))?,
                comment_id,
            },
            OperationName::Delete => Operation::Delete { id },
            OperationName::Assign => Operation::Assign {
                id,
                opts: assign_opts,
            },
            OperationName::Label => Operation::Label {
                id,
                opts: label_opts,
            },
            OperationName::List => Operation::List { opts: list_opts },
            OperationName::None => Operation::None { opts: list_opts },
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

pub fn run(mut options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
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
                | Operation::Assign { .. }
                | Operation::Label { .. }
        );

    let mut issues = Issues::open(&repo)?;

    match options.op {
        Operation::Edit {
            id,
            title,
            description,
        } => {
            let id = resolve_issue_id(&repo, id)?;
            let issue = edit(
                &mut issues,
                &repo,
                Rev::from(id.to_string()),
                title,
                description,
                &signer,
            )?;
            if !options.quiet {
                term::issue::show(&issue, issue.id(), Format::Header, &profile)?;
            }
        }
        Operation::Open {
            title: Some(title),
            description: Some(description),
            labels,
            assignees,
        } => {
            let issue = issues.create(title, description, &labels, &assignees, [], &signer)?;
            if !options.quiet {
                term::issue::show(&issue, issue.id(), Format::Header, &profile)?;
            }
        }
        Operation::Comment {
            id,
            message,
            reply_to,
        } => {
            let issue_id = resolve_issue_id(&repo, id)?;
            let mut issue = issues.get_mut(&issue_id)?;
            let (body, reply_to) = prompt_comment(message, reply_to, &issue, &repo)?;
            let comment_id = issue.comment(body, reply_to, vec![], &signer)?;

            if options.quiet {
                term::print(comment_id);
            } else {
                let comment = issue.thread().comment(&comment_id).unwrap();
                term::comment::widget(&comment_id, comment, &profile).print();
            }
        }
        Operation::Show { id, format, debug } => {
            let id = resolve_issue_id(&repo, id)?;
            let issue = issues
                .get(&id)?
                .context("No issue with the given ID exists")?;
            if debug {
                println!("{:#?}", issue);
            } else {
                term::issue::show(&issue, &id, format, &profile)?;
            }
        }
        Operation::State { id, state } => {
            let id = resolve_issue_id(&repo, id)?;
            let mut issue = issues.get_mut(&id)?;
            issue.lifecycle(state, &signer)?;
        }
        Operation::React {
            id,
            reaction,
            comment_id,
        } => {
            let id = resolve_issue_id(&repo, id)?;
            if let Ok(mut issue) = issues.get_mut(&id) {
                let comment_id = comment_id.unwrap_or_else(|| {
                    let (comment_id, _) = term::io::comment_select(&issue).unwrap();
                    *comment_id
                });
                issue.react(comment_id, reaction, true, &signer)?;
            }
        }
        Operation::Open {
            ref title,
            ref description,
            ref labels,
            ref assignees,
        } => {
            open(
                title.clone(),
                description.clone(),
                labels.to_vec(),
                assignees.to_vec(),
                &options,
                &mut issues,
                &signer,
                &profile,
            )?;
        }
        Operation::Assign {
            id,
            opts: AssignOptions { add, delete },
        } => {
            let id = resolve_issue_id(&repo, id)?;
            let Ok(mut issue) = issues.get_mut(&id) else {
                anyhow::bail!("Issue `{id}` not found");
            };
            let assignees = issue
                .assignees()
                .filter(|did| !delete.contains(did))
                .chain(add.iter())
                .cloned()
                .collect::<Vec<_>>();
            issue.assign(assignees, &signer)?;
        }
        Operation::Label {
            id,
            opts: LabelOptions { add, delete },
        } => {
            let id = resolve_issue_id(&repo, id)?;
            let Ok(mut issue) = issues.get_mut(&id) else {
                anyhow::bail!("Issue `{id}` not found");
            };
            let labels = issue
                .labels()
                .filter(|did| !delete.contains(did))
                .chain(add.iter())
                .cloned()
                .collect::<Vec<_>>();
            issue.label(labels, &signer)?;
        }
        Operation::List { opts } => {
            list(&issues, &opts.assigned, &opts.state, &profile)?;
        }
        Operation::None { opts } => {
            if let Some(operation) = resolve_issue_operation(opts.state, opts.assigned)? {
                options.op = operation;
                run(options, ctx)?;
            }
        }
        Operation::Delete { id } => {
            let id = resolve_issue_id(&repo, id)?;
            issues.remove(&id, &signer)?;
        }
    }

    if announce {
        let mut node = Node::new(profile.socket());
        node::announce(rid, &mut node)?;
    }

    Ok(())
}

fn list<R: WriteRepository + cob::Store>(
    issues: &Issues<R>,
    assigned: &Option<Assigned>,
    state: &Option<issue::State>,
    profile: &profile::Profile,
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
                issue::State::Open => term::format::positive("●").into(),
                issue::State::Closed { .. } => term::format::negative("●").into(),
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

fn open<R: WriteRepository + cob::Store, G: Signer>(
    title: Option<String>,
    description: Option<String>,
    labels: Vec<Label>,
    assignees: Vec<Did>,
    options: &Options,
    issues: &mut Issues<R>,
    signer: &G,
    profile: &Profile,
) -> anyhow::Result<()> {
    let (title, description) = if let (Some(t), Some(d)) = (title.as_ref(), description.as_ref()) {
        (t.to_owned(), d.to_owned())
    } else if let Some((t, d)) = term::issue::get_title_description(title, description)? {
        (t, d)
    } else {
        anyhow::bail!("aborting issue creation due to empty title or description");
    };
    let issue = issues.create(
        &title,
        description,
        labels.as_slice(),
        assignees.as_slice(),
        [],
        signer,
    )?;

    if !options.quiet {
        term::issue::show(&issue, issue.id(), Format::Header, profile)?;
    }
    Ok(())
}

fn edit<'a, 'g, R: WriteRepository + cob::Store, G: radicle::crypto::Signer>(
    issues: &'a mut issue::Issues<'a, R>,
    repo: &storage::git::Repository,
    id: Rev,
    title: Option<String>,
    description: Option<String>,
    signer: &G,
) -> anyhow::Result<issue::IssueMut<'a, 'g, R>> {
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

fn resolve_issue_id(repository: &Repository, rev: Option<Rev>) -> anyhow::Result<ObjectId> {
    if let Some(rev) = rev {
        Ok(rev.resolve(&repository.backend)?)
    } else {
        match tui::issue::select_id() {
            Ok(output) => {
                let patch_id = output.and_then(|output| output.ids().first().cloned());
                patch_id.ok_or_else(|| anyhow!("a patch must be provided"))
            }
            Err(TuiError::Command(CommandError::NotFound)) => {
                term::hint("An optional patch TUI can be enabled by installing `rad-tui`. You can download it from https://files.radicle.xyz/latest.");
                Err(anyhow!("a patch must be provided"))
            }
            Err(err) => Err(err.into()),
        }
    }
}

fn resolve_issue_operation(
    state: Option<issue::State>,
    assigned: Option<Assigned>,
) -> anyhow::Result<Option<Operation>> {
    let assignee = assigned.map(tui::issue::Assigned::from);

    match tui::issue::select_operation(state, assignee) {
        Ok(Some(output)) => {
            let operation = output
                .operation()
                .ok_or_else(|| anyhow!("an operation must be provided"))?;
            let issue_id = output
                .ids()
                .first()
                .ok_or_else(|| anyhow!("an issue must be provided"))?;

            match operation.as_str() {
                "show" => Ok(Some(Operation::Show {
                    id: Some(Rev::from(issue_id.to_string())),
                    format: Format::Full,
                    debug: false,
                })),
                "comment" => Ok(Some(Operation::Comment {
                    id: Some(Rev::from(issue_id.to_string())),
                    message: Message::Blank,
                    reply_to: None,
                })),
                "edit" => Ok(Some(Operation::Edit {
                    id: Some(Rev::from(issue_id.to_string())),
                    title: None,
                    description: None,
                })),
                "delete" => Ok(Some(Operation::Delete {
                    id: Some(Rev::from(issue_id.to_string())),
                })),
                _ => Ok(None),
            }
        }
        Ok(None) => Ok(None),
        Err(TuiError::Command(CommandError::NotFound)) => {
            term::hint("An optional patch TUI can be enabled by installing `rad-tui`. You can download it from https://files.radicle.xyz/latest.");
            Ok(Some(Operation::List {
                opts: ListOptions::default(),
            }))
        }
        Err(err) => Err(err.into()),
    }
}

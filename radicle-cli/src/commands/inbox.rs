use std::ffi::OsString;
use std::path::Path;
use std::process;

use anyhow::anyhow;

use localtime::LocalTime;
use radicle::identity::Identity;
use radicle::issue::cache::Issues as _;
use radicle::node::notifications;
use radicle::node::notifications::*;
use radicle::patch::cache::Patches as _;
use radicle::prelude::{Profile, RepoId};
use radicle::storage::{ReadRepository, ReadStorage};
use radicle::{cob, Storage};

use term::Element as _;

use crate::terminal as term;
use crate::terminal::args;
use crate::terminal::args::{Args, Error, Help};

pub const HELP: Help = Help {
    name: "inbox",
    description: "Manage your Radicle notifications inbox",
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad inbox [<option>...]
    rad inbox list [<option>...]
    rad inbox show <id> [<option>...]
    rad inbox clear [<option>...]

    By default, this command lists all items in your inbox.
    If your working directory is a Radicle repository, it only shows item
    belonging to this repository, unless `--all` is used.

    The `rad inbox show` command takes a notification ID (which can be found in
    the `list` command) and displays the information related to that
    notification. This will mark the notification as read.

    The `rad inbox clear` command will delete all notifications in the inbox.

Options

    --all                Operate on all repositories
    --repo <rid>         Operate on the given repository (default: rad .)
    --sort-by <field>    Sort by `id` or `timestamp` (default: timestamp)
    --reverse, -r        Reverse the list
    --help               Print help
"#,
};

#[derive(Debug, Default, PartialEq, Eq)]
enum Operation {
    #[default]
    List,
    Show,
    Clear,
}

#[derive(Default, Debug)]
enum Mode {
    #[default]
    Contextual,
    All,
    ById(Vec<NotificationId>),
    ByRepo(RepoId),
}

#[derive(Clone, Copy, Debug)]
struct SortBy {
    reverse: bool,
    field: &'static str,
}

pub struct Options {
    op: Operation,
    mode: Mode,
    sort_by: SortBy,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut op: Option<Operation> = None;
        let mut mode = None;
        let mut ids = Vec::new();
        let mut reverse = None;
        let mut field = None;

        while let Some(arg) = parser.next()? {
            match arg {
                Long("help") | Short('h') => {
                    return Err(Error::Help.into());
                }
                Long("all") | Short('a') if mode.is_none() => {
                    mode = Some(Mode::All);
                }
                Long("reverse") | Short('r') => {
                    reverse = Some(true);
                }
                Long("sort-by") => {
                    let val = parser.value()?;

                    match term::args::string(&val).as_str() {
                        "timestamp" => field = Some("timestamp"),
                        "id" => field = Some("rowid"),
                        other => {
                            return Err(anyhow!(
                                "unknown sorting field `{other}`, see `rad inbox --help`"
                            ))
                        }
                    }
                }
                Long("repo") if mode.is_none() && op.is_some() => {
                    let val = parser.value()?;
                    let repo = args::rid(&val)?;

                    mode = Some(Mode::ByRepo(repo));
                }
                Value(val) if op.is_none() => match val.to_string_lossy().as_ref() {
                    "list" => op = Some(Operation::List),
                    "show" => op = Some(Operation::Show),
                    "clear" => op = Some(Operation::Clear),
                    cmd => return Err(anyhow!("unknown command `{cmd}`, see `rad inbox --help`")),
                },
                Value(val) if op.is_some() && mode.is_none() => {
                    let id = term::args::number(&val)? as NotificationId;
                    ids.push(id);
                }
                _ => return Err(anyhow::anyhow!(arg.unexpected())),
            }
        }
        let mode = if ids.is_empty() {
            mode.unwrap_or_default()
        } else {
            Mode::ById(ids)
        };
        let op = op.unwrap_or_default();

        let sort_by = if let Some(field) = field {
            SortBy {
                field,
                reverse: reverse.unwrap_or(false),
            }
        } else {
            SortBy {
                field: "timestamp",
                reverse: true,
            }
        };

        Ok((Options { op, mode, sort_by }, vec![]))
    }
}

pub fn run(options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    let storage = &profile.storage;
    let mut notifs = profile.notifications_mut()?;
    let Options { op, mode, sort_by } = options;

    match op {
        Operation::List => list(mode, sort_by, &notifs.read_only(), storage, &profile),
        Operation::Clear => clear(mode, &mut notifs),
        Operation::Show => show(mode, &mut notifs, storage, &profile),
    }
}

fn list(
    mode: Mode,
    sort_by: SortBy,
    notifs: &notifications::StoreReader,
    storage: &Storage,
    profile: &Profile,
) -> anyhow::Result<()> {
    let repos: Vec<term::VStack<'_>> = match mode {
        Mode::Contextual => {
            if let Ok((_, rid)) = radicle::rad::cwd() {
                list_repo(rid, sort_by, notifs, storage, profile)?
                    .into_iter()
                    .collect()
            } else {
                list_all(sort_by, notifs, storage, profile)?
            }
        }
        Mode::ByRepo(rid) => list_repo(rid, sort_by, notifs, storage, profile)?
            .into_iter()
            .collect(),
        Mode::All => list_all(sort_by, notifs, storage, profile)?,
        Mode::ById(_) => anyhow::bail!("the `list` command does not take IDs"),
    };

    if repos.is_empty() {
        term::print(term::format::italic("Your inbox is empty."));
    } else {
        for repo in repos {
            repo.print();
        }
    }
    Ok(())
}

fn list_all<'a>(
    sort_by: SortBy,
    notifs: &notifications::StoreReader,
    storage: &Storage,
    profile: &Profile,
) -> anyhow::Result<Vec<term::VStack<'a>>> {
    let mut repos = storage.repositories()?;
    repos.sort_by_key(|r| r.rid);

    let mut vstacks = Vec::new();
    for repo in repos {
        let vstack = list_repo(repo.rid, sort_by, notifs, storage, profile)?;
        vstacks.extend(vstack.into_iter());
    }
    Ok(vstacks)
}

fn list_repo<'a, R: ReadStorage>(
    rid: RepoId,
    sort_by: SortBy,
    notifs: &notifications::StoreReader,
    storage: &R,
    profile: &Profile,
) -> anyhow::Result<Option<term::VStack<'a>>>
where
    <R as ReadStorage>::Repository: cob::Store,
{
    let mut table = term::Table::new(term::TableOptions {
        spacing: 3,
        ..term::TableOptions::default()
    });
    let repo = storage.repository(rid)?;
    let (_, head) = repo.head()?;
    let doc = repo.identity_doc()?;
    let proj = doc.project()?;
    let issues = profile.issues(&repo)?;
    let patches = profile.patches(&repo)?;

    let mut notifs = notifs.by_repo(&rid, sort_by.field)?.collect::<Vec<_>>();
    if !sort_by.reverse {
        // Notifications are returned in descendant order by default.
        notifs.reverse();
    }

    for n in notifs {
        let n: Notification = n?;

        let seen = if n.status.is_read() {
            term::Label::blank()
        } else {
            term::format::tertiary(String::from("●")).into()
        };
        let (category, summary, state, name) = match n.kind {
            NotificationKind::Branch { name } => {
                let commit = if let Some(head) = n.update.new() {
                    repo.commit(head)?.summary().unwrap_or_default().to_owned()
                } else {
                    String::new()
                };

                let state = match n
                    .update
                    .new()
                    .map(|oid| repo.is_ancestor_of(oid, head))
                    .transpose()
                {
                    Ok(Some(true)) => {
                        term::Paint::<String>::from(term::format::secondary("merged"))
                    }
                    Ok(Some(false)) | Ok(None) => term::format::ref_update(n.update).into(),
                    Err(e) => return Err(e.into()),
                }
                .to_owned();

                (
                    "branch".to_string(),
                    commit,
                    state,
                    term::format::default(name.to_string()),
                )
            }
            NotificationKind::Cob { type_name, id } => {
                let (category, summary, state) = if type_name == *cob::issue::TYPENAME {
                    let Some(issue) = issues.get(&id)? else {
                        // Issue could have been deleted after notification was created.
                        continue;
                    };
                    (
                        String::from("issue"),
                        issue.title().to_owned(),
                        term::format::issue::state(issue.state()),
                    )
                } else if type_name == *cob::patch::TYPENAME {
                    let Some(patch) = patches.get(&id)? else {
                        // Patch could have been deleted after notification was created.
                        continue;
                    };
                    (
                        String::from("patch"),
                        patch.title().to_owned(),
                        term::format::patch::state(patch.state()),
                    )
                } else if type_name == *cob::identity::TYPENAME {
                    let Ok(identity) = Identity::get(&id, &repo) else {
                        log::error!(
                            target: "cli",
                            "Error retrieving identity {id} for notification {}", n.id
                        );
                        continue;
                    };
                    let Some(rev) = n.update.new().and_then(|id| identity.revision(&id)) else {
                        log::error!(
                            target: "cli",
                            "Error retrieving identity revision for notification {}", n.id
                        );
                        continue;
                    };
                    (
                        String::from("id"),
                        rev.title.clone(),
                        term::format::identity::state(&rev.state),
                    )
                } else {
                    (
                        type_name.to_string(),
                        "".to_owned(),
                        term::format::default(String::new()),
                    )
                };
                (category, summary, state, term::format::cob(&id))
            }
        };
        let author = n
            .remote
            .map(|r| {
                let (alias, _) = term::format::Author::new(&r, profile).labels();
                alias
            })
            .unwrap_or_default();
        table.push([
            term::format::dim(format!("{:-03}", n.id)).into(),
            seen,
            term::format::tertiary(name).into(),
            summary.into(),
            term::format::dim(category).into(),
            state.into(),
            author,
            term::format::italic(term::format::timestamp(n.timestamp)).into(),
        ]);
    }

    if table.is_empty() {
        Ok(None)
    } else {
        Ok(Some(
            term::VStack::default()
                .border(Some(term::colors::FAINT))
                .child(term::label(term::format::bold(proj.name())))
                .divider()
                .child(table),
        ))
    }
}

fn clear(mode: Mode, notifs: &mut notifications::StoreWriter) -> anyhow::Result<()> {
    let cleared = match mode {
        Mode::All => notifs.clear_all()?,
        Mode::ById(ids) => notifs.clear(&ids)?,
        Mode::ByRepo(rid) => notifs.clear_by_repo(&rid)?,
        Mode::Contextual => {
            if let Ok((_, rid)) = radicle::rad::cwd() {
                notifs.clear_by_repo(&rid)?
            } else {
                return Err(Error::WithHint {
                    err: anyhow!("not a radicle repository"),
                    hint: "to clear all repository notifications, use the `--all` flag",
                }
                .into());
            }
        }
    };
    if cleared > 0 {
        term::success!("Cleared {cleared} item(s) from your inbox");
    } else {
        term::print(term::format::italic("Your inbox is empty."));
    }
    Ok(())
}

fn show(
    mode: Mode,
    notifs: &mut notifications::StoreWriter,
    storage: &Storage,
    profile: &Profile,
) -> anyhow::Result<()> {
    let id = match mode {
        Mode::ById(ids) => match ids.as_slice() {
            [id] => *id,
            [] => anyhow::bail!("a Notification ID must be given"),
            _ => anyhow::bail!("too many Notification IDs given"),
        },
        _ => anyhow::bail!("a Notification ID must be given"),
    };
    let n = notifs.get(id)?;
    let repo = storage.repository(n.repo)?;

    match n.kind {
        NotificationKind::Cob { type_name, id } if type_name == *cob::issue::TYPENAME => {
            let issues = profile.issues(&repo)?;
            let issue = issues.get(&id)?.unwrap();

            term::issue::show(&issue, &id, term::issue::Format::default(), profile)?;
        }
        NotificationKind::Cob { type_name, id } if type_name == *cob::patch::TYPENAME => {
            let patches = profile.patches(&repo)?;
            let patch = patches.get(&id)?.unwrap();

            term::patch::show(&patch, &id, false, &repo, None, profile)?;
        }
        NotificationKind::Cob { type_name, id } if type_name == *cob::identity::TYPENAME => {
            let identity = Identity::get(&id, &repo)?;

            term::json::to_pretty(&identity.doc, Path::new("radicle.json"))?.print();
        }
        NotificationKind::Branch { .. } => {
            let refstr = if let Some(remote) = n.remote {
                n.qualified
                    .with_namespace(remote.to_component())
                    .to_string()
            } else {
                n.qualified.to_string()
            };
            process::Command::new("git")
                .current_dir(repo.path())
                .args(["log", refstr.as_str()])
                .spawn()?
                .wait()?;
        }
        notification => {
            term::json::to_pretty(&notification, Path::new("notification.json"))?.print();
        }
    }
    notifs.set_status(NotificationStatus::ReadAt(LocalTime::now()), &[id])?;

    Ok(())
}

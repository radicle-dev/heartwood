use std::ffi::OsString;
use std::path::Path;
use std::process;

use anyhow::anyhow;

use git_ref_format::Qualified;
use localtime::LocalTime;
use radicle::cob::TypedId;
use radicle::identity::Identity;
use radicle::issue::cache::Issues as _;
use radicle::node::notifications;
use radicle::node::notifications::*;
use radicle::patch::cache::Patches as _;
use radicle::prelude::{Profile, RepoId};
use radicle::storage::{BranchName, ReadRepository, ReadStorage};
use radicle::{cob, git, Storage};

use radicle_term::Interactive;
use term::Element as _;

use crate::terminal as term;
use crate::terminal::args;
use crate::terminal::args::{Args, Error, Help};
use crate::terminal::command::CommandError;
use crate::tui;

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
    --show-unknown       Show any updates that were not recognized
    --help               Print help
"#,
};

#[derive(Debug, Default, PartialEq, Eq)]
enum Operation {
    #[default]
    Default,
    List,
    Show,
    Clear,
}

impl TryFrom<&str> for Operation {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "default" => Ok(Operation::Default),
            "list" => Ok(Operation::List),
            "show" => Ok(Operation::Show),
            "clear" => Ok(Operation::Clear),
            _ => Err(anyhow!("invalid operation name: {value}")),
        }
    }
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
pub struct SortBy {
    pub reverse: bool,
    pub field: &'static str,
}

pub struct Options {
    op: Operation,
    mode: Mode,
    sort_by: SortBy,
    show_unknown: bool,
    pub interactive: bool,
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
        let mut show_unknown = false;
        let mut interactive = false;

        while let Some(arg) = parser.next()? {
            match arg {
                Long("help") | Short('h') => {
                    return Err(Error::Help.into());
                }
                Long("interactive") | Short('i') => {
                    interactive = true;
                }
                Long("all") | Short('a') if mode.is_none() => {
                    mode = Some(Mode::All);
                }
                Long("reverse") | Short('r') => {
                    reverse = Some(true);
                }
                Long("show-unknown") => {
                    show_unknown = true;
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

        Ok((
            Options {
                op,
                mode,
                sort_by,
                show_unknown,
                interactive,
            },
            vec![],
        ))
    }
}

pub fn run(options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    let storage = &profile.storage;
    let mut notifs = profile.notifications_mut()?;

    let Options {
        op,
        mode,
        sort_by,
        show_unknown,
        interactive,
    } = options;

    match op {
        Operation::Default => {
            if interactive {
                if tui::is_installed() {
                    run_tui_operation(&profile, &storage, &mut notifs, sort_by, mode)?;
                } else {
                    list(
                        mode,
                        sort_by,
                        show_unknown,
                        &notifs.read_only(),
                        storage,
                        &profile,
                    )?;
                    tui::installation_hint();
                }
            } else {
                list(
                    mode,
                    sort_by,
                    show_unknown,
                    &notifs.read_only(),
                    storage,
                    &profile,
                )?;
            }
            Ok(())
        }
        Operation::List => list(
            mode,
            sort_by,
            show_unknown,
            &notifs.read_only(),
            storage,
            &profile,
        ),
        Operation::Clear => clear(mode, &mut notifs),
        Operation::Show => show(mode, &mut notifs, storage, &profile),
    }
}

fn list(
    mode: Mode,
    sort_by: SortBy,
    show_unknown: bool,
    notifs: &notifications::StoreReader,
    storage: &Storage,
    profile: &Profile,
) -> anyhow::Result<()> {
    let repos: Vec<term::VStack<'_>> = match mode {
        Mode::Contextual => {
            if let Ok((_, rid)) = radicle::rad::cwd() {
                list_repo(rid, sort_by, show_unknown, notifs, storage, profile)?
                    .into_iter()
                    .collect()
            } else {
                list_all(sort_by, show_unknown, notifs, storage, profile)?
            }
        }
        Mode::ByRepo(rid) => list_repo(rid, sort_by, show_unknown, notifs, storage, profile)?
            .into_iter()
            .collect(),
        Mode::All => list_all(sort_by, show_unknown, notifs, storage, profile)?,
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
    show_unknown: bool,
    notifs: &notifications::StoreReader,
    storage: &Storage,
    profile: &Profile,
) -> anyhow::Result<Vec<term::VStack<'a>>> {
    let mut repos = storage.repositories()?;
    repos.sort_by_key(|r| r.rid);

    let mut vstacks = Vec::new();
    for repo in repos {
        let vstack = list_repo(repo.rid, sort_by, show_unknown, notifs, storage, profile)?;
        vstacks.extend(vstack.into_iter());
    }
    Ok(vstacks)
}

fn list_repo<'a, R: ReadStorage>(
    rid: RepoId,
    sort_by: SortBy,
    show_unknown: bool,
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
        let author = n
            .remote
            .map(|r| {
                let (alias, _) = term::format::Author::new(&r, profile).labels();
                alias
            })
            .unwrap_or_default();
        let notification_id = term::format::dim(format!("{:-03}", n.id)).into();
        let timestamp = term::format::italic(term::format::timestamp(n.timestamp)).into();

        let NotificationRow {
            category,
            summary,
            state,
            name,
        } = match &n.kind {
            NotificationKind::Branch { name } => NotificationRow::branch(name, head, &n, &repo)?,
            NotificationKind::Cob { typed_id } => {
                match NotificationRow::cob(typed_id, &n, &issues, &patches, &repo)? {
                    Some(row) => row,
                    None => continue,
                }
            }
            NotificationKind::Unknown { refname } => {
                if show_unknown {
                    NotificationRow::unknown(refname, &n, &repo)?
                } else {
                    continue;
                }
            }
        };
        table.push([
            notification_id,
            seen,
            name.into(),
            summary.into(),
            category.into(),
            state.into(),
            author,
            timestamp,
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

struct NotificationRow {
    category: term::Paint<String>,
    summary: term::Paint<String>,
    state: term::Paint<String>,
    name: term::Paint<term::Paint<String>>,
}

impl NotificationRow {
    fn new(
        category: String,
        summary: String,
        state: term::Paint<String>,
        name: term::Paint<String>,
    ) -> Self {
        Self {
            category: term::format::dim(category),
            summary: term::Paint::new(summary),
            state,
            name: term::format::tertiary(name),
        }
    }

    fn branch<S>(
        name: &BranchName,
        head: git::Oid,
        n: &Notification,
        repo: &S,
    ) -> anyhow::Result<Self>
    where
        S: ReadRepository,
    {
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
            Ok(Some(true)) => term::Paint::<String>::from(term::format::secondary("merged")),
            Ok(Some(false)) | Ok(None) => term::format::ref_update(&n.update).into(),
            Err(e) => return Err(e.into()),
        }
        .to_owned();

        Ok(Self::new(
            "branch".to_string(),
            commit,
            state,
            term::format::default(name.to_string()),
        ))
    }

    fn cob<S, I, P>(
        typed_id: &TypedId,
        n: &Notification,
        issues: &I,
        patches: &P,
        repo: &S,
    ) -> anyhow::Result<Option<Self>>
    where
        S: ReadRepository + cob::Store,
        I: cob::issue::cache::Issues,
        P: cob::patch::cache::Patches,
    {
        let TypedId { id, .. } = typed_id;
        let (category, summary, state) = if typed_id.is_issue() {
            let Some(issue) = issues.get(id)? else {
                // Issue could have been deleted after notification was created.
                return Ok(None);
            };
            (
                String::from("issue"),
                issue.title().to_owned(),
                term::format::issue::state(issue.state()),
            )
        } else if typed_id.is_patch() {
            let Some(patch) = patches.get(id)? else {
                // Patch could have been deleted after notification was created.
                return Ok(None);
            };
            (
                String::from("patch"),
                patch.title().to_owned(),
                term::format::patch::state(patch.state()),
            )
        } else if typed_id.is_identity() {
            let Ok(identity) = Identity::get(id, repo) else {
                log::error!(
                    target: "cli",
                    "Error retrieving identity {id} for notification {}", n.id
                );
                return Ok(None);
            };
            let Some(rev) = n.update.new().and_then(|id| identity.revision(&id)) else {
                log::error!(
                    target: "cli",
                    "Error retrieving identity revision for notification {}", n.id
                );
                return Ok(None);
            };
            (
                String::from("id"),
                rev.title.clone(),
                term::format::identity::state(&rev.state),
            )
        } else {
            (
                typed_id.type_name.to_string(),
                "".to_owned(),
                term::format::default(String::new()),
            )
        };
        Ok(Some(Self::new(
            category,
            summary,
            state,
            term::format::cob(id),
        )))
    }

    fn unknown<S>(refname: &Qualified<'static>, n: &Notification, repo: &S) -> anyhow::Result<Self>
    where
        S: ReadRepository,
    {
        let commit = if let Some(head) = n.update.new() {
            repo.commit(head)?.summary().unwrap_or_default().to_owned()
        } else {
            String::new()
        };
        Ok(Self::new(
            "unknown".to_string(),
            commit,
            "".into(),
            term::format::default(refname.to_string()),
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
        NotificationKind::Cob { typed_id } if typed_id.is_issue() => {
            let issues = profile.issues(&repo)?;
            let issue = issues.get(&typed_id.id)?.unwrap();

            term::issue::show(
                &issue,
                &typed_id.id,
                term::issue::Format::default(),
                profile,
            )?;
        }
        NotificationKind::Cob { typed_id } if typed_id.is_patch() => {
            let patches = profile.patches(&repo)?;
            let patch = patches.get(&typed_id.id)?.unwrap();

            term::patch::show(&patch, &typed_id.id, false, &repo, None, profile)?;
        }
        NotificationKind::Cob { typed_id } if typed_id.is_identity() => {
            let identity = Identity::get(&typed_id.id, &repo)?;

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

/// Calls the inbox operation selection with `rad-tui inbox select`. An empty selection
/// signals that the user did not select anything and exited the program. If the selection
/// is not empty, the operation given will be called on the notification given.
fn run_tui_operation(
    profile: &Profile,
    storage: &Storage,
    notifs: &mut notifications::StoreWriter,
    sort_by: SortBy,
    mode: Mode,
) -> anyhow::Result<()> {
    let cmd = tui::Command::InboxSelectOperation {};

    match tui::Selection::from_command(cmd) {
        Ok(Some(selection)) => {
            let operation = selection
                .operation()
                .ok_or_else(|| anyhow!("an operation must be provided"))?;

            let notif_id: NotificationId = *selection
                .ids()
                .first()
                .ok_or_else(|| anyhow!("a notification must be provided"))?;

            match Operation::try_from(operation.as_str()) {
                Ok(Operation::Show) => show(mode, notifs, storage, &profile),
                Ok(Operation::Clear) => clear(mode, notifs),
                Ok(_) => Err(anyhow!("operation not supported: {operation}")),
                Err(err) => Err(err),
            }
        }
        Ok(None) => Ok(()),
        Err(err) => Err(err.into()),
    }
}

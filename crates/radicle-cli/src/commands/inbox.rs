mod args;

pub use args::Args;

use std::path::Path;
use std::process;

use anyhow::anyhow;

use localtime::LocalTime;
use radicle::cob::TypedId;
use radicle::git::fmt::Qualified;
use radicle::git::BranchName;
use radicle::identity::Identity;
use radicle::issue::cache::Issues as _;
use radicle::node::notifications;
use radicle::node::notifications::*;
use radicle::patch::cache::Patches as _;
use radicle::prelude::{NodeId, Profile, RepoId};
use radicle::storage::{ReadRepository, ReadStorage};
use radicle::{cob, git, Storage};

use term::Element as _;

use crate::terminal as term;
use args::{ClearMode, Command, ListMode, SortBy};

pub fn run(args: Args, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    let storage = &profile.storage;
    let mut notifs = profile.notifications_mut()?;
    let command = args
        .clone()
        .command
        .unwrap_or_else(|| Command::List(args.empty.into()));

    match command {
        Command::List(args) => {
            let show_unknown = args.show_unknown;
            let sort_by = args.sort_by;
            let reverse = args.reverse;

            list(
                &notifs.read_only(),
                args.into(),
                sort_by,
                reverse,
                show_unknown,
                storage,
                &profile,
            )
        }
        Command::Clear(args) => clear(&mut notifs, args.into()),
        Command::Show { id } => show(&mut notifs, id, storage, &profile),
    }
}

fn list(
    notifs: &notifications::StoreReader,
    mode: ListMode,
    sort_by: SortBy,
    reverse: bool,
    show_unknown: bool,
    storage: &Storage,
    profile: &Profile,
) -> anyhow::Result<()> {
    let repos: Vec<term::VStack<'_>> = match mode {
        ListMode::Contextual => {
            if let Ok((_, rid)) = radicle::rad::cwd() {
                list_repo(
                    notifs,
                    rid,
                    sort_by,
                    reverse,
                    show_unknown,
                    storage,
                    profile,
                )?
                .into_iter()
                .collect()
            } else {
                list_all(notifs, sort_by, reverse, show_unknown, storage, profile)?
            }
        }
        ListMode::All => list_all(notifs, sort_by, reverse, show_unknown, storage, profile)?,
        ListMode::ByRepo(rid) => list_repo(
            notifs,
            rid,
            sort_by,
            reverse,
            show_unknown,
            storage,
            profile,
        )?
        .into_iter()
        .collect(),
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
    notifs: &notifications::StoreReader,
    sort_by: SortBy,
    reverse: bool,
    show_unknown: bool,
    storage: &Storage,
    profile: &Profile,
) -> anyhow::Result<Vec<term::VStack<'a>>> {
    let mut repos = storage.repositories()?;
    repos.sort_by_key(|r| r.rid);

    let mut vstacks = Vec::new();
    for repo in repos {
        let vstack = list_repo(
            notifs,
            repo.rid,
            sort_by,
            reverse,
            show_unknown,
            storage,
            profile,
        )?;
        vstacks.extend(vstack.into_iter());
    }
    Ok(vstacks)
}

fn list_repo<'a, R: ReadStorage>(
    notifs: &notifications::StoreReader,
    rid: RepoId,
    sort_by: SortBy,
    reverse: bool,
    show_unknown: bool,
    storage: &R,
    profile: &Profile,
) -> anyhow::Result<Option<term::VStack<'a>>>
where
    <R as ReadStorage>::Repository: cob::Store<Namespace = NodeId>,
{
    let repo = storage.repository(rid)?;
    let (_, head) = repo.head()?;
    let doc = repo.identity_doc()?;
    let proj = doc.project()?;
    let issues = term::cob::issues(profile, &repo)?;
    let patches = term::cob::patches(profile, &repo)?;

    let mut notifs = notifs
        .by_repo(&rid, &sort_by.to_string())?
        .collect::<Vec<_>>();
    if !reverse {
        // Notifications are returned in descendant order by default.
        notifs.reverse();
    }

    let table = notifs.into_iter().flat_map(|n| {
        let n: Notification = match n {
            Err(e) => return Some(Err(anyhow::Error::from(e))),
            Ok(n) => n,
        };

        let seen = if n.status.is_read() {
            term::Label::blank()
        } else {
            term::format::tertiary(String::from("●")).into()
        };
        let author = n
            .remote
            .map(|r| {
                let (alias, _) = term::format::Author::new(&r, profile, false).labels();
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
            NotificationKind::Branch { name } => match NotificationRow::branch(name, head, &n, &repo) {
                Err(e) => return Some(Err(e)),
                Ok(b) => b,
            },
            NotificationKind::Cob { typed_id } => {
                match NotificationRow::cob(typed_id, &n, &issues, &patches, &repo) {
                    Ok(Some(row)) => row,
                    Ok(None) => return None,
                    Err(e) => {
                        log::error!(target: "cli", "Error loading notification for {typed_id}: {e}");
                        return None
                    }
                }
            }
            NotificationKind::Unknown { refname } => {
                if show_unknown {
                    match NotificationRow::unknown(refname, &n, &repo) {
                        Err(e) => return Some(Err(e)),
                        Ok(u) => u,
                    }
                } else {
                    return None
                }
            }
        };

        Some(Ok([
            notification_id,
            seen,
            name.into(),
            summary.into(),
            category.into(),
            state.into(),
            author,
            timestamp,
        ]))
    }).collect::<Result<term::Table<8, _>, anyhow::Error>>()?
    .with_opts(term::TableOptions {
        spacing: 3,
        ..term::TableOptions::default()
    });

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
            summary: term::Paint::new(summary.to_string()),
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
                rev.title.to_string(),
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

fn clear(notifs: &mut notifications::StoreWriter, mode: ClearMode) -> anyhow::Result<()> {
    let cleared = match mode {
        ClearMode::ByNotifications(ids) => notifs.clear(&ids)?,
        ClearMode::ByRepo(rid) => notifs.clear_by_repo(&rid)?,
        ClearMode::All => notifs.clear_all()?,
        ClearMode::Contextual => {
            if let Ok((_, rid)) = radicle::rad::cwd() {
                notifs.clear_by_repo(&rid)?
            } else {
                return Err(anyhow!("not a radicle repository"));
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
    notifs: &mut notifications::StoreWriter,
    id: NotificationId,
    storage: &Storage,
    profile: &Profile,
) -> anyhow::Result<()> {
    let n = notifs.get(id)?;
    let repo = storage.repository(n.repo)?;

    match n.kind {
        NotificationKind::Cob { typed_id } if typed_id.is_issue() => {
            let issues = term::cob::issues(profile, &repo)?;
            let issue = issues.get(&typed_id.id)?.unwrap();

            term::issue::show(
                &issue,
                &typed_id.id,
                term::issue::Format::default(),
                false,
                profile,
            )?;
        }
        NotificationKind::Cob { typed_id } if typed_id.is_patch() => {
            let patches = term::cob::patches(profile, &repo)?;
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

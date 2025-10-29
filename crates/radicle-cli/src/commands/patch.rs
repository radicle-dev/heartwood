mod archive;
mod args;
mod assign;
mod cache;
mod checkout;
mod comment;
mod delete;
mod diff;
mod edit;
mod label;
mod list;
mod react;
mod ready;
mod redact;
mod resolve;
mod review;
mod show;
mod update;

use std::collections::BTreeSet;

use anyhow::anyhow;

use radicle::cob::patch::PatchId;
use radicle::cob::{patch, Label};
use radicle::patch::cache::Patches as _;
use radicle::storage::git::transport;
use radicle::{prelude::*, Node};

use crate::git::Rev;
use crate::node;
use crate::terminal as term;
use crate::terminal::patch::Message;

pub use args::Args;

use args::{AssignArgs, Command, CommentAction, LabelArgs};

pub fn run(args: Args, ctx: impl term::Context) -> anyhow::Result<()> {
    let (workdir, rid) = if let Some(rid) = args.repo {
        (None, rid)
    } else {
        radicle::rad::cwd()
            .map(|(workdir, rid)| (Some(workdir), rid))
            .map_err(|_| anyhow!("this command must be run in the context of a repository"))?
    };

    let profile = ctx.profile()?;
    let repository = profile.storage.repository(rid)?;

    // Fallback to [`Command::List`] if no subcommand is provided.
    // Construct it using the [`EmptyArgs`] in `args.empty`.
    let mut announce = args.should_announce();
    let command = args
        .command
        .unwrap_or_else(|| Command::List(args.empty.into()));
    announce &= command.should_announce();

    transport::local::register(profile.storage.clone());

    match command {
        Command::List(args) => {
            let mut authors: BTreeSet<Did> = args.authors.iter().cloned().collect();
            if args.authored {
                authors.insert(profile.did());
            }
            list::run((&args.state).into(), authors, &repository, &profile)?;
        }

        Command::Show { id, patch, verbose } => {
            let patch_id = id.resolve(&repository.backend)?;
            show::run(
                &patch_id,
                patch,
                verbose,
                &profile,
                &repository,
                workdir.as_ref(),
            )?;
        }

        Command::Diff { id, revision } => {
            let patch_id = id.resolve(&repository.backend)?;
            let revision_id = revision
                .map(|rev| rev.resolve::<radicle::git::Oid>(&repository.backend))
                .transpose()?
                .map(patch::RevisionId::from);
            diff::run(&patch_id, revision_id, &repository, &profile)?;
        }

        Command::Update { id, base, message } => {
            let message = Message::from(message);
            let patch_id = id.resolve(&repository.backend)?;
            let base_id = base
                .as_ref()
                .map(|base| base.resolve(&repository.backend))
                .transpose()?;
            let workdir = workdir.ok_or(anyhow!(
                "this command must be run from a repository checkout"
            ))?;

            update::run(patch_id, base_id, message, &profile, &repository, &workdir)?;
        }

        Command::Archive { id, undo } => {
            let patch_id = id.resolve::<PatchId>(&repository.backend)?;
            archive::run(&patch_id, undo, &profile, &repository)?;
        }

        Command::Ready { id, undo } => {
            let patch_id = id.resolve::<PatchId>(&repository.backend)?;

            if !ready::run(&patch_id, undo, &profile, &repository)? {
                if undo {
                    anyhow::bail!("the patch must be open to be put in draft state");
                } else {
                    anyhow::bail!("this patch must be in draft state to be put in open state");
                }
            }
        }

        Command::Delete { id } => {
            let patch_id = id.resolve::<PatchId>(&repository.backend)?;
            delete::run(&patch_id, &profile, &repository)?;
        }

        Command::Checkout { id, revision, opts } => {
            let patch_id = id.resolve::<radicle::git::Oid>(&repository.backend)?;
            let revision_id = revision
                .map(|rev| rev.resolve::<radicle::git::Oid>(&repository.backend))
                .transpose()?
                .map(patch::RevisionId::from);
            let workdir = workdir.ok_or(anyhow!(
                "this command must be run from a repository checkout"
            ))?;
            checkout::run(
                &patch::PatchId::from(patch_id),
                revision_id,
                &repository,
                &workdir,
                &profile,
                opts.into(),
            )?;
        }

        Command::Comment(c) => match CommentAction::from(c) {
            CommentAction::Comment {
                revision,
                message,
                reply_to,
            } => {
                comment::run(
                    revision,
                    message,
                    reply_to,
                    args.quiet,
                    &repository,
                    &profile,
                )?;
            }
            CommentAction::Edit {
                revision,
                comment,
                message,
            } => {
                let comment = comment.resolve(&repository.backend)?;
                comment::edit::run(
                    revision,
                    comment,
                    message,
                    args.quiet,
                    &repository,
                    &profile,
                )?;
            }
            CommentAction::Redact { revision, comment } => {
                let comment = comment.resolve(&repository.backend)?;
                comment::redact::run(revision, comment, &repository, &profile)?;
            }
            CommentAction::React {
                revision,
                comment,
                emoji,
                undo,
            } => {
                let comment = comment.resolve(&repository.backend)?;
                if undo {
                    comment::react::run(revision, comment, emoji, false, &repository, &profile)?;
                } else {
                    comment::react::run(revision, comment, emoji, true, &repository, &profile)?;
                }
            }
        },

        Command::Review {
            id,
            revision,
            options,
        } => {
            let patch_id = id.resolve(&repository.backend)?;
            let revision_id = revision
                .map(|rev| rev.resolve::<radicle::git::Oid>(&repository.backend))
                .transpose()?
                .map(patch::RevisionId::from);
            review::run(patch_id, revision_id, options.into(), &profile, &repository)?;
        }

        Command::Resolve {
            id,
            review,
            comment,
            unresolve,
        } => {
            let patch = id.resolve(&repository.backend)?;
            let review = patch::ReviewId::from(
                review.resolve::<radicle::cob::EntryId>(&repository.backend)?,
            );
            let comment = comment.resolve(&repository.backend)?;
            if unresolve {
                resolve::unresolve(patch, review, comment, &repository, &profile)?;
                term::success!("Unresolved comment {comment}");
            } else {
                resolve::resolve(patch, review, comment, &repository, &profile)?;
                term::success!("Resolved comment {comment}");
            }
        }
        Command::Edit {
            id,
            revision,
            message,
        } => {
            let message = Message::from(message);
            let patch_id = id.resolve(&repository.backend)?;
            let revision_id = revision
                .map(|id| id.resolve::<radicle::git::Oid>(&repository.backend))
                .transpose()?
                .map(patch::RevisionId::from);
            edit::run(&patch_id, revision_id, message, &profile, &repository)?;
        }
        Command::Redact { id } => {
            redact::run(&id, &profile, &repository)?;
        }
        Command::Assign {
            id,
            args: AssignArgs { add, delete },
        } => {
            let patch_id = id.resolve(&repository.backend)?;
            assign::run(
                &patch_id,
                add.into_iter().collect(),
                delete.into_iter().collect(),
                &profile,
                &repository,
            )?;
        }
        Command::Label {
            id,
            args: LabelArgs { add, delete },
        } => {
            let patch_id = id.resolve(&repository.backend)?;
            label::run(
                &patch_id,
                add.into_iter().collect(),
                delete.into_iter().collect(),
                &profile,
                &repository,
            )?;
        }
        Command::Set { id, remote } => {
            let patches = term::cob::patches(&profile, &repository)?;
            let patch_id = id.resolve(&repository.backend)?;
            let patch = patches
                .get(&patch_id)?
                .ok_or_else(|| anyhow!("patch {patch_id} not found"))?;
            let workdir = workdir.ok_or(anyhow!(
                "this command must be run from a repository checkout"
            ))?;
            radicle::rad::setup_patch_upstream(
                &patch_id,
                *patch.head(),
                &workdir,
                remote.as_ref().unwrap_or(&radicle::rad::REMOTE_NAME),
                true,
            )?;
        }
        Command::Cache { id, storage } => {
            let mode = if storage {
                cache::CacheMode::Storage
            } else {
                let patch_id = id.map(|id| id.resolve(&repository.backend)).transpose()?;
                patch_id.map_or(
                    cache::CacheMode::Repository {
                        repository: &repository,
                    },
                    |id| cache::CacheMode::Patch {
                        id,
                        repository: &repository,
                    },
                )
            };
            cache::run(mode, &profile)?;
        }
        Command::React {
            id,
            emoji: react,
            undo,
        } => {
            if undo {
                react::run(&id, react, false, &repository, &profile)?;
            } else {
                react::run(&id, react, true, &repository, &profile)?;
            }
        }
    }

    if announce {
        let mut node = Node::new(profile.socket());
        node::announce(
            &repository,
            node::SyncSettings::default(),
            node::SyncReporting::default(),
            &mut node,
            &profile,
        )?;
    }
    Ok(())
}

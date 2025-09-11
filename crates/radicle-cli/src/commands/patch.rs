mod archive;
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

#[path = "patch/args.rs"]
mod args;

pub use self::args::Args;

use std::collections::BTreeSet;

use anyhow::anyhow;

use radicle::cob::patch::PatchId;
use radicle::cob::{patch, Label};
use radicle::patch::cache::Patches as _;
use radicle::storage::git::transport;
use radicle::{prelude::*, Node};

use crate::commands::patch::args::{Command, CommentSubcommand};
use crate::git::Rev;
use crate::node;
use crate::terminal as term;
use crate::terminal::args::{Error, Help};
use crate::terminal::patch::Message;

pub const HELP: Help = Help {
    name: "patch",
    description: "Manage patches",
    version: env!("RADICLE_VERSION"),
    usage: r#"
Usage

    rad patch [<option>...]
    rad patch list [--all|--merged|--open|--archived|--draft|--authored] [--author <did>]... [<option>...]
    rad patch show <patch-id> [<option>...]
    rad patch diff <patch-id> [<option>...]
    rad patch archive <patch-id> [--undo] [<option>...]
    rad patch update <patch-id> [<option>...]
    rad patch checkout <patch-id> [<option>...]
    rad patch review <patch-id> [--accept | --reject] [-m [<string>]] [-d | --delete] [<option>...]
    rad patch resolve <patch-id> [--review <review-id>] [--comment <comment-id>] [--unresolve] [<option>...]
    rad patch delete <patch-id> [<option>...]
    rad patch redact <revision-id> [<option>...]
    rad patch react <patch-id | revision-id> [--react <emoji>] [<option>...]
    rad patch assign <revision-id> [--add <did>] [--delete <did>] [<option>...]
    rad patch label <revision-id> [--add <label>] [--delete <label>] [<option>...]
    rad patch ready <patch-id> [--undo] [<option>...]
    rad patch edit <patch-id> [<option>...]
    rad patch set <patch-id> [<option>...]
    rad patch comment <patch-id | revision-id> [<option>...]
    rad patch cache [<patch-id>] [--storage] [<option>...]

Show options

    -p, --patch                Show the actual patch diff
    -v, --verbose              Show additional information about the patch

Diff options

    -r, --revision <id>        The revision to diff (default: latest)

Comment options

    -m, --message <string>     Provide a comment message via the command-line
        --reply-to <comment>   The comment to reply to
        --edit <comment>       The comment to edit (use --message to edit with the provided message)
        --react <comment>      The comment to react to
        --emoji <char>         The emoji to react with when --react is used
        --redact <comment>     The comment to redact

Edit options

    -m, --message [<string>]   Provide a comment message to the patch or revision (default: prompt)

Review options

    -r, --revision <id>        Review the given revision of the patch
    -p, --patch                Review by patch hunks
        --hunk <index>         Only review a specific hunk
        --accept               Accept a patch or set of hunks
        --reject               Reject a patch or set of hunks
    -U, --unified <n>          Generate diffs with <n> lines of context instead of the usual three
    -d, --delete               Delete a review draft
    -m, --message [<string>]   Provide a comment with the review (default: prompt)

Resolve options

    --review <id>              The review id which the comment is under
    --comment <id>             The comment to (un)resolve
    --undo                     Unresolve the comment

Assign options

    -a, --add    <did>         Add an assignee to the patch (may be specified multiple times).
                               Note: --add will take precedence over --delete

    -d, --delete <did>         Delete an assignee from the patch (may be specified multiple times).
                               Note: --add will take precedence over --delete

Archive options

        --undo                 Unarchive a patch

Label options

    -a, --add    <label>       Add a label to the patch (may be specified multiple times).
                               Note: --add will take precedence over --delete

    -d, --delete <label>       Delete a label from the patch (may be specified multiple times).
                               Note: --add will take precedence over --delete

Update options

    -b, --base <revspec>       Provide a Git revision as the base commit
    -m, --message [<string>]   Provide a comment message to the patch or revision (default: prompt)
        --no-message           Leave the patch or revision comment message blank

List options

        --all                  Show all patches, including merged and archived patches
        --archived             Show only archived patches
        --merged               Show only merged patches
        --open                 Show only open patches (default)
        --draft                Show only draft patches
        --authored             Show only patches that you have authored
        --author <did>         Show only patched where the given user is an author
                               (may be specified multiple times)

Ready options

        --undo                 Convert a patch back to a draft

Checkout options

        --revision <id>        Checkout the given revision of the patch
        --name <string>        Provide a name for the branch to checkout
        --remote <string>      Provide the git remote to use as the upstream
    -f, --force                Checkout the head of the revision, even if the branch already exists

Set options

        --remote <string>      Provide the git remote to use as the upstream

React options

        --emoji <char>         The emoji to react to the patch or revision with

Other options

        --repo <rid>           Operate on the given repository (default: cwd)
        --[no-]announce        Announce changes made to the network
    -q, --quiet                Quiet output
        --help                 Print help
"#,
};

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
    let announce = !args.no_announce && args.command.is_some_and(|c| c.is_announce());

    transport::local::register(profile.storage.clone());

    match args.command {
        Some(Command::List { filter, options, authors }) =>  {
            let mut authors: BTreeSet<Did> = authors.iter().cloned().collect();
            if options.authored {
                authors.insert(profile.did());
            }
            list::run(filter.as_ref(), authors, &repository, &profile)?;
        }

        Some(Command::Show {
            patch_id,
            patch,
            verbose,
        }) => {
            let patch_id = patch_id.resolve(&repository.backend)?;
            show::run(
                &patch_id,
                patch,
                verbose,
                &profile,
                &repository,
                workdir.as_ref(),
            )?;
        }

        Some(Command::Diff {
            patch_id,
            revision_id,
        }) => {
            let patch_id = patch_id.resolve(&repository.backend)?;
            let revision_id = revision_id
                .map(|rev| rev.resolve::<radicle::git::Oid>(&repository.backend))
                .transpose()?
                .map(patch::RevisionId::from);
            diff::run(&patch_id, revision_id, &repository, &profile)?;
        }

        Some(Command::Update {
            ref patch_id,
            ref base_id,
            message: crate::commands::patch::args::UpdateMessageArg { message, no_message: _ },
        }) => {
            let patch_id = patch_id.resolve(&repository.backend)?;
            let base_id = base_id
                .as_ref()
                .map(|base| base.resolve(&repository.backend))
                .transpose()?;
            let workdir = workdir.ok_or(anyhow!(
                "this command must be run from a repository checkout"
            ))?;

            update::run(
                patch_id,
                base_id,
                message.clone(),
                &profile,
                &repository,
                &workdir,
            )?;
        }

        Some(Command::Archive { ref patch_id, undo }) => {
            let patch_id = patch_id.resolve::<PatchId>(&repository.backend)?;
            archive::run(&patch_id, undo, &profile, &repository)?;
        }

        Some(Command::Ready { ref patch_id, undo }) => {
            let patch_id = patch_id.resolve::<PatchId>(&repository.backend)?;

            if !ready::run(&patch_id, undo, &profile, &repository)? {
                if undo {
                    anyhow::bail!("the patch must be open to be put in draft state");
                } else {
                    anyhow::bail!("this patch must be in draft state to be put in open state");
                }
            }
        }

        Some(Command::Delete { patch_id }) => {
            let patch_id = patch_id.resolve::<PatchId>(&repository.backend)?;
            delete::run(&patch_id, &profile, &repository)?;
        }

        Some(Command::Checkout {
            patch_id,
            revision_id,
            opts,
        }) => {
            let patch_id = patch_id.resolve::<radicle::git::Oid>(&repository.backend)?;
            let revision_id = revision_id
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

        Some(Command::Review {
            patch_id,
            revision_id,
            opts,
        }) => {
            let patch_id = patch_id.resolve(&repository.backend)?;
            let revision_id = revision_id
                .map(|rev| rev.resolve::<radicle::git::Oid>(&repository.backend))
                .transpose()?
                .map(patch::RevisionId::from);
            review::run(patch_id, revision_id, opts, &profile, &repository)?;
        }

        Some(Command::Resolve {
            ref patch_id,
            ref review_id,
            ref comment_id,
            undo,
        }) => {
            let patch = patch_id.resolve(&repository.backend)?;
            let review = patch::ReviewId::from(
                review_id.resolve::<radicle::cob::EntryId>(&repository.backend)?,
            );
            let comment = comment_id.resolve(&repository.backend)?;
            if undo {
                resolve::unresolve(patch, review, comment, &repository, &profile)?;
                term::success!("Unresolved comment {comment_id}");
            } else {
                resolve::resolve(patch, review, comment, &repository, &profile)?;
                term::success!("Resolved comment {comment_id}");
            }
        }
        Some(Command::Edit {
            patch_id,
            revision_id,
            message,
        }) => {
            let patch_id = patch_id.resolve(&repository.backend)?;
            let revision_id = revision_id
                .map(|id| id.resolve::<radicle::git::Oid>(&repository.backend))
                .transpose()?
                .map(patch::RevisionId::from);
            edit::run(&patch_id, revision_id, message, &profile, &repository)?;
        }
        Some(Command::Redact { revision_id }) => {
            redact::run(&revision_id, &profile, &repository)?;
        }
        Some(Command::Assign {
            patch_id,
            opts: self::args::AssignArg { add, delete },
        }) => {
            let patch_id = patch_id.resolve(&repository.backend)?;
            assign::run(&patch_id, add, delete, &profile, &repository)?;
        }
        Some(Command::Label {
            patch_id,
            opts: self::args::LabelArg { add, delete },
        }) => {
            let patch_id = patch_id.resolve(&repository.backend)?;
            label::run(&patch_id, add, delete, &profile, &repository)?;
        }
        Some(Command::Set { patch_id, remote }) => {
            let patches = term::cob::patches(&profile, &repository)?;
            let patch_id = patch_id.resolve(&repository.backend)?;
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
        Some(Command::Cache { patch_id, storage }) => {
            let mode = if storage {
                cache::CacheMode::Storage
            } else {
                let patch_id = patch_id
                    .map(|id| id.resolve(&repository.backend))
                    .transpose()?;
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
        Some(Command::Comment { subcommand: CommentSubcommand::Edit {
            revision_id,
            comment_id,
            message,
        }}) => {
            let comment = comment_id.resolve(&repository.backend)?;
            comment::edit::run(
                revision_id,
                comment,
                message,
                args.quiet,
                &repository,
                &profile,
            )?;
        }
        Some(Command::Comment { subcommand: CommentSubcommand::Redact {
            revision_id,
            comment_id,
        }}) => {
            let comment = comment_id.resolve(&repository.backend)?;
            comment::redact::run(revision_id, comment, &repository, &profile)?;
        }
        Some(Command::Comment { subcommand: CommentSubcommand::React {
            revision_id,
            comment_id,
            reaction,
            undo,
        }}) => {
            let comment = comment_id.resolve(&repository.backend)?;
            if undo {
                comment::react::run(revision_id, comment, reaction, false, &repository, &profile)?;
            } else {
                comment::react::run(revision_id, comment, reaction, true, &repository, &profile)?;
            }
        }
        Some(Command::React {
            revision_id,
            reaction,
            undo,
        }) => {
            if undo {
                react::run(&revision_id, reaction, false, &repository, &profile)?;
            } else {
                react::run(&revision_id, reaction, true, &repository, &profile)?;
            }
        }

        None => {
            unimplemented!(),
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

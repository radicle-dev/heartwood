#[path = "patch/archive.rs"]
mod archive;
#[path = "patch/assign.rs"]
mod assign;
#[path = "patch/cache.rs"]
mod cache;
#[path = "patch/checkout.rs"]
mod checkout;
#[path = "patch/comment.rs"]
mod comment;
#[path = "patch/delete.rs"]
mod delete;
#[path = "patch/diff.rs"]
mod diff;
#[path = "patch/edit.rs"]
mod edit;
#[path = "patch/label.rs"]
mod label;
#[path = "patch/list.rs"]
mod list;
#[path = "patch/ready.rs"]
mod ready;
#[path = "patch/redact.rs"]
mod redact;
#[path = "patch/review.rs"]
mod review;
#[path = "patch/show.rs"]
mod show;
#[path = "patch/update.rs"]
mod update;

use std::collections::BTreeSet;
use std::ffi::OsString;

use anyhow::anyhow;

use radicle::cob::patch::PatchId;
use radicle::cob::{patch, Label};
use radicle::patch::cache::Patches as _;
use radicle::storage::git::transport;
use radicle::{prelude::*, Node};

use crate::git::Rev;
use crate::node;
use crate::terminal as term;
use crate::terminal::args::{string, Args, Error, Help};
use crate::terminal::patch::Message;

pub const HELP: Help = Help {
    name: "patch",
    description: "Manage patches",
    version: env!("CARGO_PKG_VERSION"),
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
    rad patch delete <patch-id> [<option>...]
    rad patch redact <revision-id> [<option>...]
    rad patch assign <revision-id> [--add <did>] [--delete <did>] [<option>...]
    rad patch label <revision-id> [--add <label>] [--delete <label>] [<option>...]
    rad patch ready <patch-id> [--undo] [<option>...]
    rad patch edit <patch-id> [<option>...]
    rad patch set <patch-id> [<option>...]
    rad patch comment <patch-id | revision-id> [<option>...]
    rad patch cache [<patch-id>] [<option>...]

Show options

    -p, --patch                Show the actual patch diff
    -v, --verbose              Show additional information about the patch
        --debug                Show the patch as Rust debug output

Diff options

    -r, --revision <id>        The revision to diff (default: latest)

Comment options

    -m, --message <string>     Provide a comment message via the command-line
        --reply-to <comment>   The comment to reply to

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
    -f, --force                Checkout the head of the revision, even if the branch already exists

Other options

        --repo <rid>           Operate on the given repository (default: cwd)
        --[no-]announce        Announce changes made to the network
    -q, --quiet                Quiet output
        --help                 Print help
"#,
};

#[derive(Debug, Default, PartialEq, Eq)]
pub enum OperationName {
    Assign,
    Show,
    Diff,
    Update,
    Archive,
    Delete,
    Checkout,
    Comment,
    Ready,
    Review,
    Label,
    #[default]
    List,
    Edit,
    Redact,
    Set,
    Cache,
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
pub enum Operation {
    Show {
        patch_id: Rev,
        diff: bool,
        debug: bool,
    },
    Diff {
        patch_id: Rev,
        revision_id: Option<Rev>,
    },
    Update {
        patch_id: Rev,
        base_id: Option<Rev>,
        message: Message,
    },
    Archive {
        patch_id: Rev,
        undo: bool,
    },
    Ready {
        patch_id: Rev,
        undo: bool,
    },
    Delete {
        patch_id: Rev,
    },
    Checkout {
        patch_id: Rev,
        revision_id: Option<Rev>,
        opts: checkout::Options,
    },
    Comment {
        revision_id: Rev,
        message: Message,
        reply_to: Option<Rev>,
    },
    Review {
        patch_id: Rev,
        revision_id: Option<Rev>,
        opts: review::Options,
    },
    Assign {
        patch_id: Rev,
        opts: AssignOptions,
    },
    Label {
        patch_id: Rev,
        opts: LabelOptions,
    },
    List {
        filter: Option<patch::Status>,
    },
    Edit {
        patch_id: Rev,
        revision_id: Option<Rev>,
        message: Message,
    },
    Redact {
        revision_id: Rev,
    },
    Set {
        patch_id: Rev,
    },
    Cache {
        patch_id: Option<Rev>,
    },
}

impl Operation {
    fn is_announce(&self) -> bool {
        match self {
            Operation::Update { .. }
            | Operation::Archive { .. }
            | Operation::Ready { .. }
            | Operation::Delete { .. }
            | Operation::Comment { .. }
            | Operation::Review { .. }
            | Operation::Assign { .. }
            | Operation::Label { .. }
            | Operation::Edit { .. }
            | Operation::Redact { .. }
            | Operation::Set { .. } => true,
            Operation::Show { .. }
            | Operation::Diff { .. }
            | Operation::Checkout { .. }
            | Operation::List { .. }
            | Operation::Cache { .. } => false,
        }
    }
}

#[derive(Debug)]
pub struct Options {
    pub op: Operation,
    pub repo: Option<RepoId>,
    pub announce: bool,
    pub verbose: bool,
    pub quiet: bool,
    pub authored: bool,
    pub authors: Vec<Did>,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut op: Option<OperationName> = None;
        let mut verbose = false;
        let mut quiet = false;
        let mut authored = false;
        let mut authors = vec![];
        let mut announce = true;
        let mut patch_id = None;
        let mut revision_id = None;
        let mut message = Message::default();
        let mut filter = Some(patch::Status::Open);
        let mut diff = false;
        let mut debug = false;
        let mut undo = false;
        let mut reply_to: Option<Rev> = None;
        let mut checkout_opts = checkout::Options::default();
        let mut assign_opts = AssignOptions::default();
        let mut label_opts = LabelOptions::default();
        let mut review_op = review::Operation::default();
        let mut base_id = None;
        let mut repo = None;

        while let Some(arg) = parser.next()? {
            match arg {
                // Options.
                Long("message") | Short('m') => {
                    if message != Message::Blank {
                        // We skip this code when `no-message` is specified.
                        let txt: String = term::args::string(&parser.value()?);
                        message.append(&txt);
                    }
                }
                Long("no-message") => {
                    message = Message::Blank;
                }
                Long("announce") => {
                    announce = true;
                }
                Long("no-announce") => {
                    announce = false;
                }

                // Show options.
                Long("patch") | Short('p') if op == Some(OperationName::Show) => {
                    diff = true;
                }
                Long("debug") if op == Some(OperationName::Show) => {
                    debug = true;
                }

                // Ready options.
                Long("undo") if op == Some(OperationName::Ready) => {
                    undo = true;
                }

                // Archive options.
                Long("undo") if op == Some(OperationName::Archive) => {
                    undo = true;
                }

                // Update options.
                Short('b') | Long("base") if op == Some(OperationName::Update) => {
                    let val = parser.value()?;
                    let rev = term::args::rev(&val)?;

                    base_id = Some(rev);
                }

                // Comment options.
                Long("reply-to") if op == Some(OperationName::Comment) => {
                    let val = parser.value()?;
                    let rev = term::args::rev(&val)?;

                    reply_to = Some(rev);
                }

                // Edit options.
                Long("revision") | Short('r') if op == Some(OperationName::Edit) => {
                    let val = parser.value()?;
                    let rev = term::args::rev(&val)?;

                    revision_id = Some(rev);
                }

                // Review/diff options.
                Long("revision") | Short('r')
                    if op == Some(OperationName::Review) || op == Some(OperationName::Diff) =>
                {
                    let val = parser.value()?;
                    let rev = term::args::rev(&val)?;

                    revision_id = Some(rev);
                }
                Long("patch") | Short('p') if op == Some(OperationName::Review) => {
                    if let review::Operation::Review { by_hunk, .. } = &mut review_op {
                        *by_hunk = true;
                    } else {
                        return Err(arg.unexpected().into());
                    }
                }
                Long("unified") | Short('U') if op == Some(OperationName::Review) => {
                    if let review::Operation::Review { unified, .. } = &mut review_op {
                        let val = parser.value()?;
                        *unified = term::args::number(&val)?;
                    } else {
                        return Err(arg.unexpected().into());
                    }
                }
                Long("hunk") if op == Some(OperationName::Review) => {
                    if let review::Operation::Review { hunk, .. } = &mut review_op {
                        let val = parser.value()?;
                        let val = term::args::number(&val)
                            .map_err(|e| anyhow!("invalid hunk value: {e}"))?;

                        *hunk = Some(val);
                    } else {
                        return Err(arg.unexpected().into());
                    }
                }
                Long("delete") | Short('d') if op == Some(OperationName::Review) => {
                    review_op = review::Operation::Delete;
                }
                Long("accept") if op == Some(OperationName::Review) => {
                    if let review::Operation::Review {
                        verdict: verdict @ None,
                        ..
                    } = &mut review_op
                    {
                        *verdict = Some(patch::Verdict::Accept);
                    } else {
                        return Err(arg.unexpected().into());
                    }
                }
                Long("reject") if op == Some(OperationName::Review) => {
                    if let review::Operation::Review {
                        verdict: verdict @ None,
                        ..
                    } = &mut review_op
                    {
                        *verdict = Some(patch::Verdict::Reject);
                    } else {
                        return Err(arg.unexpected().into());
                    }
                }

                // Checkout options
                Long("revision") if op == Some(OperationName::Checkout) => {
                    let val = parser.value()?;
                    let rev = term::args::rev(&val)?;

                    revision_id = Some(rev);
                }

                Long("force") | Short('f') if op == Some(OperationName::Checkout) => {
                    checkout_opts.force = true;
                }

                Long("name") if op == Some(OperationName::Checkout) => {
                    let val = parser.value()?;
                    checkout_opts.name = Some(term::args::refstring("name", val)?);
                }

                // Assign options.
                Short('a') | Long("add") if matches!(op, Some(OperationName::Assign)) => {
                    assign_opts.add.insert(term::args::did(&parser.value()?)?);
                }

                Short('d') | Long("delete") if matches!(op, Some(OperationName::Assign)) => {
                    assign_opts
                        .delete
                        .insert(term::args::did(&parser.value()?)?);
                }

                // Label options.
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

                // List options.
                Long("all") => {
                    filter = None;
                }
                Long("draft") => {
                    filter = Some(patch::Status::Draft);
                }
                Long("archived") => {
                    filter = Some(patch::Status::Archived);
                }
                Long("merged") => {
                    filter = Some(patch::Status::Merged);
                }
                Long("open") => {
                    filter = Some(patch::Status::Open);
                }
                Long("authored") => {
                    authored = true;
                }
                Long("author") if op == Some(OperationName::List) => {
                    authors.push(term::args::did(&parser.value()?)?);
                }

                // Common.
                Long("verbose") | Short('v') => {
                    verbose = true;
                }
                Long("quiet") | Short('q') => {
                    quiet = true;
                }
                Long("repo") => {
                    let val = parser.value()?;
                    let rid = term::args::rid(&val)?;

                    repo = Some(rid);
                }
                Long("help") => {
                    return Err(Error::HelpManual { name: "rad-patch" }.into());
                }
                Short('h') => {
                    return Err(Error::Help.into());
                }

                Value(val) if op.is_none() => match val.to_string_lossy().as_ref() {
                    "l" | "list" => op = Some(OperationName::List),
                    "s" | "show" => op = Some(OperationName::Show),
                    "u" | "update" => op = Some(OperationName::Update),
                    "d" | "delete" => op = Some(OperationName::Delete),
                    "c" | "checkout" => op = Some(OperationName::Checkout),
                    "a" | "archive" => op = Some(OperationName::Archive),
                    "y" | "ready" => op = Some(OperationName::Ready),
                    "e" | "edit" => op = Some(OperationName::Edit),
                    "r" | "redact" => op = Some(OperationName::Redact),
                    "diff" => op = Some(OperationName::Diff),
                    "assign" => op = Some(OperationName::Assign),
                    "label" => op = Some(OperationName::Label),
                    "comment" => op = Some(OperationName::Comment),
                    "review" => op = Some(OperationName::Review),
                    "set" => op = Some(OperationName::Set),
                    "cache" => op = Some(OperationName::Cache),
                    unknown => anyhow::bail!("unknown operation '{}'", unknown),
                },
                Value(val) if op == Some(OperationName::Redact) => {
                    let rev = term::args::rev(&val)?;
                    revision_id = Some(rev);
                }
                Value(val)
                    if patch_id.is_none()
                        && [
                            Some(OperationName::Show),
                            Some(OperationName::Diff),
                            Some(OperationName::Update),
                            Some(OperationName::Delete),
                            Some(OperationName::Archive),
                            Some(OperationName::Ready),
                            Some(OperationName::Checkout),
                            Some(OperationName::Comment),
                            Some(OperationName::Review),
                            Some(OperationName::Edit),
                            Some(OperationName::Set),
                            Some(OperationName::Assign),
                            Some(OperationName::Label),
                            Some(OperationName::Cache),
                        ]
                        .contains(&op) =>
                {
                    let val = string(&val);
                    patch_id = Some(Rev::from(val));
                }
                _ => return Err(anyhow::anyhow!(arg.unexpected())),
            }
        }

        let op = match op.unwrap_or_default() {
            OperationName::List => Operation::List { filter },
            OperationName::Show => Operation::Show {
                patch_id: patch_id.ok_or_else(|| anyhow!("a patch must be provided"))?,
                diff,
                debug,
            },
            OperationName::Diff => Operation::Diff {
                patch_id: patch_id.ok_or_else(|| anyhow!("a patch must be provided"))?,
                revision_id,
            },
            OperationName::Delete => Operation::Delete {
                patch_id: patch_id.ok_or_else(|| anyhow!("a patch must be provided"))?,
            },
            OperationName::Update => Operation::Update {
                patch_id: patch_id.ok_or_else(|| anyhow!("a patch must be provided"))?,
                base_id,
                message,
            },
            OperationName::Archive => Operation::Archive {
                patch_id: patch_id.ok_or_else(|| anyhow!("a patch id must be provided"))?,
                undo,
            },
            OperationName::Checkout => Operation::Checkout {
                patch_id: patch_id.ok_or_else(|| anyhow!("a patch must be provided"))?,
                revision_id,
                opts: checkout_opts,
            },
            OperationName::Comment => Operation::Comment {
                revision_id: patch_id
                    .ok_or_else(|| anyhow!("a patch or revision must be provided"))?,
                message,
                reply_to,
            },
            OperationName::Review => Operation::Review {
                patch_id: patch_id
                    .ok_or_else(|| anyhow!("a patch or revision must be provided"))?,
                revision_id,
                opts: review::Options {
                    message,
                    op: review_op,
                },
            },
            OperationName::Ready => Operation::Ready {
                patch_id: patch_id.ok_or_else(|| anyhow!("a patch must be provided"))?,
                undo,
            },
            OperationName::Edit => Operation::Edit {
                patch_id: patch_id.ok_or_else(|| anyhow!("a patch must be provided"))?,
                revision_id,
                message,
            },
            OperationName::Redact => Operation::Redact {
                revision_id: revision_id.ok_or_else(|| anyhow!("a revision must be provided"))?,
            },
            OperationName::Assign => Operation::Assign {
                patch_id: patch_id.ok_or_else(|| anyhow!("a patch must be provided"))?,
                opts: assign_opts,
            },
            OperationName::Label => Operation::Label {
                patch_id: patch_id.ok_or_else(|| anyhow!("a patch must be provided"))?,
                opts: label_opts,
            },
            OperationName::Set => Operation::Set {
                patch_id: patch_id.ok_or_else(|| anyhow!("a patch must be provided"))?,
            },
            OperationName::Cache => Operation::Cache { patch_id },
        };

        Ok((
            Options {
                op,
                repo,
                verbose,
                quiet,
                announce,
                authored,
                authors,
            },
            vec![],
        ))
    }
}

pub fn run(options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    let (workdir, rid) = if let Some(rid) = options.repo {
        (None, rid)
    } else {
        radicle::rad::cwd()
            .map(|(workdir, rid)| (Some(workdir), rid))
            .map_err(|_| anyhow!("this command must be run in the context of a repository"))?
    };

    let profile = ctx.profile()?;
    let repository = profile.storage.repository(rid)?;
    let announce = options.announce && options.op.is_announce();

    transport::local::register(profile.storage.clone());

    match options.op {
        Operation::List { filter } => {
            let mut authors: BTreeSet<Did> = options.authors.iter().cloned().collect();
            if options.authored {
                authors.insert(profile.did());
            }
            list::run(filter.as_ref(), authors, &repository, &profile)?;
        }
        Operation::Show {
            patch_id,
            diff,
            debug,
        } => {
            let patch_id = patch_id.resolve(&repository.backend)?;
            show::run(
                &patch_id,
                diff,
                debug,
                options.verbose,
                &profile,
                &repository,
                workdir.as_ref(),
            )?;
        }
        Operation::Diff {
            patch_id,
            revision_id,
        } => {
            let patch_id = patch_id.resolve(&repository.backend)?;
            let revision_id = revision_id
                .map(|rev| rev.resolve::<radicle::git::Oid>(&repository.backend))
                .transpose()?
                .map(patch::RevisionId::from);
            diff::run(&patch_id, revision_id, &repository, &profile)?;
        }
        Operation::Update {
            ref patch_id,
            ref base_id,
            ref message,
        } => {
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
        Operation::Archive { ref patch_id, undo } => {
            let patch_id = patch_id.resolve::<PatchId>(&repository.backend)?;
            archive::run(&patch_id, undo, &profile, &repository)?;
        }
        Operation::Ready { ref patch_id, undo } => {
            let patch_id = patch_id.resolve::<PatchId>(&repository.backend)?;
            ready::run(&patch_id, undo, &profile, &repository)?;
        }
        Operation::Delete { patch_id } => {
            let patch_id = patch_id.resolve::<PatchId>(&repository.backend)?;
            delete::run(&patch_id, &profile, &repository)?;
        }
        Operation::Checkout {
            patch_id,
            revision_id,
            opts,
        } => {
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
                opts,
            )?;
        }
        Operation::Comment {
            revision_id,
            message,
            reply_to,
        } => {
            comment::run(
                revision_id,
                message,
                reply_to,
                options.quiet,
                &repository,
                &profile,
            )?;
        }
        Operation::Review {
            patch_id,
            revision_id,
            opts,
        } => {
            let patch_id = patch_id.resolve(&repository.backend)?;
            let revision_id = revision_id
                .map(|rev| rev.resolve::<radicle::git::Oid>(&repository.backend))
                .transpose()?
                .map(patch::RevisionId::from);
            review::run(patch_id, revision_id, opts, &profile, &repository)?;
        }
        Operation::Edit {
            patch_id,
            revision_id,
            message,
        } => {
            let patch_id = patch_id.resolve(&repository.backend)?;
            let revision_id = revision_id
                .map(|id| id.resolve::<radicle::git::Oid>(&repository.backend))
                .transpose()?
                .map(patch::RevisionId::from);
            edit::run(&patch_id, revision_id, message, &profile, &repository)?;
        }
        Operation::Redact { revision_id } => {
            redact::run(&revision_id, &profile, &repository)?;
        }
        Operation::Assign {
            patch_id,
            opts: AssignOptions { add, delete },
        } => {
            let patch_id = patch_id.resolve(&repository.backend)?;
            assign::run(&patch_id, add, delete, &profile, &repository)?;
        }
        Operation::Label {
            patch_id,
            opts: LabelOptions { add, delete },
        } => {
            let patch_id = patch_id.resolve(&repository.backend)?;
            label::run(&patch_id, add, delete, &profile, &repository)?;
        }
        Operation::Set { patch_id } => {
            let patches = profile.patches(&repository)?;
            let patch_id = patch_id.resolve(&repository.backend)?;
            let patch = patches
                .get(&patch_id)?
                .ok_or_else(|| anyhow!("patch {patch_id} not found"))?;
            let workdir = workdir.ok_or(anyhow!(
                "this command must be run from a repository checkout"
            ))?;
            radicle::rad::setup_patch_upstream(&patch_id, *patch.head(), &workdir, true)?;
        }
        Operation::Cache { patch_id } => {
            let patch_id = patch_id
                .map(|id| id.resolve(&repository.backend))
                .transpose()?;
            cache::run(patch_id, &repository, &profile)?;
        }
    }

    if announce {
        let mut node = Node::new(profile.socket());
        node::announce(rid, &mut node)?;
    }
    Ok(())
}

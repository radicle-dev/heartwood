#[path = "patch/archive.rs"]
mod archive;
#[path = "patch/assign.rs"]
mod assign;
#[path = "patch/checkout.rs"]
mod checkout;
#[path = "patch/comment.rs"]
mod comment;
#[path = "patch/common.rs"]
mod common;
#[path = "patch/delete.rs"]
mod delete;
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
use radicle::prelude::*;
use radicle::storage::git::transport;

use crate::git::Rev;
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
    rad patch archive <patch-id> [<option>...]
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

Show options

    -p, --patch                Show the actual patch diff
    -v, --verbose              Show additional information about the patch

Comment options

    -m, --message <string>     Provide a comment message via the command-line

Edit options

    -m, --message [<string>]   Provide a comment message to the patch or revision (default: prompt)

Review options

        --revision <id>        Review the given revision of the patch
    -p, --patch                Review by patch hunks
        --hunk <index>         Only review a specific hunk
        --accept               Accept a patch or set of hunks
        --reject               Reject a patch or set of hunks
    -U, --unified <n>          Generate diffs with <n> lines of context instead of the usual three
    -d, --delete               Delete a review draft
    -r, --revision             Review a patch revision
    -m, --message [<string>]   Provide a comment with the review (default: prompt)

Assign options

    -a, --add    <did>         Add an assignee to the patch (may be specified multiple times).
                               Note: --add will take precedence over --delete

    -d, --delete <did>         Delete an assignee from the patch (may be specified multiple times).
                               Note: --add will take precedence over --delete

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

    -q, --quiet                Quiet output
        --help                 Print help
"#,
};

#[derive(Debug, Default, PartialEq, Eq)]
pub enum OperationName {
    Assign,
    Show,
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

pub struct Filter(fn(&patch::State) -> bool);

impl Filter {
    /// Match everything.
    fn all() -> Self {
        Self(|_| true)
    }
}

impl Default for Filter {
    fn default() -> Self {
        Self(|state| matches!(state, patch::State::Open { .. }))
    }
}

impl std::fmt::Debug for Filter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Filter(..)")
    }
}

#[derive(Debug)]
pub enum Operation {
    Show {
        patch_id: Rev,
        diff: bool,
    },
    Update {
        patch_id: Rev,
        base_id: Option<Rev>,
        message: Message,
    },
    Archive {
        patch_id: Rev,
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
        filter: Filter,
    },
    Edit {
        patch_id: Rev,
        message: Message,
    },
    Redact {
        revision_id: Rev,
    },
    Set {
        patch_id: Rev,
    },
}

#[derive(Debug)]
pub struct Options {
    pub op: Operation,
    pub announce: bool,
    pub push: bool,
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
        let mut announce = false;
        let mut patch_id = None;
        let mut revision_id = None;
        let mut message = Message::default();
        let mut push = true;
        let mut filter = Filter::default();
        let mut diff = false;
        let mut undo = false;
        let mut reply_to: Option<Rev> = None;
        let mut checkout_opts = checkout::Options::default();
        let mut assign_opts = AssignOptions::default();
        let mut label_opts = LabelOptions::default();
        let mut review_op = review::Operation::default();
        let mut base_id = None;

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
                Long("push") => {
                    push = true;
                }
                Long("no-push") => {
                    push = false;
                }

                // Show options.
                Long("patch") | Short('p') if op == Some(OperationName::Show) => {
                    diff = true;
                }

                // Ready options.
                Long("undo") if op == Some(OperationName::Ready) => {
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

                // Review options.
                Long("revision") if op == Some(OperationName::Review) => {
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
                    filter = Filter::all();
                }
                Long("draft") => {
                    filter = Filter(|s| s == &patch::State::Draft);
                }
                Long("archived") => {
                    filter = Filter(|s| s == &patch::State::Archived);
                }
                Long("merged") => {
                    filter = Filter(|s| matches!(s, patch::State::Merged { .. }));
                }
                Long("open") => {
                    filter = Filter(|s| matches!(s, patch::State::Open { .. }));
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
                    "assign" => op = Some(OperationName::Assign),
                    "label" => op = Some(OperationName::Label),
                    "comment" => op = Some(OperationName::Comment),
                    "review" => op = Some(OperationName::Review),
                    "set" => op = Some(OperationName::Set),
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
        };

        Ok((
            Options {
                op,
                push,
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
    let (workdir, id) = radicle::rad::cwd()
        .map_err(|_| anyhow!("this command must be run in the context of a project"))?;

    let profile = ctx.profile()?;
    let repository = profile.storage.repository(id)?;

    transport::local::register(profile.storage.clone());

    match options.op {
        Operation::List { filter: Filter(f) } => {
            let mut authors: BTreeSet<Did> = options.authors.iter().cloned().collect();
            if options.authored {
                authors.insert(profile.did());
            }
            list::run(f, authors, &repository, &profile)?;
        }
        Operation::Show { patch_id, diff } => {
            let patch_id = patch_id.resolve(&repository.backend)?;
            show::run(
                &patch_id,
                diff,
                options.verbose,
                &profile,
                &repository,
                &workdir,
            )?;
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
            update::run(
                patch_id,
                base_id,
                message.clone(),
                &profile,
                &repository,
                &workdir,
            )?;
        }
        Operation::Archive { ref patch_id } => {
            let patch_id = patch_id.resolve::<PatchId>(&repository.backend)?;
            archive::run(&patch_id, &profile, &repository)?;
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
            checkout::run(
                &patch::PatchId::from(patch_id),
                revision_id,
                &repository,
                &workdir,
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
        Operation::Edit { patch_id, message } => {
            let patch_id = patch_id.resolve(&repository.backend)?;
            edit::run(&patch_id, message, &profile, &repository)?;
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
            let patches = radicle::cob::patch::Patches::open(&repository)?;
            let patch_id = patch_id.resolve(&repository.backend)?;
            let patch = patches
                .get(&patch_id)?
                .ok_or_else(|| anyhow!("patch {patch_id} not found"))?;

            radicle::rad::setup_patch_upstream(&patch_id, *patch.head(), &workdir, true)?;
        }
    }
    Ok(())
}

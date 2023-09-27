#[path = "patch/archive.rs"]
mod archive;
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
#[path = "patch/list.rs"]
mod list;
#[path = "patch/ready.rs"]
mod ready;
#[path = "patch/redact.rs"]
mod redact;
#[path = "patch/show.rs"]
mod show;
#[path = "patch/update.rs"]
mod update;

use std::ffi::OsString;

use anyhow::anyhow;

use radicle::cob::patch;
use radicle::cob::patch::PatchId;
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
    rad patch list [--all|--merged|--open|--archived|--draft] [<option>...]
    rad patch show <patch-id> [<option>...]
    rad patch archive <patch-id> [<option>...]
    rad patch update <patch-id> [<option>...]
    rad patch checkout <patch-id> [<option>...]
    rad patch delete <patch-id> [<option>...]
    rad patch redact <revision-id> [<option>...]
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

Update options

    -m, --message [<string>]   Provide a comment message to the patch or revision (default: prompt)
        --no-message           Leave the patch or revision comment message blank

List options

        --all                  Show all patches, including merged and archived patches
        --archived             Show only archived patches
        --merged               Show only merged patches
        --open                 Show only open patches (default)
        --draft                Show only draft patches

Ready options

        --undo                 Convert a patch back to a draft

Other options

    -q, --quiet                Quiet output
        --help                 Print help
"#,
};

#[derive(Debug, Default, PartialEq, Eq)]
pub enum OperationName {
    Show,
    Update,
    Archive,
    Delete,
    Checkout,
    Comment,
    Ready,
    #[default]
    List,
    Edit,
    Redact,
    Set,
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
        revision_id: Rev,
    },
    Comment {
        revision_id: Rev,
        message: Message,
        reply_to: Option<Rev>,
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
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut op: Option<OperationName> = None;
        let mut verbose = false;
        let mut quiet = false;
        let mut announce = false;
        let mut patch_id = None;
        let mut revision_id = None;
        let mut message = Message::default();
        let mut push = true;
        let mut filter = Filter::default();
        let mut diff = false;
        let mut undo = false;
        let mut reply_to: Option<Rev> = None;

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
                Long("revision") if op == Some(OperationName::Update) => {
                    let val = parser.value()?;
                    let rev = term::args::oid(&val)?;

                    revision_id = Some(rev);
                }

                // Comment options.
                Long("reply-to") if op == Some(OperationName::Comment) => {
                    let val = parser.value()?;
                    let rev = term::args::oid(&val)?;

                    reply_to = Some(rev);
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

                // Common.
                Long("verbose") | Short('v') => {
                    verbose = true;
                }
                Long("quiet") | Short('q') => {
                    quiet = true;
                }
                Long("help") => {
                    return Err(Error::HelpManual.into());
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
                    "comment" => op = Some(OperationName::Comment),
                    "set" => op = Some(OperationName::Set),
                    unknown => anyhow::bail!("unknown operation '{}'", unknown),
                },
                Value(val) if op == Some(OperationName::Redact) => {
                    let rev = term::args::oid(&val)?;
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
                            Some(OperationName::Edit),
                            Some(OperationName::Set),
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
                message,
            },
            OperationName::Archive => Operation::Archive {
                patch_id: patch_id.ok_or_else(|| anyhow!("a patch id must be provided"))?,
            },
            OperationName::Checkout => Operation::Checkout {
                revision_id: patch_id.ok_or_else(|| anyhow!("a patch must be provided"))?,
            },
            OperationName::Comment => Operation::Comment {
                revision_id: patch_id
                    .ok_or_else(|| anyhow!("a patch or revision must be provided"))?,
                message,
                reply_to,
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
            list::run(f, &repository, &profile)?;
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
            ref message,
        } => {
            let patch_id = patch_id.resolve(&repository.backend)?;
            update::run(patch_id, message.clone(), &profile, &repository, &workdir)?;
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
        Operation::Checkout { revision_id } => {
            let revision_id = revision_id.resolve::<radicle::git::Oid>(&repository.backend)?;
            checkout::run(&patch::RevisionId::from(revision_id), &repository, &workdir)?;
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
        Operation::Edit { patch_id, message } => {
            let patch_id = patch_id.resolve(&repository.backend)?;
            edit::run(&patch_id, message, &profile, &repository)?;
        }
        Operation::Redact { revision_id } => {
            redact::run(&revision_id, &profile, &repository)?;
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

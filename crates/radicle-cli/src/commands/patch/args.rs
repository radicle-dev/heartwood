use clap::{Parser, Subcommand};
use clap_complete::ArgValueCompleter;
use radicle::prelude::Did;

use crate::git::Rev;

#[derive(Parser, Debug)]
pub struct Args {
    /// Subcommand for `radicle issue`
    #[command(subcommand)]
    pub(crate) command: Option<Command>,

    /// Don't print anything
    #[arg(short, long)]
    #[clap(global = true)]
    pub(crate) quiet: bool,

    /// Don't announce issue to peers
    #[arg(long)]
    #[arg(value_name = "no-announce")]
    #[clap(global = true)]
    pub(crate) no_announce: bool,

    /// Show only the issue header, hiding the comments
    #[arg(long)]
    #[clap(global = true)]
    pub(crate) header: bool,

    /// Optionally specify the repository to manage issues for
    #[arg(value_name = "RID")]
    #[arg(long, short)]
    #[clap(global = true)]
    pub(crate) repo: Option<radicle::prelude::RepoId>,

    /// Verbose output
    #[arg(long, short)]
    #[clap(global = true)]
    pub(crate) verbose: bool,
}

#[derive(Debug, clap::Args)]
#[group(required = false, multiple = false)]
pub struct ListOptions {
    #[arg(long, group = "cmd-list")]
    pub all: bool,

    #[arg(long, group = "cmd-list")]
    pub merged: bool,

    #[arg(long, group = "cmd-list")]
    pub open: bool,

    #[arg(long, group = "cmd-list")]
    pub archived: bool,

    #[arg(long, group = "cmd-list")]
    pub draft: bool,

    #[arg(long, group = "cmd-list")]
    pub authored: bool,
}

#[derive(Debug, Parser)]
pub struct ReviewOptions {
    #[clap(long, group = "cmd-review")]
    pub accept: bool,

    #[clap(long, group = "cmd-review")]
    pub reject: bool,

    #[clap(long, short = 'm', group = "cmd-review")]
    pub message: String,

    #[clap(long, short = 'd', group = "cmd-review")]
    pub delete: bool,
}

impl From<ReviewOptions> for crate::commands::patch::review::Options {
    fn from(value: ReviewOptions) -> Self {
        Self {
            message: value.message.into(),
            op: if value.delete {
                crate::commands::patch::review::Operation::Delete
            } else {
                todo!()
            }
        }
    }
}

#[derive(Debug, clap::Args)]
#[group(required = false, multiple = false)]
pub struct UpdateMessageArg {
    #[clap(long = "message", short = 'm', group = "update-message-arg")]
    pub message: crate::terminal::patch::Message,

    #[clap(long = "no-message")]
    pub no_message: bool,
}

#[derive(Debug, clap::Args)]
#[group(required = false, multiple = false)]
pub struct LabelArg {
    #[clap(long, group = "label-arg")]
    pub add: Option<String>,

    #[clap(long, group = "label-arg")]
    pub delete: Option<String>,
}


#[derive(Debug, clap::Args)]
#[group(required = false, multiple = false)]
pub struct AssignArg {
    #[clap(long, group = "assign-arg")]
    pub add: Option<String>,

    #[clap(long, group = "assign-arg")]
    pub delete: Option<String>,
}

#[derive(Debug, clap::Args)]
pub struct CheckoutOptions {
    #[clap(long)]
    pub name: Option<git_ref_format::RefString>,

    #[clap(long)]
    pub remote: Option<git_ref_format::RefString>,

    #[clap(long)]
    pub force: bool,
}

impl From<CheckoutOptions> for crate::commands::patch::checkout::Options {
    fn from(value: CheckoutOptions) -> Self {
        Self {
            name: value.name,
            remote: value.remote,
            force: value.force,
        }
    }
}

/// Commands to create, view, and edit Radicle issues
#[derive(Subcommand, Debug)]
pub enum Command {
    List {
        #[clap(flatten)]
        options: ListOptions,

        #[clap(long)]
        authors: Vec<Did>,

        #[clap(long)]
        filter: Option<radicle::patch::Status>,
    },

    Show {
        #[arg(value_name = "PATCH_ID", add = ArgValueCompleter::new(crate::commands::hints::patch_ids_completer))]
        patch_id: Rev,

        // Show the actual patch diff
        patch: bool,

        /// Show additional information about the patch
        verbose: bool,
    },

    Diff {
        #[arg(value_name = "PATCH_ID", add = ArgValueCompleter::new(crate::commands::hints::patch_ids_completer))]
        patch_id: Rev,

        /// The revision of diff (default: latest)
        #[clap(long = "revision", short = 'r')]
        revision_id: Option<Rev>,
    },

    Archive {
        #[arg(value_name = "PATCH_ID", add = ArgValueCompleter::new(crate::commands::hints::patch_ids_completer))]
        patch_id: Rev,

        undo: bool,
    },

    Update {
        #[arg(value_name = "PATCH_ID", add = ArgValueCompleter::new(crate::commands::hints::patch_ids_completer))]
        patch_id: Rev,

        #[clap(long = "base", short = 'n')]
        base_id: Option<Rev>,

        #[clap(flatten)]
        message: UpdateMessageArg,
    },

    Checkout {
        #[arg(value_name = "PATCH_ID", add = ArgValueCompleter::new(crate::commands::hints::patch_ids_completer))]
        patch_id: Rev,

        #[clap(long)]
        revision_id: Option<Rev>,

        #[clap(flatten)]
        opts: CheckoutOptions,
    },

    Review {
        #[arg(value_name = "PATCH_ID", add = ArgValueCompleter::new(crate::commands::hints::patch_ids_completer))]
        patch_id: Rev,

        #[clap(long)]
        revision_id: Option<Rev>,

        #[clap(flatten)]
        options: ReviewOptions,
    },

    Resolve {
        #[arg(value_name = "PATCH_ID", add = ArgValueCompleter::new(crate::commands::hints::patch_ids_completer))]
        patch_id: Rev,

        #[clap(long)]
        review_id: Option<Rev>,

        #[clap(long)]
        comment_id: Option<Rev>,

        #[clap(long)]
        undo: bool,
    },

    Delete {
        #[arg(value_name = "PATCH_ID", add = ArgValueCompleter::new(crate::commands::hints::patch_ids_completer))]
        patch_id: Rev,
    },

    Redact {
        #[arg(value_name = "PATCH_ID", add = ArgValueCompleter::new(crate::commands::hints::patch_ids_completer))]
        patch_id: Rev,
    },

    React {
        #[arg(value_name = "PATCH_ID", add = ArgValueCompleter::new(crate::commands::hints::patch_ids_completer))]
        patch_id: Rev,

        #[clap(long)]
        react: Option<radicle::cob::Reaction>,
    },

    Assign {
        #[arg(value_name = "PATCH_ID", add = ArgValueCompleter::new(crate::commands::hints::patch_ids_completer))]
        patch_id: Rev,

        #[clap(flatten)]
        opts: AssignArg,
    },

    Label {
        #[arg(value_name = "PATCH_ID", add = ArgValueCompleter::new(crate::commands::hints::patch_ids_completer))]
        patch_id: Rev,

        #[clap(flatten)]
        opts: LabelArg,
    },

    Ready {
        #[arg(value_name = "PATCH_ID", add = ArgValueCompleter::new(crate::commands::hints::patch_ids_completer))]
        patch_id: Rev,

        undo: bool,
    },

    Edit {
        #[arg(value_name = "PATCH_ID", add = ArgValueCompleter::new(crate::commands::hints::patch_ids_completer))]
        patch_id: Rev,
    },

    Set {
        #[arg(value_name = "PATCH_ID", add = ArgValueCompleter::new(crate::commands::hints::patch_ids_completer))]
        patch_id: Rev,
    },

    Comment {
        #[command(subcommand)]
        subcommand: CommentSubcommand,
    },

    Cache {
        #[arg(value_name = "PATCH_ID", add = ArgValueCompleter::new(crate::commands::hints::patch_ids_completer))]
        patch_id: Option<Rev>,

        #[arg(long)]
        storage: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum CommentSubcommand {
    Edit {
        #[arg(value_name = "REVISION", add = ArgValueCompleter::new(crate::commands::hints::patch_ids_completer))]
        revision_id: Rev,
        #[arg(value_name = "COMMENT", add = ArgValueCompleter::new(crate::commands::hints::patch_ids_completer))]
        comment_id: Rev,

        #[arg(long, short)]
        message: crate::terminal::patch::Message,
    },
    Redact {
        #[arg(value_name = "REVISION", add = ArgValueCompleter::new(crate::commands::hints::patch_ids_completer))]
        revision_id: Rev,
        #[arg(value_name = "COMMENT", add = ArgValueCompleter::new(crate::commands::hints::patch_ids_completer))]
        comment_id: Rev,
    },
    React {
        #[arg(value_name = "REVISION", add = ArgValueCompleter::new(crate::commands::hints::patch_ids_completer))]
        revision_id: Rev,
        #[arg(value_name = "COMMENT", add = ArgValueCompleter::new(crate::commands::hints::patch_ids_completer))]
        comment_id: Rev,

        #[arg(long)]
        reaction: radicle::cob::Reaction,

        #[arg(long)]
        undo: bool,
    },
}

impl Command {
    pub(super) fn is_announce(&self) -> bool {
        match self {
            Self::Update { .. }
            | Self::Archive { .. }
            | Self::Ready { .. }
            | Self::Delete { .. }
            | Self::Comment {
                subcommand: CommentSubcommand::Edit { .. },
                ..
            }
            | Self::Comment {
                subcommand: CommentSubcommand::Redact { .. },
                ..
            }
            | Self::Comment {
                subcommand: CommentSubcommand::React { .. },
                ..
            }
            | Self::Review { .. }
            | Self::Resolve { .. }
            | Self::Assign { .. }
            | Self::Label { .. }
            | Self::Edit { .. }
            | Self::Redact { .. }
            | Self::React { .. }
            | Self::Set { .. } => true,
            Self::Show { .. }
            | Self::Diff { .. }
            | Self::Checkout { .. }
            | Self::List { .. }
            | Self::Cache { .. } => false,
        }
    }
}

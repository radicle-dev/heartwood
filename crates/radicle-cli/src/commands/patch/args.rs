use clap::{Parser, Subcommand};

use radicle::cob::Label;
use radicle::git;
use radicle::git::fmt::RefString;
use radicle::patch::Status;
use radicle::patch::Verdict;
use radicle::prelude::Did;
use radicle::prelude::RepoId;

use crate::commands::patch::checkout;
use crate::commands::patch::review;

use crate::git::Rev;
use crate::terminal::patch::Message;

const ABOUT: &str = "Manage patches";

/// The Radicle *patch* command is used for managing changesets inside of Radicle
/// repositories.
///
/// Though many actions can be performed using *rad patch*, certain patch-related
/// actions use *git* directly. For example, opening a patch is typically
/// done using *git push*, while merging a patch is done with a combination of
/// *git merge* and *git push*.
///
/// To make this possible, Radicle ships with a helper program, *git-remote-rad*
/// which is invoked by *git* on push and fetch to and from Radicle remotes.
///
/// With no arguments, *rad patch* defaults to the *rad patch list* command,
/// showing the list of open patches for the current repository.
///
/// ## Opening a patch
///
/// To open a patch, we start by making changes to our working copy, typically on
/// a feature branch. For example:
///
///     $ git checkout -b fix/option-parsing
///
///       ... edit some files ...
///
///     $ git commit -a -m "Fix option parsing"
///
/// Once our changes are ready to be proposed as a patch, we push them via *git*
/// to a special reference on the *rad* remote, that is used for opening patches
/// (*refs/patches*):
///
///     $ git push rad HEAD:refs/patches
///     ✓ Patch 90c77f2c33b7e472e058de4a586156f8a7fec7d6 opened
///     ...
///
/// Radicle will then open your editor, where you can edit the patch title and
/// description. Make sure either *EDITOR* or *VISUAL* is set in your environment
/// (See *environ(7)* for more details). Once you're done, simply save and exit your
/// editor. If successful, the patch is opened and its identifier is printed out.
/// You can then display the patch metadata using the *show* sub-command:
///
///     $ rad patch show 90c77f2
///
/// Note that you don't have to use the full patch identifier. An unambiguous
/// prefix of it also works.
///
/// Radicle can create a patch from any Git commit. Simply substitute *HEAD* with
/// the branch name or commit hash you wish to propose a patch for. For example:
///
///     $ git push rad d39fe32387496876fae6446daf3762aacf69d83b:refs/patches
///
/// After the patch is opened, you may notice that Radicle has set your branch
/// upstream to something like *rad/patches/90c77f2c33b7e472e058de4a586156f8a7fec7d6*.
/// This means your branch is now associated with the newly opened patch, and any
/// push from this branch will result in the patch being updated. See the next
/// section on updating a patch for more information.
///
/// Note that it's also possible to create a *draft* patch, by using the *-o
/// patch.draft* push option when opening a patch. See the *ready* patch
/// sub-command for more options.
///
/// ### Options
///
/// When opening a patch, various options can be specified using git push options.
/// This is done via the *-o* or *--push-option* flag. For example, *-o patch.draft*.
/// The full list of options follows:
///
/// *sync*, *no-sync*::
///   Whether or not to sync with the network after the patch is opened. Defaults
///   to _sync_.
///
/// *sync.debug*::
///   Show debug information about the syncing process.
///
/// *patch.draft*::
///   Open the patch as a _draft_. Turned off by default.
///
/// *patch.branch[=<name>]*::
///   Create a branch when opening a patch. Turned off by default.
///   If a custom *name* is provided, this name is used. Otherwise
///   the branch name defaults to *patches/<patch id>*.
///
/// *patch.message*=_<message>_::
///   To prevent the editor from opening, you can specify the patch message via this
///   option. Multiple *patch.message* options are concatenated with a blank line
///   in between.
///
/// *patch.base*=_<oid>_::
///   The base commit onto which this patch should be merged. By default, this is
///   your "master" branch. When building stacked patches, it may be useful to
///   set this to the head of a previous patch.
///
/// For more information on push options, see *git-push(1)*.
///
/// ## Updating a patch
///
/// To update a patch, we simply make our changes locally and push:
///
///     $ git commit --amend
///     $ git push --force
///     ✓ Patch 90c77f2 updated to revision d0018fcc21d87c91a1ff9155aed6b4e57535566b
///     ...
///
/// Note that this will only work if the current branch upstream is set correctly.
/// This happens automatically when a patch is opened from a branch without an
/// upstream set. In the above example, we used the *--force* option, since the
/// commit was amended. This is common practice when a patch has been reworked
/// after receiving a review.
///
/// If the branch upstream is not set to the patch reference, ie. *rad/patches/<id>*,
/// you can do so using `rad patch set <id>`.
///
/// As with opening a patch, you will be asked to enter a reason for updating the
/// patch, via your editor. Simply save and exit when you're done; or leave it
/// blank to skip this step.
///
/// It's also possible to change the patch _base_ during an update. Simply use the
/// *patch.base* push option as described in _Opening a patch_.
///
/// ## Checking out a patch
///
/// When working with patches opened by peers, it's often useful to be able to
/// checkout the code in its own branch. With a patch checkout, you can browse the
/// code, run tests and even propose your own update to the patch. The *checkout*
/// sub-command is used to that effect:
///
///     $ rad patch checkout 90c77f2
///
/// Radicle will create a new branch if necessary and checkout the patch head. From
/// there, you can *git-push* to publish a patch update, or simply browse the code.
///
/// ## Merging a patch
///
/// Once a patch is ready to merge, the repository maintainer simply has to use the
/// *git-merge(1)* command from the "master" branch and push via *git*. For
/// example, if some patch *26e3e56* is ready to merge, the steps would be:
///
///     $ rad patch checkout 26e3e56
///     ✓ Switched to branch patch/26e3e56
///     $ git checkout master
///     $ git merge patch/26e3e56
///     $ git push rad
///     ✓ Patch 26e3e563ddc7df8dd0c9f81274c0b3cb1b764568 merged
///     To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
///        f2de534..d6399c7  master -> master
///
/// In the above, we created a checkout for the patch, and merged that branch into
/// our master branch. Then we pushed to our *rad* remote.
///
/// ## Listing patches
///
/// To list patches, run *rad patch*. By default, this will only show open patches.
/// To list all patches, including ones that have been merged or archived, add the
/// *--all* option.
#[derive(Debug, Parser)]
#[command(about = ABOUT, disable_version_flag = true)]
pub struct Args {
    #[command(subcommand)]
    pub(super) command: Option<Command>,

    /// Quiet output
    #[arg(short, long, global = true)]
    pub(super) quiet: bool,

    /// Announce changes made to the network
    #[arg(long, global = true, conflicts_with = "no_announce")]
    announce: bool,

    /// Do not announce changes made to the network
    #[arg(long, global = true, conflicts_with = "announce")]
    no_announce: bool,

    /// Operate on the given repository [default: cwd]
    #[arg(long, global = true, value_name = "RID")]
    pub(super) repo: Option<RepoId>,

    /// Verbose output
    #[arg(long, short, global = true)]
    pub(super) verbose: bool,

    /// Arguments for the empty subcommand.
    /// Will fall back to [`Command::List`].
    #[clap(flatten)]
    pub(super) empty: EmptyArgs,
}

impl Args {
    pub(super) fn should_announce(&self) -> bool {
        self.announce || !self.no_announce
    }
}

/// Commands to create, view, and edit Radicle patches
#[derive(Subcommand, Debug)]
pub(super) enum Command {
    /// List patches in the current repository. The default is *--open*.
    #[command(alias = "l")]
    List(ListArgs),

    /// Show a specific patch
    #[command(alias = "s")]
    Show {
        /// ID of the patch
        #[arg(value_name = "PATCH_ID")]
        id: Rev,

        /// Show the diff of the changes in the patch
        #[arg(long, short)]
        patch: bool,

        /// Verbose output
        #[arg(long, short)]
        verbose: bool,
    },

    /// Show the diff of a specific patch
    ///
    /// The `git diff` of the revision's base and head will be shown
    Diff {
        /// ID of the patch
        #[arg(value_name = "PATCH_ID")]
        id: Rev,

        /// The revision to diff
        ///
        /// If not specified, the latest revision of the original author
        /// will be used
        #[arg(long, short)]
        revision: Option<Rev>,
    },

    /// Mark a patch as archived
    #[command(alias = "a")]
    Archive {
        /// ID of the patch
        #[arg(value_name = "PATCH_ID")]
        id: Rev,

        /// Unarchive a patch
        ///
        /// The patch will be marked as open
        #[arg(long)]
        undo: bool,
    },

    /// Update the metadata of a patch
    #[command(alias = "u")]
    Update {
        /// ID of the patch
        #[arg(value_name = "PATCH_ID")]
        id: Rev,

        /// Provide a Git revision as the base commit
        #[arg(long, short, value_name = "REVSPEC")]
        base: Option<Rev>,

        /// Change the message of the original revision of the patch
        #[clap(flatten)]
        message: MessageArgs,
    },

    /// Checkout a Git branch pointing to the head of a patch revision
    ///
    /// If no revision is specified, the latest revision of the original author
    /// is chosen
    #[command(alias = "c")]
    Checkout {
        /// ID of the patch
        #[arg(value_name = "PATCH_ID")]
        id: Rev,

        /// Checkout the given revision of the patch
        #[arg(long)]
        revision: Option<Rev>,

        #[clap(flatten)]
        opts: CheckoutArgs,
    },

    /// Create a review of a patch revision
    Review {
        /// ID of the patch
        #[arg(value_name = "PATCH_ID")]
        id: Rev,

        /// The particular revision to review
        ///
        /// If none is specified, the initial revision will be reviewed
        #[arg(long, short)]
        revision: Option<Rev>,

        #[clap(flatten)]
        options: ReviewArgs,
    },

    /// Mark a comment of a review as resolved or unresolved
    Resolve {
        /// ID of the patch
        #[arg(value_name = "PATCH_ID")]
        id: Rev,

        /// The review id which the comment is under
        #[arg(long, value_name = "REVIEW_ID")]
        review: Rev,

        /// The comment to (un)resolve
        #[arg(long, value_name = "COMMENT_ID")]
        comment: Rev,

        /// Unresolve the comment
        #[arg(long)]
        unresolve: bool,
    },

    /// Delete a patch
    ///
    /// This will delete any patch data associated with this user. Note that
    /// other user's data will remain, meaning the patch will remain until all
    /// other data is also deleted.
    #[command(alias = "d")]
    Delete {
        /// ID of the patch
        #[arg(value_name = "PATCH_ID")]
        id: Rev,
    },

    /// Redact a patch revision
    #[command(alias = "r")]
    Redact {
        /// ID of the patch revision
        #[arg(value_name = "REVISION_ID")]
        id: Rev,
    },

    /// React to a patch or patch revision
    React {
        /// ID of the patch or patch revision
        #[arg(value_name = "PATCH_ID|REVISION_ID")]
        id: Rev,

        /// The reaction being used
        #[arg(long, value_name = "CHAR")]
        emoji: radicle::cob::Reaction,

        /// Remove the reaction
        #[arg(long)]
        undo: bool,
    },

    /// Add or remove assignees to/from a patch
    Assign {
        /// ID of the patch
        #[arg(value_name = "PATCH_ID")]
        id: Rev,

        #[clap(flatten)]
        args: AssignArgs,
    },

    /// Add or remove labels to/from a patch
    Label {
        /// ID of the patch
        #[arg(value_name = "PATCH_ID")]
        id: Rev,

        #[clap(flatten)]
        args: LabelArgs,
    },

    /// If the patch is marked as a draft, then mark it as open
    #[command(alias = "y")]
    Ready {
        /// ID of the patch
        #[arg(value_name = "PATCH_ID")]
        id: Rev,

        /// Convert a patch back to a draft
        #[arg(long)]
        undo: bool,
    },

    #[command(alias = "e")]
    Edit {
        /// ID of the patch
        #[arg(value_name = "PATCH_ID")]
        id: Rev,

        /// ID of the patch revision
        #[arg(long, value_name = "REVISION_ID")]
        revision: Option<Rev>,

        #[clap(flatten)]
        message: MessageArgs,
    },

    /// Set an upstream branch for a patch
    Set {
        /// ID of the patch
        #[arg(value_name = "PATCH_ID")]
        id: Rev,

        /// Provide the git remote to use as the upstream
        #[arg(long, value_name = "REF", value_parser = parse_refstr)]
        remote: Option<RefString>,
    },

    /// Comment on, reply to, edit, or react to a comment
    Comment(CommentArgs),

    /// Re-cache the patches
    Cache {
        /// ID of the patch
        #[arg(value_name = "PATCH_ID")]
        id: Option<Rev>,

        /// Re-cache all patches in storage, as opposed to the current repository
        #[arg(long)]
        storage: bool,
    },
}

impl Command {
    pub(super) fn should_announce(&self) -> bool {
        match self {
            Self::Update { .. }
            | Self::Archive { .. }
            | Self::Ready { .. }
            | Self::Delete { .. }
            | Self::Comment { .. }
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

#[derive(Parser, Debug)]
pub(super) struct CommentArgs {
    /// ID of the revision to comment on
    #[arg(value_name = "REVISION_ID")]
    revision: Rev,

    #[clap(flatten)]
    message: MessageArgs,

    /// The comment to edit
    ///
    /// Use `--message` to edit with the provided message
    #[arg(
        long,
        value_name = "COMMENT_ID",
        conflicts_with = "react",
        conflicts_with = "redact"
    )]
    edit: Option<Rev>,

    /// The comment to react to
    ///
    /// Use `--emoji` for the character to react with
    ///
    /// Use `--undo` with `--emoji` to remove the reaction
    #[arg(
        long,
        value_name = "COMMENT_ID",
        conflicts_with = "edit",
        conflicts_with = "redact",
        requires = "emoji",
        group = "reaction"
    )]
    react: Option<Rev>,

    /// The comment to redact
    #[arg(
        long,
        value_name = "COMMENT_ID",
        conflicts_with = "react",
        conflicts_with = "edit"
    )]
    redact: Option<Rev>,

    /// The emoji to react with
    ///
    /// Requires using `--react <COMMENT_ID>`
    #[arg(long, requires = "reaction")]
    emoji: Option<radicle::cob::Reaction>,

    /// The comment to reply to
    #[arg(long, value_name = "COMMENT_ID")]
    reply_to: Option<Rev>,

    /// Remove the reaction
    ///
    /// Requires using `--react <COMMENT_ID> --emoji <EMOJI>`
    #[arg(long, requires = "reaction")]
    undo: bool,
}

#[derive(Debug)]
pub(super) enum CommentAction {
    Comment {
        revision: Rev,
        message: Message,
        reply_to: Option<Rev>,
    },
    Edit {
        revision: Rev,
        comment: Rev,
        message: Message,
    },
    Redact {
        revision: Rev,
        comment: Rev,
    },
    React {
        revision: Rev,
        comment: Rev,
        emoji: radicle::cob::Reaction,
        undo: bool,
    },
}

impl From<CommentArgs> for CommentAction {
    fn from(
        CommentArgs {
            revision,
            message,
            edit,
            react,
            redact,
            reply_to,
            emoji,
            undo,
        }: CommentArgs,
    ) -> Self {
        match (edit, react, redact) {
            (Some(edit), None, None) => CommentAction::Edit {
                revision,
                comment: edit,
                message: Message::from(message),
            },
            (None, Some(react), None) => CommentAction::React {
                revision,
                comment: react,
                emoji: emoji.expect("emoji must be Some when react is Some"),
                undo,
            },
            (None, None, Some(redact)) => CommentAction::Redact {
                revision,
                comment: redact,
            },
            (None, None, None) => Self::Comment {
                revision,
                message: Message::from(message),
                reply_to,
            },
            _ => unreachable!("`--edit`, `--react`, and `--redact` cannot be used together"),
        }
    }
}

#[derive(Parser, Debug, Default)]
pub(super) struct EmptyArgs {
    #[arg(long, hide = true, value_name = "DID", num_args = 1.., action = clap::ArgAction::Append)]
    authors: Vec<Did>,

    #[arg(long, hide = true)]
    authored: bool,

    #[clap(flatten)]
    state: EmptyStateArgs,
}

#[derive(Parser, Debug, Default)]
#[group(multiple = false)]
pub(super) struct EmptyStateArgs {
    #[arg(long, hide = true)]
    all: bool,

    #[arg(long, hide = true)]
    draft: bool,

    #[arg(long, hide = true)]
    open: bool,

    #[arg(long, hide = true)]
    merged: bool,

    #[arg(long, hide = true)]
    archived: bool,
}

#[derive(Parser, Debug, Default)]
pub(super) struct ListArgs {
    /// Show only patched where the given user is an author (may be specified
    /// multiple times)
    #[arg(
        long = "author",
        value_name = "DID",
        num_args = 1..,
        action = clap::ArgAction::Append,
    )]
    pub(super) authors: Vec<Did>,

    /// Show only patches that you have authored
    #[arg(long)]
    pub(super) authored: bool,

    #[clap(flatten)]
    pub(super) state: ListStateArgs,
}

impl From<EmptyArgs> for ListArgs {
    fn from(args: EmptyArgs) -> Self {
        Self {
            authors: args.authors,
            authored: args.authored,
            state: ListStateArgs::from(args.state),
        }
    }
}

#[derive(Parser, Debug, Default)]
#[group(multiple = false)]
pub(crate) struct ListStateArgs {
    /// Show all patches, including draft, merged, and archived patches
    #[arg(long)]
    pub(crate) all: bool,

    /// Show only draft patches
    #[arg(long)]
    pub(crate) draft: bool,

    /// Show only open patches (default)
    #[arg(long)]
    pub(crate) open: bool,

    /// Show only merged patches
    #[arg(long)]
    pub(crate) merged: bool,

    /// Show only archived patches
    #[arg(long)]
    pub(crate) archived: bool,
}

impl From<EmptyStateArgs> for ListStateArgs {
    fn from(args: EmptyStateArgs) -> Self {
        Self {
            all: args.all,
            draft: args.draft,
            open: args.open,
            merged: args.merged,
            archived: args.archived,
        }
    }
}

impl From<&ListStateArgs> for Option<&Status> {
    fn from(args: &ListStateArgs) -> Self {
        match (args.all, args.draft, args.open, args.merged, args.archived) {
            (true, false, false, false, false) => None,
            (false, true, false, false, false) => Some(&Status::Draft),
            (false, false, true, false, false) | (false, false, false, false, false) => {
                Some(&Status::Open)
            }
            (false, false, false, true, false) => Some(&Status::Merged),
            (false, false, false, false, true) => Some(&Status::Archived),
            _ => unreachable!(),
        }
    }
}

#[derive(Debug, Parser)]
pub(super) struct ReviewArgs {
    /// Review by patch hunks
    ///
    /// This operation is obsolete
    #[arg(long, short, group = "by-hunk", conflicts_with = "delete")]
    patch: bool,

    /// Generate diffs with <N> lines of context
    ///
    /// This operation is obsolete
    #[arg(
        long,
        short = 'U',
        value_name = "N",
        requires = "by-hunk",
        default_value_t = 3
    )]
    unified: usize,

    /// Only review a specific hunk
    ///
    /// This operation is obsolete
    #[arg(long, value_name = "INDEX", requires = "by-hunk")]
    hunk: Option<usize>,

    /// Accept a patch revision
    #[arg(long, conflicts_with = "reject", conflicts_with = "delete")]
    accept: bool,

    /// Reject a patch revision
    #[arg(long, conflicts_with = "delete")]
    reject: bool,

    /// Delete a review draft
    ///
    /// This operation is obsolete
    #[arg(long, short)]
    delete: bool,

    #[clap(flatten)]
    message_args: MessageArgs,
}

impl ReviewArgs {
    fn as_operation(&self) -> review::Operation {
        let Self {
            patch,
            accept,
            reject,
            delete,
            ..
        } = self;

        if *patch {
            let verdict = if *accept {
                Some(Verdict::Accept)
            } else if *reject {
                Some(Verdict::Reject)
            } else {
                None
            };
            return review::Operation::Review(review::ReviewOptions {
                by_hunk: true,
                unified: self.unified,
                hunk: self.hunk,
                verdict,
            });
        }

        if *delete {
            return review::Operation::Delete;
        }

        if *accept {
            return review::Operation::Review(review::ReviewOptions {
                by_hunk: false,
                unified: 3,
                hunk: None,
                verdict: Some(Verdict::Accept),
            });
        }

        if *reject {
            return review::Operation::Review(review::ReviewOptions {
                by_hunk: false,
                unified: 3,
                hunk: None,
                verdict: Some(Verdict::Reject),
            });
        }

        panic!("expected one of `--patch`, `--delete`, `--accept`, or `--reject`");
    }
}

impl From<ReviewArgs> for review::Options {
    fn from(args: ReviewArgs) -> Self {
        let op = args.as_operation();
        Self {
            message: Message::from(args.message_args),
            op,
        }
    }
}

#[derive(Debug, clap::Args)]
#[group(required = false, multiple = false)]
pub(super) struct MessageArgs {
    /// Provide a message (default: prompt)
    ///
    /// This can be specified multiple times. This will result in newlines
    /// between the specified messages.
    #[clap(
        long,
        short,
        value_name = "MESSAGE",
        num_args = 1..,
        action = clap::ArgAction::Append
    )]
    pub(super) message: Option<Vec<String>>,

    /// Do not provide a message
    #[arg(long, conflicts_with = "message")]
    pub(super) no_message: bool,
}

impl From<MessageArgs> for Message {
    fn from(
        MessageArgs {
            message,
            no_message,
        }: MessageArgs,
    ) -> Self {
        if no_message {
            assert!(message.is_none());
            return Self::Blank;
        }

        match message {
            Some(messages) => messages.into_iter().fold(Self::Blank, |mut result, m| {
                result.append(&m);
                result
            }),
            None => Self::Edit,
        }
    }
}

#[derive(Debug, clap::Args)]
pub(super) struct CheckoutArgs {
    /// Provide a name for the branch to checkout
    #[arg(long, value_name = "BRANCH", value_parser = parse_refstr)]
    pub(super) name: Option<RefString>,

    /// Provide the git remote to use as the upstream
    #[arg(long, value_parser = parse_refstr)]
    pub(super) remote: Option<RefString>,

    /// Checkout the head of the revision, even if the branch already exists
    #[arg(long, short)]
    pub(super) force: bool,
}

impl From<CheckoutArgs> for checkout::Options {
    fn from(value: CheckoutArgs) -> Self {
        Self {
            name: value.name,
            remote: value.remote,
            force: value.force,
        }
    }
}

#[derive(Parser, Debug)]
#[group(required = true)]
pub(super) struct AssignArgs {
    /// Add an assignee to the patch (may be specified multiple times).
    ///
    /// Note: `--add` takes precedence over `--delete`
    #[arg(long, short, value_name = "DID", num_args = 1.., action = clap::ArgAction::Append)]
    pub(super) add: Vec<Did>,

    /// Remove an assignee from the patch (may be specified multiple times).
    ///
    /// Note: `--add` takes precedence over `--delete`
    #[clap(long, short, value_name = "DID", num_args = 1.., action = clap::ArgAction::Append)]
    pub(super) delete: Vec<Did>,
}

#[derive(Parser, Debug)]
#[group(required = true)]
pub(super) struct LabelArgs {
    /// Add a label to the patch (may be specified multiple times).
    ///
    /// Note: `--add` takes precedence over `--delete`
    #[arg(long, short, value_name = "LABEL", num_args = 1.., action = clap::ArgAction::Append)]
    pub(super) add: Vec<Label>,

    /// Remove a label from the patch (may be specified multiple times).
    ///
    /// Note: `--add` takes precedence over `--delete`
    #[clap(long, short, value_name = "LABEL", num_args = 1.., action = clap::ArgAction::Append)]
    pub(super) delete: Vec<Label>,
}

fn parse_refstr(refstr: &str) -> Result<RefString, git::fmt::Error> {
    RefString::try_from(refstr)
}

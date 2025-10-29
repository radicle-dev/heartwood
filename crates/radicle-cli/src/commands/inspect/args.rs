use clap::Parser;

const ABOUT: &str = "Inspect a Radicle repository";
const LONG_ABOUT: &str = r#"Inspects the given path or RID. If neither is specified,
the current repository is inspected.
"#;

#[derive(Debug, Parser)]
#[group(multiple = false)]
pub(super) struct TargetArgs {
    /// Inspect the repository's delegates
    #[arg(long)]
    pub(super) delegates: bool,

    /// Show the history of the repository identity document
    #[arg(long)]
    pub(super) history: bool,

    /// Inspect the identity document
    #[arg(long)]
    pub(super) identity: bool,

    /// Inspect the repository's identity payload
    #[arg(long)]
    pub(super) payload: bool,

    /// Inspect the repository's seeding policy
    #[arg(long)]
    pub(super) policy: bool,

    /// Inspect the repository's refs on the local device
    #[arg(long)]
    pub(super) refs: bool,

    /// Return the repository identifier (RID)
    #[arg(long)]
    pub(super) rid: bool,

    /// Inspect the values of `rad/sigrefs` for all remotes of this repository
    #[arg(long)]
    pub(super) sigrefs: bool,

    /// Inspect the repository's visibility
    #[arg(long)]
    pub(super) visibility: bool,
}

pub(super) enum Target {
    Delegates,
    History,
    Identity,
    Payload,
    Policy,
    Refs,
    RepoId,
    Sigrefs,
    Visibility,
}

impl From<TargetArgs> for Target {
    fn from(args: TargetArgs) -> Self {
        match (
            args.delegates,
            args.history,
            args.identity,
            args.payload,
            args.policy,
            args.refs,
            args.rid,
            args.sigrefs,
            args.visibility,
        ) {
            (true, false, false, false, false, false, false, false, false) => Target::Delegates,
            (false, true, false, false, false, false, false, false, false) => Target::History,
            (false, false, true, false, false, false, false, false, false) => Target::Identity,
            (false, false, false, true, false, false, false, false, false) => Target::Payload,
            (false, false, false, false, true, false, false, false, false) => Target::Policy,
            (false, false, false, false, false, true, false, false, false) => Target::Refs,
            (false, false, false, false, false, false, true, false, false)
            | (false, false, false, false, false, false, false, false, false) => Target::RepoId,
            (false, false, false, false, false, false, false, true, false) => Target::Sigrefs,
            (false, false, false, false, false, false, false, false, true) => Target::Visibility,
            _ => unreachable!(),
        }
    }
}

#[derive(Debug, Parser)]
#[command(about = ABOUT, long_about = LONG_ABOUT, disable_version_flag = true)]
pub struct Args {
    /// Repository, by RID or by path
    #[arg(value_name = "RID|PATH")]
    pub(super) repo: Option<String>,

    #[clap(flatten)]
    pub(super) target: TargetArgs,
}

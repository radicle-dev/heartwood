use clap::Parser;

use radicle::prelude::RepoId;

const ABOUT: &str = "Remove all remotes from a repository";

const LONG_ABOUT: &str = r#"
Removes all remotes from a repository, as long as they are not the
local operator or a delegate of the repository.

Note that remotes will still be fetched as long as they are
followed and/or the follow scope is "all".
"#;

#[derive(Debug, Parser)]
#[command(about = ABOUT, long_about = LONG_ABOUT, disable_version_flag = true)]
pub struct Args {
    /// Operate on the given repository
    #[arg(value_name = "RID")]
    pub(super) repo: RepoId,

    /// Do not ask for confirmation before removal
    #[arg(long)]
    pub(super) no_confirm: bool,
}

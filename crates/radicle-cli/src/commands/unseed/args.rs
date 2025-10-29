use clap::Parser;
use radicle::prelude::RepoId;

const ABOUT: &str = "Remove repository seeding policies";

const LONG_ABOUT: &str = r#"
The `unseed` command removes the seeding policy, if found,
for the given repositories."#;

#[derive(Debug, Parser)]
#[command(about = ABOUT, long_about = LONG_ABOUT, disable_version_flag = true)]
pub struct Args {
    /// ID of the repository to remove the seeding policy for (may be repeated)
    #[arg(value_name = "RID", required = true, action = clap::ArgAction::Append)]
    pub rids: Vec<RepoId>,
}

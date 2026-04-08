use clap::Parser;
use radicle::prelude::{Did, RepoId};

const ABOUT: &str = "Checkout a repository into the local directory";
const LONG_ABOUT: &str = r#"
Creates a working copy from a repository in local storage.
"#;

#[derive(Debug, Parser)]
#[command(about = ABOUT, long_about = LONG_ABOUT, disable_version_flag = true)]
pub struct Args {
    /// Repository ID of the repository to checkout
    #[arg(value_name = "RID")]
    pub(super) repo: RepoId,

    /// The DID of the remote peer to checkout
    #[arg(long, value_name = "DID")]
    pub(super) remote: Option<Did>,

    /// Don't ask for confirmation during checkout
    // TODO(erikli): This is obsolete and should be removed
    #[arg(long)]
    no_confirm: bool,
}

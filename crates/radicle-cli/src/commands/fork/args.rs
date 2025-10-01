pub(crate) const ABOUT: &str = "Create a fork of a repository";

#[derive(Debug, clap::Parser)]
#[command(about = ABOUT, disable_version_flag = true)]
pub struct Args {
    /// The Repository ID of the repository to fork
    #[arg(value_name = "RID")]
    pub(super) rid: Option<radicle::identity::RepoId>,
}

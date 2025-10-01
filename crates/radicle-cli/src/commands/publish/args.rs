pub(crate) const ABOUT: &str = "Publish a repository to the network";

const LONG_ABOUT: &str = r#"
Publishing a private repository makes it public and discoverable
on the network.

By default, this command will publish the current repository.
If an `<rid>` is specified, that repository will be published instead.

Note that this command can only be run for repositories with a
single delegate. The delegate must be the currently authenticated
user. For repositories with more than one delegate, the `rad id`
command must be used."#;

#[derive(Debug, clap::Parser)]
#[command(about = ABOUT, long_about = LONG_ABOUT, disable_version_flag = true)]
pub struct Args {
    /// The Repository ID of the repository to publish
    #[arg(value_name = "RID")]
    pub(super) rid: Option<radicle::identity::RepoId>,
}

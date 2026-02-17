use std::io;

use clap::Parser;

use radicle::identity::RepoId;
use radicle_term::Interactive;

const ABOUT: &str = "Publish a repository to the network";

const LONG_ABOUT: &str = r#"
Publishing a private repository makes it public and discoverable
on the network.

By default, this command will publish the current repository.
If an `<rid>` is specified, that repository will be published instead.

Note that this command can only be run for repositories with a
single delegate. The delegate must be the currently authenticated
user. For repositories with more than one delegate, the `rad id`
command must be used."#;

#[derive(Debug, Parser)]
#[command(about = ABOUT, long_about = LONG_ABOUT, disable_version_flag = true)]
pub struct Args {
    /// The Repository ID of the repository to publish
    ///
    /// [example values: rad:z3Tr6bC7ctEg2EHmLvknUr29mEDLH, z3Tr6bC7ctEg2EHmLvknUr29mEDLH]
    #[arg(value_name = "RID")]
    pub(super) rid: Option<RepoId>,

    /// Do not ask for confirmation to publish the repository.
    #[arg(long)]
    #[arg(global = true)]
    no_confirm: bool,
}

impl Args {
    pub(super) fn interactive(&self) -> Interactive {
        if self.no_confirm {
            Interactive::No
        } else {
            Interactive::new(io::stdout())
        }
    }
}

#[cfg(test)]
mod test {
    use super::Args;
    use clap::error::ErrorKind;
    use clap::Parser;

    #[test]
    fn should_parse_rid_non_urn() {
        let args = Args::try_parse_from(["publish", "z3Tr6bC7ctEg2EHmLvknUr29mEDLH"]);
        assert!(args.is_ok())
    }

    #[test]
    fn should_parse_rid_urn() {
        let args = Args::try_parse_from(["publish", "rad:z3Tr6bC7ctEg2EHmLvknUr29mEDLH"]);
        assert!(args.is_ok())
    }

    #[test]
    fn should_not_parse_rid_url() {
        let err =
            Args::try_parse_from(["publish", "rad://z3Tr6bC7ctEg2EHmLvknUr29mEDLH"]).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::ValueValidation);
    }
}

use radicle::identity::RepoId;

const ABOUT: &str = "Create a fork of a repository";

#[derive(Debug, clap::Parser)]
#[command(about = ABOUT, disable_version_flag = true)]
pub struct Args {
    /// The Repository ID of the repository to fork
    ///
    /// [example values: rad:z3Tr6bC7ctEg2EHmLvknUr29mEDLH, z3Tr6bC7ctEg2EHmLvknUr29mEDLH]
    #[arg(value_name = "RID")]
    pub(super) rid: Option<RepoId>,
}

#[cfg(test)]
mod test {
    use super::Args;
    use clap::error::ErrorKind;
    use clap::Parser;

    #[test]
    fn should_parse_rid_non_urn() {
        let args = Args::try_parse_from(["fork", "z3Tr6bC7ctEg2EHmLvknUr29mEDLH"]);
        assert!(args.is_ok())
    }

    #[test]
    fn should_parse_rid_urn() {
        let args = Args::try_parse_from(["fork", "rad:z3Tr6bC7ctEg2EHmLvknUr29mEDLH"]);
        assert!(args.is_ok())
    }

    #[test]
    fn should_not_parse_rid_url() {
        let err =
            Args::try_parse_from(["fork", "rad://z3Tr6bC7ctEg2EHmLvknUr29mEDLH"]).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::ValueValidation);
    }
}

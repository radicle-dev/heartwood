use clap::Parser;

use crate::terminal::args::BlockTarget;

const ABOUT: &str = "Block repositories or nodes from being seeded or followed";

#[derive(Parser, Debug)]
#[command(about = ABOUT, disable_version_flag = true)]
pub struct Args {
    /// A Repository ID or Node ID to block from seeding or following (respectively)
    ///
    /// [example values: rad:z3Tr6bC7ctEg2EHmLvknUr29mEDLH, z6MkiswaKJ85vafhffCGBu2gdBsYoDAyHVBWRxL3j297fwS9]
    #[arg(value_name = "RID|NID")]
    pub(super) target: BlockTarget,
}

#[cfg(test)]
mod test {
    use clap::error::ErrorKind;
    use clap::Parser;

    use super::Args;

    #[test]
    fn should_parse_nid() {
        let args =
            Args::try_parse_from(["block", "z6MkiswaKJ85vafhffCGBu2gdBsYoDAyHVBWRxL3j297fwS9"]);
        assert!(args.is_ok())
    }

    #[test]
    fn should_parse_rid() {
        let args = Args::try_parse_from(["block", "rad:z3Tr6bC7ctEg2EHmLvknUr29mEDLH"]);
        assert!(args.is_ok())
    }

    #[test]
    fn should_not_parse() {
        let err = Args::try_parse_from(["block", "bee"]).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::ValueValidation);
    }
}

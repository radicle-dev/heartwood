use clap::Parser;

use crate::terminal::args::BlockTarget;

const ABOUT: &str = "Unblock repositories or nodes to allow them to be seeded or followed";

#[derive(Parser, Debug)]
#[command(about = ABOUT, disable_version_flag = true)]
pub struct Args {
    /// A Repository ID or Node ID to allow to be seeded or followed
    ///
    /// [example values: rad:z3Tr6bC7ctEg2EHmLvknUr29mEDLH, z6MkiswaKJ85vafhffCGBu2gdBsYoDAyHVBWRxL3j297fwS9]
    #[arg(value_name = "RID|NID")]
    pub(super) target: BlockTarget,
}

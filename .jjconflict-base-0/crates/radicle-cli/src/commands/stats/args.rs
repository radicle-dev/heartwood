use clap::Parser;

const ABOUT: &str = "Displays aggregated repository and node metrics";

#[derive(Debug, Parser)]
#[command(about = ABOUT, disable_version_flag = true)]
pub struct Args {}

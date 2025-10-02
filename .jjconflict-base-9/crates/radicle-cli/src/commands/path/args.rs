use clap::Parser;

const ABOUT: &str = "Display the Radicle home path";

#[derive(Parser, Debug)]
#[command(about = ABOUT, disable_version_flag = true)]
pub struct Args {}

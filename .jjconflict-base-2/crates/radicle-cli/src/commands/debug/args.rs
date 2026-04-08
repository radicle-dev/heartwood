use clap::Parser;

const ABOUT: &str = "Write out information to help debug your Radicle node remotely";

const LONG_ABOUT: &str = r#"
Run this if you are reporting a problem in Radicle. The output is
helpful for Radicle developers to debug your problem remotely. The
output is meant to not include any sensitive information, but
please check it, and then forward to the Radicle developers."#;

#[derive(Parser, Debug)]
#[command(about = ABOUT, long_about = LONG_ABOUT, disable_version_flag = true)]
pub struct Args {}

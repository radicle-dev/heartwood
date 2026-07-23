use clap::{Parser, Subcommand};
use radicle::node::Alias;

const ABOUT: &str = "Manage your local Radicle configuration";

const LONG_ABOUT: &str = r#"
If no argument is specified, prints the current Radicle configuration as JSON.
To initialize a new configuration file, use `rad config init`.
"#;

#[derive(Debug, Parser)]
#[command(about = ABOUT, long_about = LONG_ABOUT, disable_version_flag = true)]
pub struct Args {
    #[command(subcommand)]
    pub(crate) command: Option<Command>,
}

#[derive(Subcommand, Debug)]
#[group(multiple = false)]
pub(crate) enum Command {
    /// Show the current Radicle configuration as JSON (default)
    Show,
    /// Initialize a new config file
    Init {
        /// Alias to use for the new configuration
        #[arg(long)]
        alias: Alias,
    },
    /// Open the config in your editor
    Edit,
    /// Prints the JSON Schema of the Radicle configuration
    Schema,
}

use clap::{Parser, Subcommand};
use radicle::node::Alias;

const ABOUT: &str = "Manage your local Radicle configuration";

const LONG_ABOUT: &str = r#"
If no argument is specified, prints the current radicle configuration as JSON.
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
    /// Show the current radicle configuration as JSON (default)
    Show,
    /// Initialize a new config file
    Init {
        /// Alias to use for the new configuration
        #[arg(long)]
        alias: Alias,
    },
    /// Open the config in your editor
    Edit,
    /// Get a value from the current configuration
    Get {
        /// The JSON key path to the value you want to get
        key: String,
    },
    /// Prints the JSON Schema of the Radicle configuration
    Schema,
    /// Set a key to a value in the current configuration
    Set {
        /// The JSON key path to the value you want to set
        key: String,
        /// The JSON value used to set the field
        value: String,
    },
    /// Set a key in the current configuration to `null`
    Unset {
        /// The JSON key path to the value you want to unset
        key: String,
    },
    /// Push a value onto an array, which is identified by the key, in the
    /// current configuration
    Push {
        /// The JSON key path to the array you want to push to
        key: String,
        /// The JSON value being pushed onto the array
        value: String,
    },
    /// Remove a value from an array, which is identified by the key, in the
    /// current configuration
    ///
    /// All instances of the value in the array will be removed
    Remove {
        /// The JSON key path to the array you want to push to
        key: String,
        /// The JSON value being pushed onto the array
        value: String,
    },
}

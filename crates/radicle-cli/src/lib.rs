#![allow(clippy::collapsible_if)]
#![allow(clippy::or_fun_call)]
#![allow(clippy::too_many_arguments)]
pub mod commands;
pub mod git;
pub mod node;
pub mod pager;
pub mod project;
pub mod terminal;

mod warning;

extern crate radicle_localtime as localtime;

/// Returns an error saying that given command is obsolete and has been removed.
/// See also [`warning::obsolete`].
fn removed(command: &str) -> anyhow::Error {
    anyhow::anyhow!(
        "The command `{}` is obsolete and has been removed.",
        command
    )
}

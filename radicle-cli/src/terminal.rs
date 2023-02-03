pub mod args;
pub mod cob;
pub mod command;
pub mod format;
pub mod io;
pub mod patch;
pub mod spinner;
pub mod table;
pub mod textbox;

use std::ffi::OsString;
use std::process;

use dialoguer::console::style;
use radicle::profile::Profile;

pub use args::{Args, Error, Help};
pub use console::measure_text_width as text_width;
pub use dialoguer::Editor;
pub use io::*;
pub use spinner::{spinner, Spinner};
pub use table::Table;
pub use textbox::TextBox;

/// Context passed to all commands.
pub trait Context {
    /// Return the currently active profile, or an error if no profile is active.
    fn profile(&self) -> Result<Profile, anyhow::Error>;
}

impl Context for Profile {
    fn profile(&self) -> Result<Profile, anyhow::Error> {
        Ok(self.clone())
    }
}

impl<F> Context for F
where
    F: Fn() -> Result<Profile, anyhow::Error>,
{
    fn profile(&self) -> Result<Profile, anyhow::Error> {
        self()
    }
}

/// A command that can be run.
pub trait Command<A: Args, C: Context> {
    /// Run the command, given arguments and a context.
    fn run(self, args: A, context: C) -> anyhow::Result<()>;
}

impl<F, A: Args, C: Context> Command<A, C> for F
where
    F: FnOnce(A, C) -> anyhow::Result<()>,
{
    fn run(self, args: A, context: C) -> anyhow::Result<()> {
        self(args, context)
    }
}

pub fn run_command<A, C>(help: Help, action: &str, cmd: C) -> !
where
    A: Args,
    C: Command<A, fn() -> anyhow::Result<Profile>>,
{
    let args = std::env::args_os().into_iter().skip(1).collect();

    run_command_args(help, action, cmd, args)
}

pub fn run_command_args<A, C>(help: Help, action: &str, cmd: C, args: Vec<OsString>) -> !
where
    A: Args,
    C: Command<A, fn() -> anyhow::Result<Profile>>,
{
    use io as term;

    let options = match A::from_args(args) {
        Ok((opts, unparsed)) => {
            if let Err(err) = args::finish(unparsed) {
                term::error(err);
                process::exit(1);
            }
            opts
        }
        Err(err) => {
            match err.downcast_ref::<Error>() {
                Some(Error::Help) => {
                    term::help(help.name, help.version, help.description, help.usage);
                    process::exit(0);
                }
                Some(Error::Usage) => {
                    term::usage(help.name, help.usage);
                    process::exit(1);
                }
                _ => {}
            };
            eprintln!(
                "{} {} {} {}",
                style("==").red(),
                style("Error:").red(),
                style(format!("rad-{}:", help.name)).red(),
                style(&err).red()
            );

            if let Some(Error::WithHint { hint, .. }) = err.downcast_ref::<Error>() {
                eprintln!("{}", style(hint).yellow());
            }

            process::exit(1);
        }
    };

    match cmd.run(options, self::profile) {
        Ok(()) => process::exit(0),
        Err(err) => {
            term::fail(&format!("{action} failed"), &err);
            process::exit(1);
        }
    }
}

/// Get the default profile. Fails if there is no profile.
pub fn profile() -> Result<Profile, anyhow::Error> {
    let error = args::Error::WithHint {
        err: anyhow::anyhow!("Could not load radicle profile"),
        hint: "To setup your radicle profile, run `rad auth`.",
    };

    match Profile::load() {
        Ok(profile) => Ok(profile),
        Err(_) => Err(error.into()),
    }
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum Interactive {
    Yes,
    No,
}

impl Default for Interactive {
    fn default() -> Self {
        Interactive::No
    }
}

impl Interactive {
    pub fn yes(&self) -> bool {
        (*self).into()
    }

    pub fn no(&self) -> bool {
        !self.yes()
    }
}

impl From<Interactive> for bool {
    fn from(c: Interactive) -> Self {
        match c {
            Interactive::Yes => true,
            Interactive::No => false,
        }
    }
}

impl From<bool> for Interactive {
    fn from(b: bool) -> Self {
        if b {
            Interactive::Yes
        } else {
            Interactive::No
        }
    }
}

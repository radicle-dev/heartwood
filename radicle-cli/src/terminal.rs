pub mod args;
pub use args::{Args, Error, Help};
pub mod format;
pub mod io;
pub use io::signer;
pub mod comment;
pub mod highlight;
pub mod issue;
pub mod patch;

use std::ffi::OsString;
use std::process;

pub use radicle_term::*;

use radicle::profile::Profile;

use crate::terminal;

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

pub fn run_command<A, C>(help: Help, cmd: C) -> !
where
    A: Args,
    C: Command<A, fn() -> anyhow::Result<Profile>>,
{
    let args = std::env::args_os().skip(1).collect();

    run_command_args(help, cmd, args)
}

pub fn run_command_args<A, C>(help: Help, cmd: C, args: Vec<OsString>) -> !
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
            let hint = match err.downcast_ref::<Error>() {
                Some(Error::Help) => {
                    term::help(help.name, help.version, help.description, help.usage);
                    process::exit(0);
                }
                Some(Error::HelpManual { name }) => {
                    let Ok(status) = term::manual(name) else {
                        io::error(format!("rad {}: failed to load manual page", help.name));
                        process::exit(1);
                    };
                    process::exit(status.code().unwrap_or(0));
                }
                Some(Error::Usage) => {
                    term::usage(help.name, help.usage);
                    process::exit(1);
                }
                Some(Error::WithHint { hint, .. }) => Some(hint),
                None => None,
            };
            io::error(format!("rad {}: {err}", help.name));

            if let Some(hint) = hint {
                io::hint(hint);
            }
            process::exit(1);
        }
    };

    match cmd.run(options, self::profile) {
        Ok(()) => process::exit(0),
        Err(err) => {
            terminal::fail(help.name, &err);
            process::exit(1);
        }
    }
}

/// Get the default profile. Fails if there is no profile.
pub fn profile() -> Result<Profile, anyhow::Error> {
    match Profile::load() {
        Ok(profile) => Ok(profile),
        Err(e) => Err(args::Error::WithHint {
            err: anyhow::anyhow!("Could not load radicle profile: {e}"),
            hint: "To setup your radicle profile, run `rad auth`.",
        }
        .into()),
    }
}

pub fn fail(_name: &str, error: &anyhow::Error) {
    let err = error.to_string();
    let err = err.trim_end();

    for line in err.lines() {
        io::error(line);
    }

    if let Some(Error::WithHint { hint, .. }) = error.downcast_ref::<Error>() {
        io::hint(hint);
    }
}

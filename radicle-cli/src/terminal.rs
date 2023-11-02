pub mod args;
pub use args::{Args, Error, Help};
pub mod format;
pub mod io;
pub mod job;
pub use io::signer;
pub mod comment;
pub mod highlight;
pub mod issue;
pub mod json;
pub mod patch;
pub mod upload_pack;

use std::ffi::OsString;
use std::process;

pub use radicle_term::*;

use radicle::profile::{Home, Profile};

use crate::terminal;

/// Context passed to all commands.
pub trait Context {
    /// Return the currently active profile, or an error if no profile is active.
    fn profile(&self) -> Result<Profile, anyhow::Error>;
    /// Return the Radicle home.
    fn home(&self) -> Result<Home, std::io::Error>;
}

impl Context for Profile {
    fn profile(&self) -> Result<Profile, anyhow::Error> {
        Ok(self.clone())
    }

    fn home(&self) -> Result<Home, std::io::Error> {
        Ok(self.home.clone())
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
    C: Command<A, DefaultContext>,
{
    let args = std::env::args_os().skip(1).collect();

    run_command_args(help, cmd, args)
}

pub fn run_command_args<A, C>(help: Help, cmd: C, args: Vec<OsString>) -> !
where
    A: Args,
    C: Command<A, DefaultContext>,
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
                    help.print();
                    process::exit(0);
                }
                // Print the manual, or the regular help if there's an error.
                Some(Error::HelpManual { name }) => {
                    let Ok(status) = term::manual(name) else {
                        help.print();
                        process::exit(0);
                    };
                    if !status.success() {
                        help.print();
                        process::exit(0);
                    }
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

    match cmd.run(options, DefaultContext) {
        Ok(()) => process::exit(0),
        Err(err) => {
            terminal::fail(help.name, &err);
            process::exit(1);
        }
    }
}

/// Gets the default profile. Fails if there is no profile.
pub struct DefaultContext;

impl Context for DefaultContext {
    fn home(&self) -> Result<Home, std::io::Error> {
        radicle::profile::home()
    }

    fn profile(&self) -> Result<Profile, anyhow::Error> {
        match Profile::load() {
            Ok(profile) => Ok(profile),
            Err(radicle::profile::Error::NotFound(path)) => Err(args::Error::WithHint {
                err: anyhow::anyhow!("Radicle profile not found in '{}'.", path.display()),
                hint: "To setup your radicle profile, run `rad auth`.",
            }
            .into()),
            Err(radicle::profile::Error::Config(e)) => Err(e.into()),
            Err(e) => Err(anyhow::anyhow!("Could not load radicle profile: {e}")),
        }
    }
}

pub fn fail(_name: &str, error: &anyhow::Error) {
    let err = error.to_string();
    let err = err.trim_end();

    for line in err.lines() {
        io::error(line);
    }

    // Catch common node errors, and offer a hint.
    if let Some(e) = error.downcast_ref::<radicle::node::Error>() {
        if e.is_connection_err() {
            io::hint("to start your node, run `rad node start`.");
        }
    }
    if let Some(Error::WithHint { hint, .. }) = error.downcast_ref::<Error>() {
        io::hint(hint);
    }
}

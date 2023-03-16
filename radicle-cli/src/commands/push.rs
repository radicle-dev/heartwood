use std::ffi::OsString;
use std::path::Path;

use anyhow::anyhow;

use radicle::git;

use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};

pub const HELP: Help = Help {
    name: "push",
    description: "Publish a project to the network",
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad push [<option>...]

    By default, only the current branch is pushed.

Options

    --all               Push all branches (default: false)
    --help              Print help

Git options

    -f, --force           Force push
    -u, --set-upstream    Set upstream tracking branch

"#,
};

#[derive(Default, Debug)]
pub struct Options {
    pub verbose: bool,
    pub force: bool,
    pub all: bool,
    pub set_upstream: bool,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut verbose = false;
        let mut force = false;
        let mut all = false;
        let mut set_upstream = false;

        while let Some(arg) = parser.next()? {
            match arg {
                Long("verbose") | Short('v') => {
                    verbose = true;
                }
                Long("help") => {
                    return Err(Error::Help.into());
                }
                Long("all") => {
                    all = true;
                }
                Long("set-upstream") | Short('u') => {
                    set_upstream = true;
                }
                Long("force") | Short('f') => {
                    force = true;
                }
                arg => {
                    return Err(anyhow!(arg.unexpected()));
                }
            }
        }

        Ok((
            Options {
                force,
                all,
                set_upstream,
                verbose,
            },
            vec![],
        ))
    }
}

pub fn run(options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    ctx.profile()?;

    term::info!("Pushing 🌱 to remote `rad`");

    let cwd = Path::new(".").canonicalize()?;
    let mut args = vec!["push"];

    if options.force {
        args.push("--force");
    }
    if options.set_upstream {
        args.push("--set-upstream");
    }
    if options.all {
        args.push("--all");
    }
    if options.verbose {
        args.push("--verbose");
    }
    args.push("rad"); // Push to "rad" remote.

    term::subcommand(format!("git {}", args.join(" ")));

    // Push to storage.
    match git::run::<_, _, &str, &str>(cwd, args, []) {
        Ok(output) => term::blob(output),
        Err(err) => return Err(err.into()),
    }

    Ok(())
}

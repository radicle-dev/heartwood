use std::ffi::OsString;
use std::path::Path;

use radicle::git;

use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};

use anyhow::anyhow;

pub const HELP: Help = Help {
    name: "push",
    description: env!("CARGO_PKG_DESCRIPTION"),
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad push [--all] [--[no-]sync] [<option>...]

    By default, only the current branch is synced.

Options

    --all               Push all branches (default: false)
    --sync              Sync after pushing to the "rad" remote (default: false)
    --no-sync           Do not sync after pushing to the "rad" remote
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
    pub sync: bool,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut verbose = false;
        let mut force = false;
        let mut all = false;
        let mut sync = false;
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
                Long("sync") => {
                    sync = true;
                }
                Long("no-sync") => {
                    sync = false;
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
                sync,
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

    term::subcommand(&format!("git {}", args.join(" ")));

    // Push to storage.
    match git::run::<_, _, &str, &str>(&cwd, args, []) {
        Ok(output) => term::blob(output),
        Err(err) => return Err(err.into()),
    }

    if options.sync {
        term::warning("the `--sync` option is not yet supported");
    }

    Ok(())
}

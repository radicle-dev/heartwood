use std::ffi::OsString;

use radicle::storage::{ReadRepository, ReadStorage};

use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};

use term::Element;

pub const HELP: Help = Help {
    name: "ls",
    description: "List projects",
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad ls [<option>...]

Options

    --versbose, -v  Verbose output
    --help          Print help
"#,
};

pub struct Options {
    verbose: bool,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut verbose = false;

        if let Some(arg) = parser.next()? {
            match arg {
                Long("help") => {
                    return Err(Error::Help.into());
                }
                Long("verbose") | Short('v') => verbose = true,
                _ => return Err(anyhow::anyhow!(arg.unexpected())),
            }
        }

        Ok((Options { verbose }, vec![]))
    }
}

pub fn run(options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    let storage = &profile.storage;
    let mut table = term::Table::default();

    storage.repositories()?.into_iter().for_each(|id| {
        let repo = match storage.repository(id) {
            Ok(repo) => repo,
            Err(err) => {
                if options.verbose {
                    term::warning(&format!("failed to load project '{id}': {err}"));
                }
                return;
            }
        };
        let head = match repo.head() {
            Ok((_, head)) => head,
            Err(err) => {
                if options.verbose {
                    term::warning(&format!("failed to get head of project '{id}': {err}"));
                }
                return;
            }
        };
        let proj = match repo.project_of(profile.id()) {
            Ok(proj) => proj,
            Err(err) => {
                if options.verbose {
                    term::warning(&format!("failed to get local project '{id}': {err}"));
                }
                return;
            }
        };
        let head = term::format::oid(head);
        table.push([
            term::format::bold(proj.name().to_owned()),
            term::format::tertiary(id.urn()),
            term::format::secondary(head),
            term::format::italic(proj.description().to_owned()),
        ]);
    });
    table.print();

    Ok(())
}

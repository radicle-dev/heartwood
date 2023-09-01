use std::ffi::OsString;

use radicle::storage::git::RepositoryInfo;

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

    --private       Show only private repositories
    --public        Show only public repositories
    --verbose, -v   Verbose output
    --help          Print help
"#,
};

pub struct Options {
    verbose: bool,
    public: bool,
    private: bool,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut verbose = false;
        let mut private = false;
        let mut public = false;

        while let Some(arg) = parser.next()? {
            match arg {
                Long("help") | Short('h') => {
                    return Err(Error::Help.into());
                }
                Long("private") => {
                    private = true;
                }
                Long("public") => {
                    public = true;
                }
                Long("verbose") | Short('v') => verbose = true,
                _ => return Err(anyhow::anyhow!(arg.unexpected())),
            }
        }

        Ok((
            Options {
                verbose,
                private,
                public,
            },
            vec![],
        ))
    }
}

pub fn run(options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    let storage = &profile.storage;
    let mut table = term::Table::default();

    for RepositoryInfo { rid, head, doc } in storage.repositories()? {
        if doc.visibility.is_public() && options.private && !options.public {
            continue;
        }
        if !doc.visibility.is_public() && !options.private && options.public {
            continue;
        }

        let proj = match doc.verified()?.project() {
            Ok(proj) => proj,
            Err(err) => {
                if options.verbose {
                    term::warning(&format!("failed to get local project '{rid}': {err}"));
                }
                continue;
            }
        };
        let head = term::format::oid(head).into();
        table.push([
            term::format::bold(proj.name().to_owned()),
            term::format::tertiary(rid.urn()),
            term::format::secondary(head),
            term::format::italic(proj.description().to_owned()),
        ]);
    }
    table.print();

    Ok(())
}

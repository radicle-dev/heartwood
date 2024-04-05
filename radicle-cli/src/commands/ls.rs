use std::ffi::OsString;

use radicle::storage::{ReadStorage, RepositoryInfo};

use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};

use term::Element;

pub const HELP: Help = Help {
    name: "ls",
    description: "List repositories",
    version: env!("RADICLE_VERSION"),
    usage: r#"
Usage

    rad ls [<option>...]

    By default, this command shows you all repositories that you have forked or initialized.
    If you wish to see all seeded repositories, use the `--all` option.

Options

    --private       Show only private repositories
    --public        Show only public repositories
    --seeded, -s    Show all seeded repositories
    --all, -a       Show all repositories in storage
    --verbose, -v   Verbose output
    --help          Print help
"#,
};

pub struct Options {
    #[allow(dead_code)]
    verbose: bool,
    public: bool,
    private: bool,
    all: bool,
    seeded: bool,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut verbose = false;
        let mut private = false;
        let mut public = false;
        let mut all = false;
        let mut seeded = false;

        while let Some(arg) = parser.next()? {
            match arg {
                Long("help") | Short('h') => {
                    return Err(Error::Help.into());
                }
                Long("all") | Short('a') => {
                    all = true;
                }
                Long("seeded") | Short('s') => {
                    seeded = true;
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
                all,
                seeded,
            },
            vec![],
        ))
    }
}

pub fn run(options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    let storage = &profile.storage;
    let repos = storage.repositories()?;
    let policy = profile.policies()?;
    let mut table = term::Table::new(term::TableOptions::bordered());
    let mut rows = Vec::new();

    if repos.is_empty() {
        return Ok(());
    }

    for RepositoryInfo {
        rid,
        head,
        doc,
        refs,
    } in repos
    {
        if doc.visibility.is_public() && options.private && !options.public {
            continue;
        }
        if !doc.visibility.is_public() && !options.private && options.public {
            continue;
        }
        if refs.is_none() && !options.all && !options.seeded {
            continue;
        }
        let seeded = policy.is_seeding(&rid)?;

        if !seeded && !options.all {
            continue;
        }
        if !seeded && options.seeded {
            continue;
        }
        let proj = doc.project()?;
        let head = term::format::oid(head).into();

        rows.push([
            term::format::bold(proj.name().to_owned()),
            term::format::tertiary(rid.urn()),
            if seeded {
                term::format::visibility(&doc.visibility).into()
            } else {
                term::format::dim("local").into()
            },
            term::format::secondary(head),
            term::format::italic(proj.description().to_owned()),
        ]);
    }
    rows.sort();

    if rows.is_empty() {
        term::print(term::format::italic("Nothing to show."));
    } else {
        table.push([
            "Name".into(),
            "RID".into(),
            "Visibility".into(),
            "Head".into(),
            "Description".into(),
        ]);
        table.divider();
        table.extend(rows);
        table.print();
    }

    Ok(())
}

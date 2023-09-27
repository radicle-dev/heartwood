use std::ffi::OsString;

use anyhow::anyhow;

use radicle::git;
use radicle::rad;
use radicle_surf as surf;

use crate::git::pretty_diff::ToPretty as _;
use crate::git::Rev;
use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};
use crate::terminal::highlight::Highlighter;
use crate::terminal::{Constraint, Element as _};

pub const HELP: Help = Help {
    name: "diff",
    description: "Show changes between commits",
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad diff [<commit>] [--staged] [<option>...]
    rad diff <commit> [<commit>] [<option>...]

    This command is meant to operate as closely as possible to `git diff`,
    except its output is optimized for human-readability.

Options

    --staged        View staged changes
    --color         Force color output
    --help          Print help
"#,
};

pub struct Options {
    pub commits: Vec<Rev>,
    pub staged: bool,
    pub unified: usize,
    pub color: bool,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut commits = Vec::new();
        let mut staged = false;
        let mut unified = 5;
        let mut color = false;

        while let Some(arg) = parser.next()? {
            match arg {
                Long("unified") | Short('U') => {
                    let val = parser.value()?;
                    unified = term::args::number(&val)?;
                }
                Long("staged") | Long("cached") => staged = true,
                Long("color") => color = true,
                Long("help") | Short('h') => return Err(Error::Help.into()),
                Value(val) => {
                    let rev = term::args::rev(&val)?;

                    commits.push(rev);
                }
                _ => return Err(anyhow::anyhow!(arg.unexpected())),
            }
        }

        Ok((
            Options {
                commits,
                staged,
                unified,
                color,
            },
            vec![],
        ))
    }
}

pub fn run(options: Options, _ctx: impl term::Context) -> anyhow::Result<()> {
    let (repo, _) = rad::cwd()?;
    let oids = options
        .commits
        .into_iter()
        .map(|rev| {
            repo.revparse_single(rev.as_str())
                .map_err(|e| anyhow!("unknown object {rev}: {e}"))
                .and_then(|o| {
                    o.into_commit()
                        .map_err(|_| anyhow!("object {rev} is not a commit"))
                })
        })
        .collect::<Result<Vec<_>, _>>()?;

    let mut opts = git::raw::DiffOptions::new();
    opts.patience(true)
        .minimal(true)
        .context_lines(options.unified as u32);

    let mut find_opts = git::raw::DiffFindOptions::new();
    find_opts.exact_match_only(true);
    find_opts.all(true);
    find_opts.copies(false); // We don't support finding copies at the moment.

    let mut diff = match oids.as_slice() {
        [] => {
            if options.staged {
                let head = repo.head()?.peel_to_tree()?;
                // HEAD vs. index.
                repo.diff_tree_to_index(Some(&head), None, Some(&mut opts))
            } else {
                // Working tree vs. index.
                repo.diff_index_to_workdir(None, None)
            }
        }
        [commit] => {
            let commit = commit.tree()?;
            if options.staged {
                // Commit vs. index.
                repo.diff_tree_to_index(Some(&commit), None, Some(&mut opts))
            } else {
                // Commit vs. working tree.
                repo.diff_tree_to_workdir(Some(&commit), Some(&mut opts))
            }
        }
        [left, right] => {
            // Commit vs. commit.
            let left = left.tree()?;
            let right = right.tree()?;

            repo.diff_tree_to_tree(Some(&left), Some(&right), Some(&mut opts))
        }
        _ => {
            anyhow::bail!("Too many commits given. See `rad diff --help` for usage.");
        }
    }?;
    diff.find_similar(Some(&mut find_opts))?;

    term::Paint::force(options.color);

    let diff = surf::diff::Diff::try_from(diff)?;
    let mut hi = Highlighter::default();
    let pretty = diff.pretty(&mut hi, &(), &repo);

    pretty.write(Constraint::from_env().unwrap_or_default());

    Ok(())
}

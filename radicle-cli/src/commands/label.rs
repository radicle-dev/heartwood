use std::ffi::OsString;
use std::str::FromStr;

use anyhow::anyhow;
use nonempty::NonEmpty;

use radicle::cob;
use radicle::cob::common::Label;
use radicle::cob::{issue, patch, store};
use radicle::crypto::Signer;
use radicle::storage::{self, WriteStorage};

use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};

pub const HELP: Help = Help {
    name: "label",
    description: "Label an issue or patch",
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad label <issue-id> <label>... [<option>...]

    Adds the given labels to the patch or issue.

Options

    --help      Print help
"#,
};

#[derive(Debug)]
pub struct Options {
    pub id: cob::ObjectId,
    pub labels: NonEmpty<Label>,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut id: Option<cob::ObjectId> = None;
        let mut labels: Vec<Label> = Vec::new();

        while let Some(arg) = parser.next()? {
            match arg {
                Long("help") | Short('h') => {
                    return Err(Error::Help.into());
                }
                Value(ref val) if id.is_none() => {
                    id = Some(term::args::cob(val)?);
                }
                Value(ref val) if id.is_some() => {
                    let s: String = val.parse()?;
                    let label = Label::from_str(&s)?;

                    labels.push(label);
                }
                _ => {
                    return Err(anyhow!(arg.unexpected()));
                }
            }
        }

        Ok((
            Options {
                id: id.ok_or_else(|| anyhow!("an issue or patch must be specified"))?,
                labels: NonEmpty::from_vec(labels)
                    .ok_or_else(|| anyhow!("at least one label must be specified"))?,
            },
            vec![],
        ))
    }
}

fn label(
    options: Options,
    repo: &storage::git::Repository,
    signer: impl Signer,
) -> anyhow::Result<()> {
    let mut issues = issue::Issues::open(repo)?;
    match issues.get_mut(&options.id) {
        Ok(mut issue) => {
            let labels = issue
                .labels()
                .cloned()
                .chain(options.labels)
                .collect::<Vec<_>>();

            issue.label(labels, &signer)?;

            return Ok(());
        }
        Err(store::Error::NotFound(_, _)) => {}
        Err(e) => return Err(e.into()),
    }

    let mut patches = patch::Patches::open(repo)?;
    match patches.get_mut(&options.id) {
        Ok(mut patch) => {
            let labels = patch
                .labels()
                .cloned()
                .chain(options.labels)
                .collect::<Vec<_>>();

            patch.label(labels, &signer)?;

            return Ok(());
        }
        Err(store::Error::NotFound(_, _)) => {}
        Err(e) => return Err(e.into()),
    }

    anyhow::bail!("Couldn't find issue or patch {}", options.id)
}

pub fn run(options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    let (_, id) = radicle::rad::cwd()?;
    let repo = profile.storage.repository_mut(id)?;
    let signer = term::signer(&profile)?;

    label(options, &repo, signer)?;

    Ok(())
}

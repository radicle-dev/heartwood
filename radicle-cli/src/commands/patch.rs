#[path = "patch/common.rs"]
mod common;
#[path = "patch/create.rs"]
mod create;
#[path = "patch/list.rs"]
mod list;

use std::ffi::OsString;
use std::str::FromStr;

use anyhow::anyhow;

use radicle::cob::patch::PatchId;
use radicle::prelude::*;

use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};
use crate::terminal::patch::Comment;

pub const HELP: Help = Help {
    name: "patch",
    description: "Work with radicle patches",
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad patch [<option>...]

Create options

    -u, --update [<id>]        Update an existing patch (default: no)
        --[no-]sync            Sync patch to seed (default: sync)
        --[no-]push            Push patch head to storage (default: true)
    -m, --message [<string>]   Provide a comment message to the patch or revision (default: prompt)
        --no-message           Leave the patch or revision comment message blank

Options

    -l, --list                 List all patches (default: false)
        --help                 Print help
"#,
};

#[derive(Debug)]
pub enum Update {
    No,
    Any,
    Patch(PatchId),
}

impl Default for Update {
    fn default() -> Self {
        Self::No
    }
}

#[derive(Default, Debug)]
pub struct Options {
    pub list: bool,
    pub verbose: bool,
    pub sync: bool,
    pub push: bool,
    pub update: Update,
    pub message: Comment,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut list = false;
        let mut verbose = false;
        let mut sync = true;
        let mut message = Comment::default();
        let mut push = true;
        let mut update = Update::default();

        while let Some(arg) = parser.next()? {
            match arg {
                // Operations.
                Long("list") | Short('l') => {
                    list = true;
                }
                Long("update") | Short('u') => {
                    if let Ok(val) = parser.value() {
                        let val = val
                            .to_str()
                            .ok_or_else(|| anyhow!("patch id specified is not UTF-8"))?;
                        let id = PatchId::from_str(val)
                            .map_err(|_| anyhow!("invalid patch id '{}'", val))?;

                        update = Update::Patch(id);
                    } else {
                        update = Update::Any;
                    }
                }

                // Options.
                Long("message") | Short('m') => {
                    let txt: String = parser.value()?.to_string_lossy().into();
                    message.append(&txt);
                }
                Long("no-message") => {
                    message = Comment::Blank;
                }
                Long("sync") => {
                    sync = true;
                }
                Long("no-sync") => {
                    sync = false;
                }
                Long("push") => {
                    push = true;
                }
                Long("no-push") => {
                    push = false;
                }

                // Common.
                Long("verbose") | Short('v') => {
                    verbose = true;
                }
                Long("help") => {
                    return Err(Error::Help.into());
                }
                _ => return Err(anyhow::anyhow!(arg.unexpected())),
            }
        }

        Ok((
            Options {
                list,
                sync,
                message,
                push,
                update,
                verbose,
            },
            vec![],
        ))
    }
}

pub fn run(options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    let (workdir, id) = radicle::rad::cwd()
        .map_err(|_| anyhow!("this command must be run in the context of a project"))?;

    let profile = ctx.profile()?;
    let storage = profile.storage.repository(id)?;

    if options.list {
        list::run(&storage, &profile, Some(workdir), options)?;
    } else {
        create::run(&storage, &profile, &workdir, options)?;
    }
    Ok(())
}

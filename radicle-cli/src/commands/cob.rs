use std::ffi::OsString;
use std::str::FromStr;

use anyhow::anyhow;
use chrono::prelude::*;
use nonempty::NonEmpty;
use radicle::cob;
use radicle::prelude::RepoId;
use radicle::storage::ReadStorage;
use radicle_cob::object::collaboration::list;

use crate::git::Rev;
use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};

pub const HELP: Help = Help {
    name: "cob",
    description: "Manage collaborative objects",
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad cob <command> [<option>...]
    rad cob list --repo <rid> --type <typename>
    rad cob show --repo <rid> --type <typename> --object <oid>

Commands

    list       List all COBs of a given type (--object is not needed)
    show       Show a COB as raw operations

Options

    --help     Print help
"#,
};

enum OperationName {
    List,
    Show,
}

enum Operation {
    List,
    Show(Rev),
}

pub struct Options {
    rid: RepoId,
    op: Operation,
    type_name: cob::TypeName,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut op: Option<OperationName> = None;
        let mut type_name: Option<cob::TypeName> = None;
        let mut oid: Option<Rev> = None;
        let mut rid: Option<RepoId> = None;

        while let Some(arg) = parser.next()? {
            match arg {
                Value(val) if op.is_none() => match val.to_string_lossy().as_ref() {
                    "l" | "list" => op = Some(OperationName::List),
                    "s" | "show" => op = Some(OperationName::Show),
                    unknown => anyhow::bail!("unknown operation '{unknown}'"),
                },
                Long("type") | Short('t') => {
                    let v = parser.value()?;
                    let v = term::args::string(&v);
                    let v = cob::TypeName::from_str(&v)?;

                    type_name = Some(v);
                }
                Long("object") => {
                    let v = parser.value()?;
                    let v = term::args::string(&v);

                    oid = Some(Rev::from(v));
                }
                Long("repo") => {
                    let v = parser.value()?;
                    let v = term::args::rid(&v)?;

                    rid = Some(v);
                }
                Long("help") | Short('h') => {
                    return Err(Error::Help.into());
                }
                _ => return Err(anyhow::anyhow!(arg.unexpected())),
            }
        }

        Ok((
            Options {
                op: {
                    match op.ok_or_else(|| anyhow!("a command must be specified"))? {
                        OperationName::List => Operation::List,
                        OperationName::Show => Operation::Show(oid.ok_or_else(|| {
                            anyhow!("an object id must be specified with `--object")
                        })?),
                    }
                },
                rid: rid
                    .ok_or_else(|| anyhow!("a repository id must be specified with `--repo`"))?,
                type_name: type_name
                    .ok_or_else(|| anyhow!("an object type must be specified with `--type`"))?,
            },
            vec![],
        ))
    }
}

pub fn run(options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    let storage = &profile.storage;
    let repo = storage.repository(options.rid)?;

    match options.op {
        Operation::List => {
            let cobs = list::<NonEmpty<cob::Entry>, _>(&repo, &options.type_name)?;
            for cob in cobs {
                println!("{}", cob.id);
            }
        }
        Operation::Show(oid) => {
            let oid = oid.resolve(&repo.backend)?;
            let ops = cob::store::ops(&oid, &options.type_name, &repo)?;

            for op in ops.into_iter().rev() {
                let time = DateTime::<Utc>::from(
                    std::time::UNIX_EPOCH + std::time::Duration::from_secs(op.timestamp.as_secs()),
                )
                .to_rfc2822();

                term::print(term::format::yellow(format!("commit {}", op.id)));
                if let Some(oid) = op.identity {
                    term::print(term::format::tertiary(format!("parent {oid}")));
                }
                for parent in op.parents {
                    term::print(format!("parent {}", parent));
                }
                term::print(format!("author {}", op.author));
                term::print(format!("date   {}", time));
                term::blank();

                for action in op.actions {
                    let obj: serde_json::Value = serde_json::from_slice(&action)?;
                    let val = serde_json::to_string_pretty(&obj)?;

                    for line in val.lines() {
                        term::indented(term::format::dim(line));
                    }
                    term::blank();
                }
            }
        }
    }

    Ok(())
}

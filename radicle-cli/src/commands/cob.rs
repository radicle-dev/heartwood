use std::ffi::OsString;
use std::str::FromStr;

use anyhow::anyhow;
use chrono::prelude::*;
use nonempty::NonEmpty;
use radicle::cob;
use radicle::cob::migrate;
use radicle::cob::Op;
use radicle::identity::Identity;
use radicle::issue::cache::Issues;
use radicle::patch::cache::Patches;
use radicle::prelude::RepoId;
use radicle::storage::ReadStorage;
use radicle_cob::object::collaboration::list;
use serde_json::json;

use crate::git::Rev;
use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};

pub const HELP: Help = Help {
    name: "cob",
    description: "Manage collaborative objects",
    version: env!("RADICLE_VERSION"),
    usage: r#"
Usage

    rad cob <command> [<option>...]
    rad cob list  --repo <rid> --type <typename>
    rad cob log   --repo <rid> --type <typename> --object <oid> [<option>...]
    rad cob show  --repo <rid> --type <typename> --object <oid> [<option>...]

Commands

    list                       List all COBs of a given type (--object is not needed)
    log                        Print a log of all raw operations on a COB

Log options

    --format (pretty | json)   Desired output format (default: pretty)

Show options

    --format json              Desired output format (default: json)

Other options

    --help                     Print help
"#,
};

#[derive(PartialEq)]
enum OperationName {
    List,
    Log,
    Show,
}

enum Operation {
    List,
    Log(Rev),
    Show(Rev),
}

enum Format {
    Json,
    Pretty,
}

pub struct Options {
    rid: RepoId,
    op: Operation,
    type_name: cob::TypeName,
    format: Format,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut op: Option<OperationName> = None;
        let mut type_name: Option<cob::TypeName> = None;
        let mut oid: Option<Rev> = None;
        let mut rid: Option<RepoId> = None;
        let mut format: Option<Format> = None;

        while let Some(arg) = parser.next()? {
            match arg {
                Value(val) if op.is_none() => match val.to_string_lossy().as_ref() {
                    "list" => op = Some(OperationName::List),
                    "log" => op = Some(OperationName::Log),
                    "show" => op = Some(OperationName::Show),
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
                Long("format")
                    if op == Some(OperationName::Log) || op == Some(OperationName::Show) =>
                {
                    let v: String = term::args::string(&parser.value()?);
                    match v.as_ref() {
                        "pretty" if op == Some(OperationName::Log) => format = Some(Format::Pretty),
                        "json" => format = Some(Format::Json),
                        unknown => anyhow::bail!("unknown format '{unknown}'"),
                    }
                }
                _ => return Err(anyhow::anyhow!(arg.unexpected())),
            }
        }

        Ok((
            Options {
                op: {
                    match op.ok_or_else(|| anyhow!("a command must be specified"))? {
                        OperationName::List => Operation::List,
                        OperationName::Log => Operation::Log(oid.ok_or_else(|| {
                            anyhow!("an object id must be specified with `--object`")
                        })?),
                        OperationName::Show => Operation::Show(oid.ok_or_else(|| {
                            anyhow!("an object id must be specified with `--object`")
                        })?),
                    }
                },
                rid: rid
                    .ok_or_else(|| anyhow!("a repository id must be specified with `--repo`"))?,
                type_name: type_name
                    .ok_or_else(|| anyhow!("an object type must be specified with `--type`"))?,
                format: format.unwrap_or(Format::Pretty),
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
        Operation::Log(oid) => {
            let oid = oid.resolve(&repo.backend)?;
            let ops = cob::store::ops(&oid, &options.type_name, &repo)?;

            for op in ops.into_iter().rev() {
                match options.format {
                    Format::Json => print_op_json(op)?,
                    Format::Pretty => print_op_pretty(op)?,
                }
            }
        }
        Operation::Show(oid) => {
            let oid = &oid.resolve(&repo.backend)?;

            if options.type_name == cob::patch::TYPENAME.clone() {
                let patches = profile.patches(&repo, migrate::ignore)?;
                let Some(patch) = patches.get(oid)? else {
                    anyhow::bail!(cob::store::Error::NotFound(options.type_name, *oid))
                };
                serde_json::to_writer_pretty(std::io::stdout(), &patch)?
            } else if options.type_name == cob::issue::TYPENAME.clone() {
                let issues = profile.issues(&repo, migrate::ignore)?;
                let Some(issue) = issues.get(oid)? else {
                    anyhow::bail!(cob::store::Error::NotFound(options.type_name, *oid))
                };
                serde_json::to_writer_pretty(std::io::stdout(), &issue)?
            } else if options.type_name == cob::identity::TYPENAME.clone() {
                let Some(cob) = cob::get::<Identity, _>(&repo, &options.type_name, oid)? else {
                    anyhow::bail!(cob::store::Error::NotFound(options.type_name, *oid))
                };
                serde_json::to_writer_pretty(std::io::stdout(), &cob.object)?
            } else {
                anyhow::bail!("the type name '{}' is unknown", options.type_name);
            }
            println!();
        }
    }

    Ok(())
}

fn print_op_pretty(op: Op<Vec<u8>>) -> anyhow::Result<()> {
    let time = DateTime::<Utc>::from(
        std::time::UNIX_EPOCH + std::time::Duration::from_secs(op.timestamp.as_secs()),
    )
    .to_rfc2822();
    term::print(term::format::yellow(format!("commit   {}", op.id)));
    if let Some(oid) = op.identity {
        term::print(term::format::tertiary(format!("resource {oid}")));
    }
    for parent in op.parents {
        term::print(format!("parent   {}", parent));
    }
    for parent in op.related {
        term::print(format!("rel      {}", parent));
    }
    term::print(format!("author   {}", op.author));
    term::print(format!("date     {}", time));
    term::blank();
    for action in op.actions {
        let obj: serde_json::Value = serde_json::from_slice(&action)?;
        let val = serde_json::to_string_pretty(&obj)?;
        for line in val.lines() {
            term::indented(term::format::dim(line));
        }
        term::blank();
    }
    Ok(())
}

fn print_op_json(op: Op<Vec<u8>>) -> anyhow::Result<()> {
    let mut ser = json!(op);
    ser.as_object_mut().unwrap().insert(
        "actions".to_string(),
        json!(op
            .actions
            .iter()
            .map(|action: &Vec<u8>| -> Result<serde_json::Value, _> {
                serde_json::from_slice(action)
            })
            .collect::<Result<Vec<serde_json::Value>, _>>()?),
    );
    term::print(ser);
    Ok(())
}

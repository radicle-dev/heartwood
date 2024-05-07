use std::ffi::OsString;
use std::str::FromStr;

use anyhow::anyhow;
use chrono::prelude::*;
use nonempty::NonEmpty;
use radicle::cob;
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
    rad cob list --repo <rid> --type <typename>
    rad cob log  --repo <rid> --type <typename> --object <oid>

Commands

    list       List all COBs of a given type (--object is not needed)
    log        Print a log of all raw operations on a COB
    show       Print the materialized object of a COB

Options

    --help     Print help
"#,
};

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
                _ => return Err(anyhow::anyhow!(arg.unexpected())),
            }
        }

        Ok((
            Options {
                op: {
                    match op.ok_or_else(|| anyhow!("a command must be specified"))? {
                        OperationName::List => Operation::List,
                        OperationName::Log => Operation::Log(oid.ok_or_else(|| {
                            anyhow!("an object id must be specified with `--object")
                        })?),
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
        Operation::Log(oid) => {
            let oid = oid.resolve(&repo.backend)?;
            let ops = cob::store::ops(&oid, &options.type_name, &repo)?;

            for op in ops.into_iter().rev() {
                let mut ser = json!(op);
                ser.as_object_mut().unwrap().insert(
                    "actions".to_string(),
                    json!(op
                        .actions
                        .iter()
                        .map(|action: &Vec<u8>| -> Result<serde_json::Value, _> {
                            serde_json::from_slice(&action)
                        })
                        .collect::<Result<Vec<serde_json::Value>, _>>()?),
                );
                term::print(ser)
            }
        }
        Operation::Show(oid) => {
            let oid = &oid.resolve(&repo.backend)?;

            if options.type_name == cob::patch::TYPENAME.clone() {
                let patches = profile.patches(&repo)?;
                let Some(patch) = patches.get(oid)? else {
                    anyhow::bail!(cob::store::Error::NotFound(options.type_name, *oid))
                };
                term::print(json!(patch))
            } else if options.type_name == cob::issue::TYPENAME.clone() {
                let issues = profile.issues(&repo)?;
                let Some(issue) = issues.get(oid)? else {
                    anyhow::bail!(cob::store::Error::NotFound(options.type_name, *oid))
                };
                term::print(json!(issue))
            } else if options.type_name == cob::identity::TYPENAME.clone() {
                let Some(cob) = cob::get::<Identity, _>(&repo, &options.type_name, oid)? else {
                    anyhow::bail!(cob::store::Error::NotFound(options.type_name, *oid))
                };
                term::print(json!(&cob.object))
            } else {
                anyhow::bail!("the type name '{}' is unknown", options.type_name)
            }
        }
    }

    Ok(())
}

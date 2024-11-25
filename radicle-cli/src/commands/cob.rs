use std::ffi::OsString;
use std::io;
use std::io::Write;
use std::str::FromStr;

use anyhow::anyhow;
use chrono::prelude::*;
use nonempty::NonEmpty;
use radicle::cob;
use radicle::cob::stream::CobStream as _;
use radicle::cob::Op;
use radicle::git;
use radicle::identity::Identity;
use radicle::issue::cache::Issues as _;
use radicle::patch::cache::Patches as _;
use radicle::prelude::RepoId;
use radicle::storage;
use radicle::storage::ReadStorage;
use radicle::Profile;
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
    rad cob log --repo <rid> --type <typename> --object <oid> [<option>...]
    rad cob ops  --repo <rid> --type <typename> --object <oid> [<option>...]
    rad cob show --repo <rid> --type <typename> --object <oid> [<option>...]
    rad cob migrate [<option>...]

Commands

    list                       List all COBs of a given type (--object is not needed)
    log                        Print a log of all raw operations on a COB
    ops                        Print all the operations for a a COB
    migrate                    Migrate the COB database to the latest version

Log options

    --format (pretty | json)   Desired output format (default: pretty)

Show options

    --format json              Desired output format (default: json)

Ops options

    --from <oid>               Where to start the action iteration
    --until <oid>              Where to end the action iteration

Other options

    --help                     Print help
"#,
};

#[derive(PartialEq)]
enum OperationName {
    List,
    Log,
    Migrate,
    Show,
    Operations,
}

enum Operation {
    List {
        repo: RepoId,
        type_name: cob::TypeName,
    },
    Log {
        repo: RepoId,
        rev: Rev,
        type_name: cob::TypeName,
    },
    Migrate,
    Show {
        repo: RepoId,
        revs: Vec<Rev>,
        type_name: cob::TypeName,
    },
    Operations {
        repo: RepoId,
        rev: Rev,
        from: Option<Rev>,
        until: Option<Rev>,
        type_name: cob::TypeName,
    },
}

enum Format {
    Json,
    Pretty,
}

pub struct Options {
    op: Operation,
    format: Format,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut op: Option<OperationName> = None;
        let mut type_name: Option<cob::TypeName> = None;
        let mut revs: Vec<Rev> = vec![];
        let mut rid: Option<RepoId> = None;
        let mut format: Option<Format> = None;
        let mut from: Option<Rev> = None;
        let mut until: Option<Rev> = None;

        while let Some(arg) = parser.next()? {
            match arg {
                Value(val) if op.is_none() => match val.to_string_lossy().as_ref() {
                    "list" => op = Some(OperationName::List),
                    "log" => op = Some(OperationName::Log),
                    "migrate" => op = Some(OperationName::Migrate),
                    "show" => op = Some(OperationName::Show),
                    "actions" => op = Some(OperationName::Operations),
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

                    revs.push(Rev::from(v));
                }
                Long("repo") => {
                    let v = parser.value()?;
                    let v = term::args::rid(&v)?;

                    rid = Some(v);
                }
                Long("from") if matches!(op, Some(OperationName::Operations)) => {
                    let v = parser.value()?;
                    from = Some(term::args::rev(&v)?);
                }
                Long("until") if matches!(op, Some(OperationName::Operations)) => {
                    let v = parser.value()?;
                    until = Some(term::args::rev(&v)?);
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
        let repo = rid.ok_or_else(|| anyhow!("a repository id must be specified with `--repo`"));
        let type_name =
            type_name.ok_or_else(|| anyhow!("an object type must be specified with `--type`"));

        Ok((
            Options {
                op: {
                    match op.ok_or_else(|| anyhow!("a command must be specified"))? {
                        OperationName::List => Operation::List {
                            repo: repo?,
                            type_name: type_name?,
                        },
                        OperationName::Log => Operation::Log {
                            repo: repo?,
                            rev: revs.pop().ok_or_else(|| {
                                anyhow!("an object id must be specified with `--object`")
                            })?,
                            type_name: type_name?,
                        },
                        OperationName::Migrate => Operation::Migrate,
                        OperationName::Show => {
                            if revs.is_empty() {
                                anyhow::bail!("an object id must be specified with `--object`")
                            }
                            Operation::Show {
                                repo: repo?,
                                revs,
                                type_name: type_name?,
                            }
                        }
                        OperationName::Operations => Operation::Operations {
                            repo: repo?,
                            rev: revs.pop().ok_or_else(|| {
                                anyhow!("an object id must be specified with `--object`")
                            })?,
                            from,
                            until,
                            type_name: type_name?,
                        },
                    }
                },
                format: format.unwrap_or(Format::Pretty),
            },
            vec![],
        ))
    }
}

pub fn run(options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    let storage = &profile.storage;

    match options.op {
        Operation::List { repo, type_name } => {
            let repo = storage.repository(repo)?;
            let cobs = list::<NonEmpty<cob::Entry>, _>(&repo, &type_name)?;
            for cob in cobs {
                println!("{}", cob.id);
            }
        }
        Operation::Log {
            repo,
            rev: oid,
            type_name,
        } => {
            let repo = storage.repository(repo)?;
            let oid = oid.resolve(&repo.backend)?;
            let ops = cob::store::ops(&oid, &type_name, &repo)?;

            for op in ops.into_iter().rev() {
                match options.format {
                    Format::Json => print_op_json(op)?,
                    Format::Pretty => print_op_pretty(op)?,
                }
            }
        }
        Operation::Migrate => {
            let mut db = profile.cobs_db_mut()?;
            if db.check_version().is_ok() {
                term::success!("Collaborative objects database is already up to date");
            } else {
                let version = db.migrate(term::cob::migrate::spinner())?;
                term::success!(
                    "Migrated collaborative objects database successfully (version={version})"
                );
            }
        }
        Operation::Show {
            repo,
            revs,
            type_name,
        } => {
            let repo = storage.repository(repo)?;

            if let Err(e) = show(revs, &repo, type_name, &profile) {
                if let Some(err) = e.downcast_ref::<io::Error>() {
                    if err.kind() == io::ErrorKind::BrokenPipe {
                        return Ok(());
                    }
                }
                return Err(e);
            }
        }
        Operation::Operations {
            repo,
            rev: id,
            from,
            until,
            type_name,
        } => {
            let repo = storage.repository(repo)?;
            let id = id.resolve(&repo.backend)?;
            let from = from.map(|from| from.resolve(&repo.backend)).transpose()?;
            let until = until
                .map(|until| until.resolve(&repo.backend))
                .transpose()?;
            if type_name == cob::patch::TYPENAME.clone() {
                operations::<cob::patch::Action>(type_name, id, from, until, &repo)?;
            } else if type_name == cob::issue::TYPENAME.clone() {
                operations::<cob::issue::Action>(type_name, id, from, until, &repo)?;
            } else if type_name == cob::identity::TYPENAME.clone() {
                operations::<cob::identity::Action>(type_name, id, from, until, &repo)?;
            } else {
                operations::<serde_json::Value>(type_name, id, from, until, &repo)?;
            }
        }
    }

    Ok(())
}

fn show(
    revs: Vec<Rev>,
    repo: &storage::git::Repository,
    type_name: cob::TypeName,
    profile: &Profile,
) -> Result<(), anyhow::Error> {
    let mut stdout = std::io::stdout();

    if type_name == cob::patch::TYPENAME.clone() {
        let patches = term::cob::patches(profile, repo)?;
        for oid in revs {
            let oid = &oid.resolve(&repo.backend)?;
            let Some(patch) = patches.get(oid)? else {
                anyhow::bail!(cob::store::Error::NotFound(type_name, *oid));
            };
            serde_json::to_writer(&stdout, &patch)?;
            stdout.write_all(b"\n")?;
        }
    } else if type_name == cob::issue::TYPENAME.clone() {
        let issues = term::cob::issues(profile, repo)?;
        for oid in revs {
            let oid = &oid.resolve(&repo.backend)?;
            let Some(issue) = issues.get(oid)? else {
                anyhow::bail!(cob::store::Error::NotFound(type_name, *oid))
            };
            serde_json::to_writer(&stdout, &issue)?;
            stdout.write_all(b"\n")?;
        }
    } else if type_name == cob::identity::TYPENAME.clone() {
        for oid in revs {
            let oid = &oid.resolve(&repo.backend)?;
            let Some(cob) = cob::get::<Identity, _>(repo, &type_name, oid)? else {
                anyhow::bail!(cob::store::Error::NotFound(type_name, *oid));
            };
            serde_json::to_writer(&stdout, &cob.object)?;
            stdout.write_all(b"\n")?;
        }
    } else {
        anyhow::bail!("the type name '{type_name}' is unknown");
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

fn operations<A>(
    typename: cob::TypeName,
    oid: cob::ObjectId,
    from: Option<git::Oid>,
    until: Option<git::Oid>,
    repo: &storage::git::Repository,
) -> anyhow::Result<()>
where
    A: serde::Serialize,
    A: for<'de> serde::Deserialize<'de>,
{
    let history = cob::stream::CobRange::new(&typename, &oid);
    let stream = cob::stream::Stream::<A>::new(&repo.backend, history, typename);
    let iter = match (from, until) {
        (None, None) => stream.all()?,
        (None, Some(until)) => stream.until(until)?,
        (Some(from), None) => stream.since(from)?,
        (Some(from), Some(until)) => stream.range(from, until)?,
    };

    for action in iter {
        let action = action?;
        println!("{}", serde_json::to_string_pretty(&action)?);
    }

    Ok(())
}

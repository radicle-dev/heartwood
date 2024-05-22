use std::ffi::OsString;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::str::FromStr;

use anyhow::anyhow;
use chrono::prelude::*;
use nonempty::NonEmpty;
use radicle_git_ext::oid::Oid;
use serde_json::json;
use serde_jsonlines::JsonLinesReader;

use radicle::cob;
use radicle::cob::helper::Helper;
use radicle::cob::store::Store;
use radicle::cob::Op;
use radicle::identity::Identity;
use radicle::issue::cache::Issues;
use radicle::patch::cache::Patches;
use radicle::patch::Patch;
use radicle::prelude::RepoId;
use radicle::storage::git::Repository;
use radicle::storage::{ReadStorage, WriteStorage};
use radicle_cob::object::collaboration::list;

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
    rad cob create --repo <rid> --type <typename> <filename> [<option>...]
    rad cob list   --repo <rid> --type <typename>
    rad cob log    --repo <rid> --type <typename> --object <oid> [<option>...]
    rad cob show   --repo <rid> --type <typename> --object <oid> [<option>...]
    rad cob update --repo <rid> --type <typename> --object <oid> <filename>
                    [<option>...]

Commands

    update                      Add actions to a COB
    create                      Create a new COB of a given type given initial actions
    list                        List all COBs of a given type (--object is not needed)
    log                         Print a log of all raw operations on a COB

Log options

    --format (pretty | json)    Desired output format (default: pretty)

Create, Update options

    --embed-file <name> <path>  Supply embed of given name via file at given path
    --embed-hash <name> <oid>   Supply embed of given name via object ID of blob

Show, Update options

    --format json               Desired output format (default: json)

Other options

    --help                      Print help
"#,
};

#[derive(Clone, Copy, PartialEq)]
enum OperationName {
    Update,
    Create,
    List,
    Log,
    Show,
}

enum EmbedContent {
    Path(PathBuf),
    Hash(Rev),
}

/// A precursor to [`cob::Embed`] used for parsing
/// that can be initialized without relying on a [`Repository`].
struct Embed {
    name: String,
    content: EmbedContent,
}

/// A thin wrapper around [`cob::TypeName`] used for parsing.
/// Well known COB type names are captured as variants,
/// with [`TypeName::Other`] as an escape hatch for type names
/// that are not well known.
enum FilteredTypeName {
    Issue,
    Patch,
    Identity,
    Other(cob::TypeName),
}

impl From<cob::TypeName> for FilteredTypeName {
    fn from(value: cob::TypeName) -> Self {
        if value == *cob::issue::TYPENAME {
            FilteredTypeName::Issue
        } else if value == *cob::patch::TYPENAME {
            FilteredTypeName::Patch
        } else if value == *cob::identity::TYPENAME {
            FilteredTypeName::Identity
        } else {
            FilteredTypeName::Other(value)
        }
    }
}

impl AsRef<cob::TypeName> for FilteredTypeName {
    fn as_ref(&self) -> &cob::TypeName {
        match self {
            FilteredTypeName::Issue => &cob::issue::TYPENAME,
            FilteredTypeName::Patch => &cob::patch::TYPENAME,
            FilteredTypeName::Identity => &cob::identity::TYPENAME,
            FilteredTypeName::Other(value) => value,
        }
    }
}

impl Embed {
    fn try_into_bytes(self, repo: &Repository) -> anyhow::Result<cob::Embed<Vec<u8>>> {
        match self {
            Embed {
                name,
                content: EmbedContent::Hash(hash),
            } => {
                let oid: Oid = hash.resolve(&repo.backend)?;
                Ok(cob::Embed {
                    name,
                    content: repo.backend.find_blob(oid.into())?.content().to_vec(),
                })
            }
            Embed {
                name,
                content: EmbedContent::Path(path),
            } => Ok(cob::Embed::file(name, path)?),
        }
    }
}

enum Operation {
    Update {
        oid: Rev,
        message: String,
        actions: PathBuf,
        embeds: Vec<Embed>,
    },
    Create {
        message: String,
        actions: PathBuf,
        embeds: Vec<Embed>,
    },
    List,
    Log {
        oid: Rev,
        format: Format,
    },
    Show(Rev),
}

enum Format {
    Json,
    Pretty,
}

pub struct Options {
    rid: RepoId,
    op: Operation,
    type_name: FilteredTypeName,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;
        use OperationName::*;

        let mut parser = lexopt::Parser::from_args(args);

        let op = match parser.next()? {
            None | Some(Long("help") | Short('h')) => {
                return Err(Error::Help.into());
            }
            Some(Value(val)) => match val.to_string_lossy().as_ref() {
                "update" => Update,
                "create" => Create,
                "list" => List,
                "log" => Log,
                "show" => Show,
                unknown => anyhow::bail!("unknown operation '{unknown}'"),
            },
            Some(arg) => return Err(anyhow::anyhow!(arg.unexpected())),
        };

        let mut type_name: Option<FilteredTypeName> = None;
        let mut oid: Option<Rev> = None;
        let mut rid: Option<RepoId> = None;
        let mut format: Format = Format::Pretty;
        let mut message: Option<String> = None;
        let mut embeds: Vec<Embed> = vec![];
        let mut actions: Option<PathBuf> = None;

        while let Some(arg) = parser.next()? {
            match (&op, &arg) {
                (_, Long("help") | Short('h')) => {
                    return Err(Error::Help.into());
                }
                (_, Long("repo") | Short('r')) => {
                    rid = Some(term::args::rid(&parser.value()?)?);
                }
                (_, Long("type") | Short('t')) => {
                    let v = term::args::string(&parser.value()?);
                    type_name = Some(FilteredTypeName::from(cob::TypeName::from_str(&v)?));
                }
                (Update | Log | Show, Long("object") | Short('o')) => {
                    let v = term::args::string(&parser.value()?);
                    oid = Some(Rev::from(v));
                }
                (Update | Create, Long("message") | Short('m')) => {
                    message = Some(term::args::string(&parser.value()?));
                }
                (Log | Show | Update, Long("format")) => {
                    format = match (op, term::args::string(&parser.value()?).as_ref()) {
                        (Log, "pretty") => Format::Pretty,
                        (Log | Show | Update, "json") => Format::Json,
                        (_, unknown) => anyhow::bail!("unknown format '{unknown}'"),
                    };
                }
                (Update | Create, Long("embed-file")) => {
                    let mut values = parser.values()?;

                    let name = values
                        .next()
                        .map(|s| term::args::string(&s))
                        .ok_or(anyhow!("expected name of embed"))?;

                    let content = EmbedContent::Path(PathBuf::from(
                        values
                            .next()
                            .ok_or(anyhow!("expected path to file to embed"))?,
                    ));

                    embeds.push(Embed { name, content });
                }
                (Update | Create, Long("embed-hash")) => {
                    let mut values = parser.values()?;

                    let name = values
                        .next()
                        .map(|s| term::args::string(&s))
                        .ok_or(anyhow!("expected name of embed"))?;

                    let content = EmbedContent::Hash(Rev::from(term::args::string(
                        &values
                            .next()
                            .ok_or(anyhow!("expected hash of file to embed"))?,
                    )));

                    embeds.push(Embed { name, content });
                }
                (Update | Create, Value(val)) => {
                    actions = Some(PathBuf::from(term::args::string(val)));
                }
                _ => return Err(anyhow::anyhow!(arg.unexpected())),
            }
        }

        Ok((
            Options {
                op: match op {
                    Update => Operation::Update {
                        oid: oid.ok_or_else(|| {
                            anyhow!("an object id must be specified with `--object`")
                        })?,
                        message: message.ok_or_else(|| {
                            anyhow!("a message must be specified with `--message`")
                        })?,
                        actions: actions.ok_or_else(|| {
                            anyhow!("a file containing actions must be specified")
                        })?,
                        embeds,
                    },
                    Create => Operation::Create {
                        message: message.ok_or_else(|| {
                            anyhow!("a message must be specified with `--message`")
                        })?,
                        actions: actions.ok_or_else(|| {
                            anyhow!("a file containing initial actions must be specified")
                        })?,
                        embeds,
                    },
                    List => Operation::List,
                    Log => Operation::Log {
                        oid: oid.ok_or_else(|| {
                            anyhow!("an object id must be specified with `--object`")
                        })?,
                        format,
                    },
                    Show => Operation::Show(oid.ok_or_else(|| {
                        anyhow!("an object id must be specified with `--object`")
                    })?),
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
    let Options { rid, op, type_name } = options;
    let profile: radicle::Profile = ctx.profile()?;
    let storage = &profile.storage;
    let repo = storage.repository(rid)?;

    match op {
        Operation::Update {
            oid,
            message,
            actions,
            embeds,
        } => {
            let repo = storage.repository_mut(rid)?;
            let reader = JsonLinesReader::new(BufReader::new(File::open(actions)?));
            let oid = &oid.resolve(&repo.backend)?;
            let mut embeds = embeds
                .into_iter()
                .map(|embed| embed.try_into_bytes(&repo))
                .collect::<anyhow::Result<Vec<_>>>()?;

            println!(
                "{}",
                match type_name {
                    FilteredTypeName::Patch => {
                        let mut actions = reader
                            .read_all::<radicle::cob::patch::Action>()
                            .collect::<std::io::Result<Vec<_>>>()?;

                        let mut patches = profile.patches_mut(&repo)?;
                        let mut patch = patches.get_mut(oid)?;

                        patch.transaction(&message, &profile.signer()?, |tx| {
                            tx.actions.append(&mut actions);
                            tx.embeds.append(&mut embeds);
                            Ok(())
                        })?
                    }
                    FilteredTypeName::Issue => {
                        let mut actions = reader
                            .read_all::<radicle::cob::issue::Action>()
                            .collect::<std::io::Result<Vec<_>>>()?;

                        let mut issues = profile.issues_mut(&repo)?;
                        let mut issue = issues.get_mut(oid)?;

                        issue.transaction(&message, &profile.signer()?, |tx| {
                            tx.actions.append(&mut actions);
                            tx.embeds.append(&mut embeds);
                            Ok(())
                        })?
                    }
                    FilteredTypeName::Identity => {
                        let mut actions = reader
                            .read_all::<radicle::cob::identity::Action>()
                            .collect::<std::io::Result<Vec<_>>>()?;

                        let mut identity = Identity::get_mut(oid, &repo)?;

                        identity.transaction(&message, &profile.signer()?, |tx| {
                            tx.actions.append(&mut actions);
                            tx.embeds.append(&mut embeds);
                            Ok(())
                        })?
                    }
                    FilteredTypeName::Other(type_name) => {
                        let mut store: Store<Helper, _> = Store::open_for(&type_name, &repo)?;
                        let actions = reader
                            .read_all::<radicle::cob::helper::Action>()
                            .collect::<std::io::Result<Vec<_>>>()?;
                        let tx = cob::store::Transaction::new(&type_name, actions, embeds);
                        tx.commit(&message, *oid, &mut store, &profile.signer()?)?.1
                    }
                }
            );
        }
        Operation::Create {
            message,
            embeds,
            actions,
        } => {
            let repo = storage.repository_mut(rid)?;
            let reader = JsonLinesReader::new(BufReader::new(File::open(actions)?));
            let embeds = embeds
                .into_iter()
                .map(|embed| embed.try_into_bytes(&repo))
                .collect::<anyhow::Result<Vec<_>>>()?;

            println!(
                "{}",
                match type_name {
                    FilteredTypeName::Patch => {
                        let store: Store<Patch, _> = Store::open(&repo)?;
                        let actions = reader
                            .read_all::<radicle::cob::patch::Action>()
                            .collect::<std::io::Result<Vec<_>>>()?;
                        let actions = NonEmpty::from_vec(actions)
                            .ok_or_else(|| anyhow::anyhow!("at least one action is required"))?;
                        let (oid, _) =
                            store.create(&message, actions, embeds, &profile.signer()?)?;
                        oid
                    }
                    FilteredTypeName::Issue => {
                        let store: Store<cob::issue::Issue, _> = Store::open(&repo)?;
                        let actions = reader
                            .read_all::<radicle::cob::issue::Action>()
                            .collect::<std::io::Result<Vec<_>>>()?;
                        let actions = NonEmpty::from_vec(actions)
                            .ok_or_else(|| anyhow::anyhow!("at least one action is required"))?;
                        let (oid, _) =
                            store.create(&message, actions, embeds, &profile.signer()?)?;
                        oid
                    }
                    FilteredTypeName::Identity => {
                        let store: Store<radicle::cob::identity::Identity, _> = Store::open(&repo)?;
                        let actions = reader
                            .read_all::<radicle::cob::identity::Action>()
                            .collect::<std::io::Result<Vec<_>>>()?;
                        let actions = NonEmpty::from_vec(actions)
                            .ok_or_else(|| anyhow::anyhow!("at least one action is required"))?;
                        let (oid, _) =
                            store.create(&message, actions, embeds, &profile.signer()?)?;
                        oid
                    }
                    FilteredTypeName::Other(type_name) => {
                        let store: Store<Helper, _> = Store::open_for(&type_name, &repo)?;
                        let actions = reader
                            .read_all::<radicle::cob::helper::Action>()
                            .collect::<std::io::Result<Vec<_>>>()?;
                        let actions = NonEmpty::from_vec(actions)
                            .ok_or_else(|| anyhow::anyhow!("at least one action is required"))?;
                        let (oid, _) =
                            store.create(&message, actions, embeds, &profile.signer()?)?;
                        oid
                    }
                }
            );
        }
        Operation::List => {
            let cobs = list::<NonEmpty<cob::Entry>, _>(&repo, type_name.as_ref())?;
            for cob in cobs {
                println!("{}", cob.id);
            }
        }
        Operation::Log { oid, format } => {
            let oid = oid.resolve(&repo.backend)?;
            let ops = cob::store::ops(&oid, type_name.as_ref(), &repo)?;

            for op in ops.into_iter().rev() {
                match format {
                    Format::Json => print_op_json(op)?,
                    Format::Pretty => print_op_pretty(op)?,
                }
            }
        }
        Operation::Show(oid) => {
            let oid = &oid.resolve(&repo.backend)?;

            match type_name {
                FilteredTypeName::Patch => {
                    let patches = profile.patches(&repo)?;
                    let patch = patches.get(oid)?.ok_or_else(|| {
                        anyhow!(cob::store::Error::NotFound(
                            type_name.as_ref().clone(),
                            *oid
                        ))
                    })?;
                    serde_json::to_writer_pretty(std::io::stdout(), &patch)?
                }
                FilteredTypeName::Issue => {
                    let issues = profile.issues(&repo)?;
                    let issue = issues.get(oid)?.ok_or_else(|| {
                        anyhow!(cob::store::Error::NotFound(
                            type_name.as_ref().clone(),
                            *oid
                        ))
                    })?;
                    serde_json::to_writer_pretty(std::io::stdout(), &issue)?
                }
                FilteredTypeName::Identity => {
                    let cob = cob::get::<Identity, _>(&repo, type_name.as_ref(), oid)?.ok_or_else(
                        || {
                            anyhow!(cob::store::Error::NotFound(
                                type_name.as_ref().clone(),
                                *oid
                            ))
                        },
                    )?;
                    serde_json::to_writer_pretty(std::io::stdout(), &cob.object)?
                }
                FilteredTypeName::Other(type_name) => {
                    let cob = cob::get::<Helper, _>(&repo, &type_name, oid)?
                        .ok_or_else(|| anyhow!(cob::store::Error::NotFound(type_name, *oid)))?;
                    serde_json::to_writer_pretty(std::io::stdout(), &cob.object())?;
                }
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
    ser.as_object_mut()
        .expect("ops must serialize to objects")
        .insert(
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

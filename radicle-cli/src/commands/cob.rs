use std::ffi::OsString;
use std::path::PathBuf;
use std::str::FromStr;
use std::{fs, io};

use anyhow::{anyhow, bail};

use chrono::prelude::*;

use nonempty::NonEmpty;

use radicle::cob;
use radicle::cob::store::CobAction;
use radicle::prelude::*;
use radicle::storage::git;

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

    rad cob create  --repo <rid> --type <typename> <filename> [<option>...]
    rad cob list    --repo <rid> --type <typename>
    rad cob log     --repo <rid> --type <typename> --object <oid> [<option>...]
    rad cob migrate [<option>...]
    rad cob show    --repo <rid> --type <typename> --object <oid> [<option>...]
    rad cob update  --repo <rid> --type <typename> --object <oid> <filename>
                    [<option>...]

Commands

    create                      Create a new COB of a given type given initial actions
    list                        List all COBs of a given type (--object is not needed)
    log                         Print a log of all raw operations on a COB
    migrate                     Migrate the COB database to the latest version
    update                      Add actions to a COB
    show                        Print the state of COBs

Create, Update options

    --embed-file <name> <path>  Supply embed of given name via file at given path
    --embed-hash <name> <oid>   Supply embed of given name via object ID of blob

Log options

    --format (pretty | json)    Desired output format (default: pretty)

Show options

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
    Migrate,
    Show,
}

enum Operation {
    Create {
        rid: RepoId,
        type_name: FilteredTypeName,
        message: String,
        actions: PathBuf,
        embeds: Vec<Embed>,
    },
    List {
        rid: RepoId,
        type_name: FilteredTypeName,
    },
    Log {
        rid: RepoId,
        type_name: FilteredTypeName,
        oid: Rev,
        format: Format,
    },
    Migrate,
    Show {
        rid: RepoId,
        type_name: FilteredTypeName,
        oids: Vec<Rev>,
    },
    Update {
        rid: RepoId,
        type_name: FilteredTypeName,
        oid: Rev,
        message: String,
        actions: PathBuf,
        embeds: Vec<Embed>,
    },
}

enum Format {
    Json,
    Pretty,
}

pub struct Options {
    op: Operation,
}

/// A precursor to [`cob::Embed`] used for parsing
/// that can be initialized without relying on a [`git::Repository`].
struct Embed {
    name: String,
    content: EmbedContent,
}

enum EmbedContent {
    Path(PathBuf),
    Hash(Rev),
}

/// A thin wrapper around [`cob::TypeName`] used for parsing.
/// Well known COB type names are captured as variants,
/// with [`FilteredTypeName::Other`] as an escape hatch for type names
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
    fn try_into_bytes(self, repo: &git::Repository) -> anyhow::Result<cob::Embed<cob::Uri>> {
        let content = match self.content {
            EmbedContent::Hash(hash) => {
                let oid: git::Oid = hash.resolve(&repo.backend)?;
                &repo.backend.find_blob(oid.into())?.content().to_vec()
            }
            EmbedContent::Path(path) => &std::fs::read(path)?,
        };

        Ok(cob::Embed::store(self.name, content, &repo.backend)?)
    }
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;
        use term::args::string;
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
                "migrate" => Migrate,
                "show" => Show,
                unknown => bail!("unknown operation '{unknown}'"),
            },
            Some(arg) => return Err(anyhow!(arg.unexpected())),
        };

        let mut type_name: Option<FilteredTypeName> = None;
        let mut oids: Vec<Rev> = vec![];
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
                    let v = string(&parser.value()?);
                    type_name = Some(FilteredTypeName::from(cob::TypeName::from_str(&v)?));
                }
                (Update | Log | Show, Long("object") | Short('o')) => {
                    let v = string(&parser.value()?);
                    oids.push(Rev::from(v));
                }
                (Update | Create, Long("message") | Short('m')) => {
                    message = Some(string(&parser.value()?));
                }
                (Log | Show | Update, Long("format")) => {
                    format = match (op, string(&parser.value()?).as_ref()) {
                        (Log, "pretty") => Format::Pretty,
                        (Log | Show | Update, "json") => Format::Json,
                        (_, unknown) => bail!("unknown format '{unknown}'"),
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
                _ => return Err(anyhow!(arg.unexpected())),
            }
        }

        if op == OperationName::Migrate {
            return Ok((
                Options {
                    op: Operation::Migrate,
                },
                vec![],
            ));
        }

        let rid = rid.ok_or_else(|| anyhow!("a repository id must be specified with `--repo`"))?;
        let type_name =
            type_name.ok_or_else(|| anyhow!("an object type must be specified with `--type`"))?;

        let missing_oid = || anyhow!("an object id must be specified with `--object`");
        let missing_message = || anyhow!("a message must be specified with `--message`");

        Ok((
            Options {
                op: match op {
                    Create => Operation::Create {
                        rid,
                        type_name,
                        message: message.ok_or_else(missing_message)?,
                        actions: actions.ok_or_else(|| {
                            anyhow!("a file containing initial actions must be specified")
                        })?,
                        embeds,
                    },
                    List => Operation::List { rid, type_name },
                    Log => Operation::Log {
                        rid,
                        type_name,
                        oid: oids.pop().ok_or_else(missing_oid)?,
                        format,
                    },
                    Migrate => Operation::Migrate,
                    Show => {
                        if oids.is_empty() {
                            return Err(missing_oid());
                        }
                        Operation::Show {
                            rid,
                            oids,
                            type_name,
                        }
                    }
                    Update => Operation::Update {
                        rid,
                        type_name,
                        oid: oids.pop().ok_or_else(missing_oid)?,
                        message: message.ok_or_else(missing_message)?,
                        actions: actions.ok_or_else(|| {
                            anyhow!("a file containing actions must be specified")
                        })?,
                        embeds,
                    },
                },
            },
            vec![],
        ))
    }
}

pub fn run(Options { op }: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    use cob::store::Store;
    use FilteredTypeName::*;
    use Operation::*;

    let profile = ctx.profile()?;
    let storage = &profile.storage;

    match op {
        Create {
            rid,
            type_name,
            message,
            embeds,
            actions,
        } => {
            let signer = &profile.signer()?;
            let repo = storage.repository_mut(rid)?;

            let reader = io::BufReader::new(fs::File::open(actions)?);

            let embeds = embeds
                .into_iter()
                .map(|embed| embed.try_into_bytes(&repo))
                .collect::<anyhow::Result<Vec<_>>>()?;

            let oid = match type_name {
                Patch => {
                    let store: Store<cob::patch::Patch, _> = Store::open(&repo)?;
                    let actions = read_jsonl_actions(reader)?;
                    let (oid, _) = store.create(&message, actions, embeds, signer)?;
                    oid
                }
                Issue => {
                    let store: Store<cob::issue::Issue, _> = Store::open(&repo)?;
                    let actions = read_jsonl_actions(reader)?;
                    let (oid, _) = store.create(&message, actions, embeds, signer)?;
                    oid
                }
                Identity => {
                    let store: Store<cob::identity::Identity, _> = Store::open(&repo)?;
                    let actions = read_jsonl_actions(reader)?;
                    let (oid, _) = store.create(&message, actions, embeds, signer)?;
                    oid
                }
                Other(type_name) => {
                    let store: Store<cob::external::External, _> =
                        Store::open_for(&type_name, &repo)?;
                    let actions = read_jsonl_actions(reader)?;
                    let (oid, _) = store.create(&message, actions, embeds, signer)?;
                    oid
                }
            };
            println!("{oid}");
        }
        Migrate => {
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
        List { rid, type_name } => {
            let repo = storage.repository(rid)?;
            let cobs = radicle_cob::list::<NonEmpty<cob::Entry>, _>(&repo, type_name.as_ref())?;
            for cob in cobs {
                println!("{}", cob.id);
            }
        }
        Log {
            rid,
            type_name,
            oid: rev,
            format,
        } => {
            let repo = storage.repository(rid)?;
            let oid = rev.resolve(&repo.backend)?;
            let ops = cob::store::ops(&oid, type_name.as_ref(), &repo)?;

            for op in ops.into_iter().rev() {
                match format {
                    Format::Json => print_op_json(op)?,
                    Format::Pretty => print_op_pretty(op)?,
                }
            }
        }
        Show {
            rid,
            oids: revs,
            type_name,
        } => {
            let repo = storage.repository(rid)?;
            if let Err(e) = show(revs, &repo, type_name, &profile) {
                if let Some(err) = e.downcast_ref::<std::io::Error>() {
                    if err.kind() == std::io::ErrorKind::BrokenPipe {
                        return Ok(());
                    }
                }
                return Err(e);
            }
        }
        Update {
            rid,
            type_name,
            oid: rev,
            message,
            actions,
            embeds,
        } => {
            let signer = &profile.signer()?;
            let repo = storage.repository_mut(rid)?;
            let reader = io::BufReader::new(fs::File::open(actions)?);
            let oid = &rev.resolve(&repo.backend)?;
            let mut embeds = embeds
                .into_iter()
                .map(|embed| embed.try_into_bytes(&repo))
                .collect::<anyhow::Result<Vec<_>>>()?;

            let oid = match type_name {
                Patch => {
                    let mut actions: Vec<cob::patch::Action> = read_jsonl(reader)?;
                    let mut patches = profile.patches_mut(&repo)?;
                    let mut patch = patches.get_mut(oid)?;
                    patch.transaction(&message, &profile.signer()?, |tx| {
                        tx.actions.append(&mut actions);
                        tx.embeds.append(&mut embeds);
                        Ok(())
                    })?
                }
                Issue => {
                    let mut actions: Vec<cob::issue::Action> = read_jsonl(reader)?;
                    let mut issues = profile.issues_mut(&repo)?;
                    let mut issue = issues.get_mut(oid)?;
                    issue.transaction(&message, &profile.signer()?, |tx| {
                        tx.actions.append(&mut actions);
                        tx.embeds.append(&mut embeds);
                        Ok(())
                    })?
                }
                Identity => {
                    use cob::identity::{Action, Identity};
                    let actions: Vec<Action> = read_jsonl(reader)?;
                    let mut store = Store::<Identity, _>::open(&repo)?;
                    let tx = cob::store::Transaction::new(type_name.as_ref(), actions, embeds);
                    let (_, oid) = tx.commit(&message, *oid, &mut store, signer)?;
                    oid
                }
                Other(type_name) => {
                    use cob::external::{Action, External};
                    let actions: Vec<Action> = read_jsonl(reader)?;
                    let mut store: Store<External, _> = Store::open_for(&type_name, &repo)?;
                    let tx = cob::store::Transaction::new(&type_name, actions, embeds);
                    let (_, oid) = tx.commit(&message, *oid, &mut store, signer)?;
                    oid
                }
            };

            println!("{oid}");
        }
    }
    Ok(())
}

fn show(
    revs: Vec<Rev>,
    repo: &git::Repository,
    type_name: FilteredTypeName,
    profile: &Profile,
) -> Result<(), anyhow::Error> {
    use io::Write as _;
    let mut stdout = std::io::stdout();

    match type_name {
        FilteredTypeName::Identity => {
            use cob::identity;
            for oid in revs {
                let oid = &oid.resolve(&repo.backend)?;
                let Some(cob) = cob::get::<identity::Identity, _>(repo, type_name.as_ref(), oid)?
                else {
                    bail!(cob::store::Error::NotFound(
                        type_name.as_ref().clone(),
                        *oid
                    ));
                };
                serde_json::to_writer(&stdout, &cob.object)?;
                stdout.write_all(b"\n")?;
            }
        }
        FilteredTypeName::Issue => {
            use radicle::issue::cache::Issues as _;
            let issues = term::cob::issues(profile, repo)?;
            for oid in revs {
                let oid = &oid.resolve(&repo.backend)?;
                let Some(issue) = issues.get(oid)? else {
                    bail!(cob::store::Error::NotFound(
                        type_name.as_ref().clone(),
                        *oid
                    ))
                };
                serde_json::to_writer(&stdout, &issue)?;
                stdout.write_all(b"\n")?;
            }
        }
        FilteredTypeName::Patch => {
            use radicle::patch::cache::Patches as _;
            let patches = term::cob::patches(profile, repo)?;
            for oid in revs {
                let oid = &oid.resolve(&repo.backend)?;
                let Some(patch) = patches.get(oid)? else {
                    bail!(cob::store::Error::NotFound(
                        type_name.as_ref().clone(),
                        *oid
                    ));
                };
                serde_json::to_writer(&stdout, &patch)?;
                stdout.write_all(b"\n")?;
            }
        }
        FilteredTypeName::Other(type_name) => {
            let store =
                cob::store::Store::<cob::external::External, _>::open_for(&type_name, repo)?;
            for oid in revs {
                let oid = &oid.resolve(&repo.backend)?;
                let cob = store
                    .get(oid)?
                    .ok_or_else(|| anyhow!(cob::store::Error::NotFound(type_name.clone(), *oid)))?;
                serde_json::to_writer(&stdout, &cob)?;
                stdout.write_all(b"\n")?;
            }
        }
    }
    Ok(())
}

fn print_op_pretty(op: cob::Op<Vec<u8>>) -> anyhow::Result<()> {
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

fn print_op_json(op: cob::Op<Vec<u8>>) -> anyhow::Result<()> {
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

/// Naive implementation for reading JSONL streams,
/// see <https://jsonlines.org/>.
fn read_jsonl<R, T>(reader: io::BufReader<R>) -> anyhow::Result<Vec<T>>
where
    R: io::Read,
    T: serde::de::DeserializeOwned,
{
    use io::BufRead as _;
    let mut result: Vec<T> = Vec::new();
    for line in reader.lines() {
        result.push(serde_json::from_str(&line?)?);
    }
    Ok(result)
}

/// Tiny utility to read a [`NonEmpty`] of COB actions.
/// This is used for `rad cob create` and `rad cob update`.
fn read_jsonl_actions<R, A>(reader: io::BufReader<R>) -> anyhow::Result<NonEmpty<A>>
where
    R: io::Read,
    A: CobAction + serde::de::DeserializeOwned,
{
    NonEmpty::from_vec(read_jsonl(reader)?)
        .ok_or_else(|| anyhow!("at least one action is required"))
}

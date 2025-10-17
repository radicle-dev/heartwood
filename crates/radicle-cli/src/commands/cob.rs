mod args;

use std::io;
use std::path::Path;

use anyhow::{anyhow, bail};

use chrono::prelude::*;

use nonempty::NonEmpty;

use radicle::cob;
use radicle::cob::store::CobAction;
use radicle::cob::stream::CobStream as _;
use radicle::git;
use radicle::prelude::*;
use radicle::storage;

use crate::git::Rev;
use crate::terminal as term;

pub use args::Args;

use args::{parse_many_embeds, FilteredTypeName, Format};

fn embeds(
    repo: &storage::git::Repository,
    files: Vec<String>,
    hashes: Vec<String>,
) -> anyhow::Result<Vec<cob::Embed<cob::Uri>>> {
    parse_many_embeds::<std::path::PathBuf>(&files)
        .chain(parse_many_embeds::<Rev>(&hashes))
        .map(|embed| embed.try_into_bytes(repo))
        .collect::<anyhow::Result<Vec<_>>>()
}

pub fn run(args: Args, ctx: impl term::Context) -> anyhow::Result<()> {
    use args::Command::*;
    use args::FilteredTypeName::*;
    use cob::store::Store;

    let profile = ctx.profile()?;
    let storage = &profile.storage;

    match args.command {
        Create(args::Create {
            repo,
            type_name,
            operation,
        }) => {
            let signer = &profile.signer()?;
            let repo = storage.repository_mut(repo)?;
            let embeds = embeds(&repo, operation.embed_files, operation.embed_hashes)?;

            let oid = match type_name {
                Patch => {
                    let store: Store<cob::patch::Patch, _> = Store::open(&repo)?;
                    let actions = read_jsonl_actions(&operation.actions)?;
                    let (oid, _) = store.create(&operation.message, actions, embeds, signer)?;
                    oid
                }
                Issue => {
                    let store: Store<cob::issue::Issue, _> = Store::open(&repo)?;
                    let actions = read_jsonl_actions(&operation.actions)?;
                    let (oid, _) = store.create(&operation.message, actions, embeds, signer)?;
                    oid
                }
                Identity => anyhow::bail!(
                    "Creation of collaborative objects of type {} is not supported.",
                    &type_name
                ),
                Other(type_name) => {
                    let store: Store<cob::external::External, _> =
                        Store::open_for(&type_name, &repo)?;
                    let actions = read_jsonl_actions(&operation.actions)?;
                    let (oid, _) = store.create(&operation.message, actions, embeds, signer)?;
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
        List { repo, type_name } => {
            let repo = storage.repository(repo)?;
            let cobs = radicle_cob::list::<NonEmpty<cob::Entry>, _>(
                &repo,
                FilteredTypeName::from(type_name).as_ref(),
            )?;
            for cob in cobs {
                println!("{}", cob.id);
            }
        }
        Log {
            repo,
            type_name,
            object,
            format,
            from,
            until,
        } => {
            let repo = storage.repository(repo)?;
            let oid = object.resolve(&repo.backend)?;

            let from = from.map(|from| from.resolve(&repo.backend)).transpose()?;
            let until = until
                .map(|until| until.resolve(&repo.backend))
                .transpose()?;

            match type_name.into() {
                Issue => operations::<cob::issue::Action>(
                    &cob::issue::TYPENAME,
                    oid,
                    from,
                    until,
                    &repo,
                    format,
                )?,
                Patch => operations::<cob::patch::Action>(
                    &cob::patch::TYPENAME,
                    oid,
                    from,
                    until,
                    &repo,
                    format,
                )?,
                Identity => operations::<cob::identity::Action>(
                    &cob::identity::TYPENAME,
                    oid,
                    from,
                    until,
                    &repo,
                    format,
                )?,
                Other(type_name) => {
                    operations::<serde_json::Value>(&type_name, oid, from, until, &repo, format)?
                }
            }
        }
        Show {
            repo,
            objects,
            type_name,
            format: _,
        } => {
            let repo = storage.repository(repo)?;
            if let Err(e) = show(objects, &repo, type_name.into(), &profile) {
                if let Some(err) = e.downcast_ref::<std::io::Error>() {
                    if err.kind() == std::io::ErrorKind::BrokenPipe {
                        return Ok(());
                    }
                }
                return Err(e);
            }
        }
        Update(args::Update {
            repo,
            type_name,
            object,
            operation,
            format: _,
        }) => {
            let signer = &profile.signer()?;
            let repo = storage.repository_mut(repo)?;
            let oid = object.resolve::<radicle::git::Oid>(&repo.backend)?.into();
            let embeds = embeds(&repo, operation.embed_files, operation.embed_hashes)?;

            let oid = match type_name {
                Patch => {
                    let actions: Vec<cob::patch::Action> =
                        read_jsonl_actions(&operation.actions)?.into();
                    let mut patches = profile.patches_mut(&repo)?;
                    let mut patch = patches.get_mut(&oid)?;
                    patch.transaction(&operation.message, &*profile.signer()?, |tx| {
                        tx.extend(actions)?;
                        tx.embed(embeds)?;
                        Ok(())
                    })?
                }
                Issue => {
                    let actions: Vec<cob::issue::Action> =
                        read_jsonl_actions(&operation.actions)?.into();
                    let mut issues = profile.issues_mut(&repo)?;
                    let mut issue = issues.get_mut(&oid)?;
                    issue.transaction(&operation.message, &*profile.signer()?, |tx| {
                        tx.extend(actions)?;
                        tx.embed(embeds)?;
                        Ok(())
                    })?
                }
                Identity => anyhow::bail!(
                    "Update of collaborative objects of type {} is not supported.",
                    &type_name
                ),
                Other(type_name) => {
                    use cob::external::{Action, External};
                    let actions: Vec<Action> = read_jsonl_actions(&operation.actions)?.into();
                    let mut store: Store<External, _> = Store::open_for(&type_name, &repo)?;
                    let tx = cob::store::Transaction::new(type_name.clone(), actions, embeds);
                    let (_, oid) = tx.commit(&operation.message, oid, &mut store, signer)?;
                    oid
                }
            };

            println!("{oid}");
        }
    }
    Ok(())
}

fn show(
    oids: Vec<Rev>,
    repo: &storage::git::Repository,
    type_name: FilteredTypeName,
    profile: &Profile,
) -> Result<(), anyhow::Error> {
    use io::Write as _;
    let mut stdout = std::io::stdout();

    match type_name {
        FilteredTypeName::Identity => {
            use cob::identity;
            for oid in oids {
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
            for oid in oids {
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
            for oid in oids {
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
            for oid in oids {
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

fn print_op_pretty<A>(op: cob::Op<A>) -> anyhow::Result<()>
where
    A: serde::Serialize,
{
    let time = DateTime::<Utc>::from(
        std::time::UNIX_EPOCH + std::time::Duration::from_secs(op.timestamp.as_secs()),
    )
    .to_rfc2822();
    term::print(term::format::yellow(format!("commit   {}", op.id)));
    if let Some(oid) = op.identity {
        term::print(term::format::tertiary(format!("resource {oid}")));
    }
    for parent in op.parents {
        term::print(format!("parent   {parent}"));
    }
    for parent in op.related {
        term::print(format!("rel      {parent}"));
    }
    term::print(format!("author   {}", op.author));
    term::print(format!("date     {time}"));
    term::blank();
    for action in op.actions {
        let val = serde_json::to_string_pretty(&action)?;
        for line in val.lines() {
            term::indented(term::format::dim(line));
        }
        term::blank();
    }
    Ok(())
}

fn print_op_json<A>(op: cob::Op<A>) -> anyhow::Result<()>
where
    A: serde::Serialize,
{
    term::print(serde_json::to_value(&op)?);
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
fn read_jsonl_actions<A>(path: impl AsRef<Path>) -> anyhow::Result<NonEmpty<A>>
where
    A: CobAction + serde::de::DeserializeOwned,
{
    let reader = io::BufReader::new(std::fs::File::open(&path)?);

    NonEmpty::from_vec(read_jsonl(reader)?)
        .ok_or_else(|| anyhow!("at least one action is required"))
}

fn operations<A>(
    typename: &cob::TypeName,
    oid: cob::ObjectId,
    from: Option<git::Oid>,
    until: Option<git::Oid>,
    repo: &storage::git::Repository,
    format: Format,
) -> anyhow::Result<()>
where
    A: serde::Serialize,
    A: for<'de> serde::Deserialize<'de>,
{
    let history = cob::stream::CobRange::new(typename, &oid);
    let stream = cob::stream::Stream::<A>::new(&repo.backend, history, typename.clone());
    let iter = match (from, until) {
        (None, None) => stream.all()?,
        (None, Some(until)) => stream.until(until)?,
        (Some(from), None) => stream.since(from)?,
        (Some(from), Some(until)) => stream.range(from, until)?,
    };

    // Reverse
    let iter = iter.collect::<Vec<_>>().into_iter().rev();

    for op in iter {
        let op = op?;
        match format {
            Format::Json => print_op_json(op)?,
            Format::Pretty => print_op_pretty(op)?,
        }
    }

    Ok(())
}

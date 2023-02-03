#![allow(clippy::or_fun_call)]
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::str::FromStr;

use anyhow::{anyhow, Context as _};
use chrono::prelude::*;
use json_color::{Color, Colorizer};

use radicle::crypto::Unverified;
use radicle::identity::Untrusted;
use radicle::identity::{Doc, Id};
use radicle::storage::{ReadRepository, ReadStorage, WriteStorage};

use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};

pub const HELP: Help = Help {
    name: "inspect",
    description: "Inspect an identity or project directory",
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad inspect <path> [<option>...]
    rad inspect <id>   [<option>...]
    rad inspect

    Inspects the given path or ID. If neither is specified,
    the current project is inspected.

Options

    --id        Return the ID in simplified form
    --payload   Inspect the object's payload
    --refs      Inspect the object's refs on the local device (requires `tree`)
    --history   Show object's history
    --help      Print help
"#,
};

#[derive(Default, Debug, Eq, PartialEq)]
pub struct Options {
    pub id: Option<Id>,
    pub refs: bool,
    pub payload: bool,
    pub history: bool,
    pub id_only: bool,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut id: Option<Id> = None;
        let mut refs = false;
        let mut payload = false;
        let mut history = false;
        let mut id_only = false;

        while let Some(arg) = parser.next()? {
            match arg {
                Long("help") => {
                    return Err(Error::Help.into());
                }
                Long("refs") => {
                    refs = true;
                }
                Long("payload") => {
                    payload = true;
                }
                Long("history") => {
                    history = true;
                }
                Long("id") => {
                    id_only = true;
                }
                Value(val) if id.is_none() => {
                    let val = val.to_string_lossy();

                    if let Ok(val) = Id::from_str(&val) {
                        id = Some(val);
                    } else if let Ok(val) = PathBuf::from_str(&val) {
                        id = radicle::rad::repo(val)
                            .map(|(_, id)| Some(id))
                            .context("Supplied argument is not a valid `path`")?;
                    } else {
                        return Err(anyhow!("invalid `path` or `id` '{}'", val));
                    }
                }
                _ => return Err(anyhow::anyhow!(arg.unexpected())),
            }
        }

        Ok((
            Options {
                id,
                payload,
                history,
                refs,
                id_only,
            },
            vec![],
        ))
    }
}

pub fn run(options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    let storage = &profile.storage;
    let signer = term::signer(&profile)?;

    let id = match options.id {
        Some(id) => id,
        None => {
            let (_, id) = radicle::rad::repo(Path::new("."))
                .context("Current directory is not a Radicle project")?;

            id
        }
    };

    let project = storage
        .get(signer.public_key(), id)?
        .context("No project with such `id` exists")?;

    if options.refs {
        let path = profile
            .home
            .storage()
            .join(id.urn())
            .join("refs")
            .join("namespaces");

        Command::new("tree")
            .current_dir(path)
            .args(["--noreport", "--prune"])
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()?
            .wait()?;
    } else if options.payload {
        println!(
            "{}",
            colorizer().colorize_json_str(&serde_json::to_string_pretty(&project.payload)?)?
        );
    } else if options.history {
        let repo = storage.repository(id)?;
        let head = Doc::<Untrusted>::head(signer.public_key(), &repo)?;
        let history = repo.revwalk(head)?;

        for oid in history {
            let oid = oid?.into();
            let tip = repo.commit(oid)?;
            let blob = Doc::<Unverified>::blob_at(oid, &repo)?;
            let content: serde_json::Value = serde_json::from_slice(blob.content())?;
            let timezone = if tip.time().sign() == '+' {
                #[allow(deprecated)]
                FixedOffset::east(tip.time().offset_minutes() * 60)
            } else {
                #[allow(deprecated)]
                FixedOffset::west(tip.time().offset_minutes() * 60)
            };
            let time = DateTime::<Utc>::from(
                std::time::UNIX_EPOCH + std::time::Duration::from_secs(tip.time().seconds() as u64),
            )
            .with_timezone(&timezone)
            .to_rfc2822();

            println!(
                "{} {}",
                term::format::yellow("commit"),
                term::format::yellow(oid),
            );
            if let Ok(parent) = tip.parent_id(0) {
                println!("parent {parent}");
            }
            println!("blob   {}", blob.id());
            println!("date   {time}");
            println!();

            if let Some(msg) = tip.message() {
                for line in msg.lines() {
                    term::indented(term::format::dim(line));
                }
                term::blank();
            }

            let json = colorizer().colorize_json_str(&serde_json::to_string_pretty(&content)?)?;
            for line in json.lines() {
                println!(" {line}");
            }
            println!();
        }
    } else if options.id_only {
        term::info!("{}", term::format::highlight(id.urn()));
    } else {
        term::info!("{}", term::format::highlight(id));
    }

    Ok(())
}

// Used for JSON Colorizing
fn colorizer() -> Colorizer {
    Colorizer::new()
        .null(Color::Cyan)
        .boolean(Color::Cyan)
        .number(Color::Magenta)
        .string(Color::Green)
        .key(Color::Blue)
        .build()
}

#![allow(clippy::or_fun_call)]
use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{anyhow, Context as _};
use chrono::prelude::*;
use json_color::{Color, Colorizer};

use radicle::crypto::{Unverified, Verified};
use radicle::identity::Untrusted;
use radicle::identity::{Doc, Id};
use radicle::storage::git::Storage;
use radicle::storage::{ReadRepository, ReadStorage};

use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};

pub const HELP: Help = Help {
    name: "inspect",
    description: "Inspect a radicle repository",
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad inspect <path> [<option>...]
    rad inspect <rid>  [<option>...]
    rad inspect [<option>...]

    Inspects the given path or RID. If neither is specified,
    the current repository is inspected.

Options

    --id        Return the repository identifier (RID)
    --payload   Inspect the repository's identity payload
    --refs      Inspect the repository's refs on the local device
    --history   Show the history of the repository identity document
    --help      Print help
"#,
};

#[derive(Default, Debug, Eq, PartialEq)]
pub enum Target {
    Refs,
    Payload,
    History,
    #[default]
    Id,
}

#[derive(Default, Debug, Eq, PartialEq)]
pub struct Options {
    pub id: Option<Id>,
    pub target: Target,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut id: Option<Id> = None;
        let mut target = Target::default();

        while let Some(arg) = parser.next()? {
            match arg {
                Long("help") | Short('h') => {
                    return Err(Error::Help.into());
                }
                Long("refs") => {
                    target = Target::Refs;
                }
                Long("payload") => {
                    target = Target::Payload;
                }
                Long("history") => {
                    target = Target::History;
                }
                Long("id") => {
                    target = Target::Id;
                }
                Value(val) if id.is_none() => {
                    let val = val.to_string_lossy();

                    if let Ok(val) = Id::from_str(&val) {
                        id = Some(val);
                    } else if let Ok(val) = PathBuf::from_str(&val) {
                        id = radicle::rad::repo(val)
                            .map(|(_, id)| Some(id))
                            .context("Supplied argument is not a valid path")?;
                    } else {
                        return Err(anyhow!("invalid path or RID '{}'", val));
                    }
                }
                _ => return Err(anyhow::anyhow!(arg.unexpected())),
            }
        }

        Ok((Options { id, target }, vec![]))
    }
}

pub fn run(options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    let id = match options.id {
        Some(id) => id,
        None => {
            let (_, id) = radicle::rad::repo(Path::new("."))
                .context("Current directory is not a radicle project")?;

            id
        }
    };

    if options.target == Target::Id {
        term::info!("{}", term::format::highlight(id.urn()));
        return Ok(());
    }

    let profile = ctx.profile()?;
    let storage = &profile.storage;
    let signer = term::signer(&profile)?;
    let repo = storage
        .repository(id)
        .context("No project with the given RID exists")?;
    let project = Doc::<Verified>::canonical(&repo)?;

    match options.target {
        Target::Refs => {
            refs(storage, id)?;
        }
        Target::Payload => {
            println!(
                "{}",
                colorizer().colorize_json_str(&serde_json::to_string_pretty(&project.payload)?)?
            );
        }
        Target::History => {
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
                    std::time::UNIX_EPOCH
                        + std::time::Duration::from_secs(tip.time().seconds() as u64),
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
                        if line.is_empty() {
                            println!();
                        } else {
                            term::indented(term::format::dim(line));
                        }
                    }
                    term::blank();
                }

                let json =
                    colorizer().colorize_json_str(&serde_json::to_string_pretty(&content)?)?;
                for line in json.lines() {
                    println!(" {line}");
                }
                println!();
            }
        }
        Target::Id => {
            // Handled above.
        }
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

fn refs(storage: &Storage, id: Id) -> anyhow::Result<()> {
    let repo = storage.repository(id)?;
    let mut refs = Vec::new();
    for r in repo.references()? {
        let r = r?;
        if let Some(namespace) = r.namespace {
            refs.push(format!("{}/{}", namespace, r.name));
        }
    }

    print!("{}", tree(refs));

    Ok(())
}

/// Show the list of given git references as a newline terminated tree `String` similar to the tree command.
fn tree(mut refs: Vec<String>) -> String {
    refs.sort();

    // List of references with additional unique entries for each 'directory'.
    //
    // i.e. "refs/heads/master" becomes ["refs"], ["refs", "heads"], and ["refs", "heads",
    // "master"].
    let mut refs_expanded: Vec<Vec<String>> = Vec::new();
    // Number of entries per Git 'directory'.
    let mut ref_entries: HashMap<Vec<String>, usize> = HashMap::new();
    let mut last: Vec<String> = Vec::new();

    for r in refs {
        let r: Vec<String> = r.split('/').map(|s| s.to_string()).collect();

        for (i, v) in r.iter().enumerate() {
            let last_v = last.get(i);
            if Some(v) != last_v {
                last = r.clone().iter().take(i + 1).map(String::from).collect();

                refs_expanded.push(last.clone());

                let mut dir = last.clone();
                dir.pop();
                if dir.is_empty() {
                    continue;
                }

                if let Some(num) = ref_entries.get_mut(&dir) {
                    *num += 1;
                } else {
                    ref_entries.insert(dir, 1);
                }
            }
        }
    }
    let mut tree = String::default();

    for mut ref_components in refs_expanded {
        // Better to explode when things do not go as expected.
        let name = ref_components.pop().expect("non-empty vector");
        if ref_components.is_empty() {
            tree.push_str(&format!("{name}\n"));
            continue;
        }

        for i in 1..ref_components.len() {
            let parent: Vec<String> = ref_components.iter().take(i).cloned().collect();

            let num = ref_entries.get(&parent).unwrap_or(&0);
            if *num == 0 {
                tree.push_str("    ");
            } else {
                tree.push_str("│   ");
            }
        }

        if let Some(num) = ref_entries.get_mut(&ref_components) {
            if *num == 1 {
                tree.push_str(&format!("└── {name}\n"));
            } else {
                tree.push_str(&format!("├── {name}\n"));
            }
            *num -= 1;
        }
    }

    tree
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_tree() {
        let arg = vec![
            String::from("z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/refs/heads/master"),
            String::from("z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/refs/rad/id"),
            String::from("z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/refs/rad/sigrefs"),
        ];
        let exp = r#"
z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
└── refs
    ├── heads
    │   └── master
    └── rad
        ├── id
        └── sigrefs
"#
        .trim_start();

        assert_eq!(tree(arg), exp);
        assert_eq!(tree(vec![String::new()]), "\n");
    }
}

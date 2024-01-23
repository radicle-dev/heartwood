use std::process::{Child, Command, Stdio};

use anyhow::{anyhow, Error};
use serde::Deserialize;

use crate::git::Rev;
use crate::terminal as term;

use crate::commands::rad_patch::{ListOptions, State};

pub trait ToArg<T> {
    fn to_arg(&self) -> T;
}

impl ToArg<String> for State {
    fn to_arg(&self) -> String {
        match self {
            State::All => "state:all".to_owned(),
            State::Draft => "state:draft".to_owned(),
            State::Open => "state:open".to_owned(),
            State::Merged => "state:merged".to_owned(),
            State::Archived => "state:archived".to_owned(),
        }
    }
}

/// A patch operation returned by the id / operation selection TUI.
/// Structs of this type are being parsed and instanced from JSON
/// if `--json` is given to the TUI call.
/// If converted to from `String`, allow JSON only.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(tag = "operation")]
pub enum PatchOperation {
    Show { id: String },
    Checkout { id: String },
    Comment { id: String },
    Edit { id: String },
    Delete { id: String },
}

impl From<String> for PatchOperation {
    fn from(value: String) -> Self {
        serde_json::from_str(&value).unwrap()
    }
}

fn wait_for_tui(tui: Child) -> Result<Option<String>, Error> {
    let output = tui
        .wait_with_output()
        .map_err(|err| anyhow!("Failed to wait on `rad tui`: {err}"))?;

    let stderr = String::from_utf8(output.stderr)?.trim().to_owned();

    if output.status.success() {
        Ok((!stderr.is_empty()).then_some(stderr))
    } else {
        Err(anyhow!("An internal error occured in `rad tui`: {stderr}"))
    }
}

pub fn select_patch_id() -> Result<Option<Rev>, Error> {
    match Command::new("rad-tui")
        .arg("patch")
        .arg("select")
        .arg("--id")
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(tui) => wait_for_tui(tui).map(|output| output.map(Rev::from)),
        Err(_) => {
            term::tip!(
                "An optional patch selector can be enabled by installing `rad-tui`. You can download it from https://files.radicle.xyz/latest.",
            );
            Ok(None)
        }
    }
}

pub fn select_patch_operation(opts: &ListOptions) -> Result<Option<PatchOperation>, Error> {
    let mut filter = opts.state.to_arg();

    if opts.authored {
        filter.push_str(",is:authored");
    }
    if !opts.authors.is_empty() {
        let mut authors = opts.authors.iter().peekable();
        let mut list = String::new();

        while let Some(author) = authors.next() {
            list.push_str(&author.to_string());
            if authors.peek().is_some() {
                list.push(',');
            }
        }
        filter.push_str(&format!(",authors:[{list}]"));
    }

    println!("rad-tui patch --json select --filter {}", filter);

    match Command::new("rad-tui")
        .arg("patch")
        .arg("--json")
        .arg("select")
        .arg("--filter")
        .arg(filter)
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(tui) => wait_for_tui(tui).map(|output| output.map(PatchOperation::from)),
        Err(_) => {
            term::tip!(
                "An optional patch selector can be enabled by installing `rad-tui`. You can download it from https://files.radicle.xyz/latest.",
            );
            Ok(None)
        }
    }
}

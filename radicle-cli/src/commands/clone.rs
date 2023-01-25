#![allow(clippy::or_fun_call)]
use std::ffi::OsString;
use std::path::Path;
use std::str::FromStr;

use anyhow::anyhow;
use thiserror::Error;

use radicle::git::raw;
use radicle::identity::doc;
use radicle::identity::doc::{DocError, Id};
use radicle::node;
use radicle::node::{FetchLookup, Handle};
use radicle::prelude::*;
use radicle::rad;
use radicle::storage;
use radicle::storage::git::{ProjectError, Storage};
use radicle::storage::WriteStorage;

use crate::commands::rad_checkout as checkout;
use crate::project;
use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};
use crate::terminal::Interactive;

pub const HELP: Help = Help {
    name: "clone",
    description: "Clone a project",
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad clone <id> [<option>...]

Options

    --no-confirm    Don't ask for confirmation during clone
    --help          Print help

"#,
};

#[derive(Debug)]
pub struct Options {
    id: Id,
    #[allow(dead_code)]
    interactive: Interactive,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut id: Option<Id> = None;
        let mut interactive = Interactive::Yes;

        while let Some(arg) = parser.next()? {
            match arg {
                Long("no-confirm") => {
                    interactive = Interactive::No;
                }
                Long("help") => {
                    return Err(Error::Help.into());
                }
                Value(val) if id.is_none() => {
                    let val = val.to_string_lossy();
                    let val = val.strip_prefix("rad://").unwrap_or(&val);
                    let val = Id::from_str(val)?;

                    id = Some(val);
                }
                _ => return Err(anyhow!(arg.unexpected())),
            }
        }
        let id = id.ok_or_else(|| {
            anyhow!("to clone, a radicle id must be provided; see `rad clone --help`")
        })?;

        Ok((Options { id, interactive }, vec![]))
    }
}

pub fn run(options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    let signer = term::signer(&profile)?;
    let mut node = radicle::Node::new(profile.socket());
    let (working, doc, proj) = clone(options.id, &signer, &profile.storage, &mut node)?;
    let delegates = doc
        .delegates
        .iter()
        .map(|d| **d)
        .filter(|id| id != profile.id())
        .collect::<Vec<_>>();
    let default_branch = proj.default_branch().clone();
    let path = working.workdir().unwrap(); // SAFETY: The working copy is not bare.

    // Setup tracking for project delegates.
    checkout::setup_remotes(
        project::SetupRemote {
            project: options.id,
            default_branch,
            repo: &working,
            fetch: true,
            tracking: true,
        },
        &delegates,
    )?;

    term::headline(&format!(
        "🌱 Project successfully cloned under {}",
        term::format::highlight(Path::new(".").join(path).display())
    ));

    Ok(())
}

#[derive(Error, Debug)]
pub enum CloneError<H: node::Handle> {
    #[error("node: {0}")]
    Node(#[from] node::Error),
    #[error("fetch: {0}")]
    Fetch(#[from] node::FetchError),
    #[error("fork: {0}")]
    Fork(#[from] rad::ForkError),
    #[error("storage: {0}")]
    Storage(#[from] storage::Error),
    #[error("checkout: {0}")]
    Checkout(#[from] rad::CheckoutError),
    #[error("identity document error: {0}")]
    Doc(#[from] DocError),
    #[error("payload: {0}")]
    Payload(#[from] doc::PayloadError),
    #[error("project error: {0}")]
    Project(#[from] ProjectError),
    #[error("handle error: {0}")]
    Handle(H::Error),
}

pub fn clone<G: Signer, H: Handle>(
    id: Id,
    signer: &G,
    storage: &Storage,
    node: &mut H,
) -> Result<(raw::Repository, Doc<Verified>, Project), CloneError<H>> {
    let me = *signer.public_key();

    // Track & fetch project.
    if node.track_repo(id).map_err(CloneError::Handle)? {
        term::success!(
            "Tracking relationship restablished for {}",
            term::format::tertiary(id)
        );
    }

    let spinner = term::spinner(format!("Fetching {}..", term::format::tertiary(id)));
    match node.fetch(id).map_err(CloneError::Handle)? {
        FetchLookup::Found { seeds, results } => {
            // TODO: If none of them succeeds, output an error. Otherwise tell the caller
            // how many succeeded.
            for result in results.iter().take(seeds.len()) {
                match &*result {
                    Ok(_updates) => {}
                    Err(_err) => {}
                }
            }
        }
        FetchLookup::NotFound => {
            // TODO: Return error.
        }
        FetchLookup::NotTracking => {
            // SAFETY: Since we track it above, this shouldn't trigger unless there's a bug.
            panic!("clone: Repository is not tracked");
        }
        FetchLookup::Error(err) => {
            return Err(err.into());
        }
    }
    spinner.finish();

    // Create a local fork of the project, under our own id.
    {
        let spinner = term::spinner(format!(
            "Forking under {}..",
            term::format::tertiary(term::format::node(&me))
        ));
        rad::fork(id, signer, &storage)?;

        spinner.finish();
    }

    let doc = storage.repository(id)?.identity_of(&me)?;
    let proj = doc.project()?;
    let path = Path::new(proj.name());

    // Checkout.
    let spinner = term::spinner(format!(
        "Creating checkout in ./{}..",
        term::format::tertiary(path.display())
    ));
    let repo = rad::checkout(id, &me, path, &storage)?;

    spinner.finish();

    Ok((repo, doc, proj))
}

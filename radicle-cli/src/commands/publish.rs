use std::ffi::OsString;

use anyhow::{anyhow, Context as _};

use radicle::identity::{Identity, Visibility};
use radicle::node::Handle as _;
use radicle::prelude::RepoId;
use radicle::storage::{SignRepository, ValidateRepository, WriteRepository, WriteStorage};

use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};

pub const HELP: Help = Help {
    name: "publish",
    description: "Publish a repository to the network",
    version: env!("RADICLE_VERSION"),
    usage: r#"
Usage

    rad publish [<rid>] [<option>...]

    Publishing a private repository makes it public and discoverable
    on the network.

    By default, this command will publish the current repository.
    If an `<rid>` is specified, that repository will be published instead.

    Note that this command can only be run for repositories with a
    single delegate. The delegate must be the currently authenticated
    user. For repositories with more than one delegate, the `rad id`
    command must be used.

Options

    --help                    Print help
"#,
};

#[derive(Default, Debug)]
pub struct Options {
    pub rid: Option<RepoId>,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut rid = None;

        while let Some(arg) = parser.next()? {
            match arg {
                Long("help") | Short('h') => {
                    return Err(Error::Help.into());
                }
                Value(val) if rid.is_none() => {
                    rid = Some(term::args::rid(&val)?);
                }
                arg => {
                    return Err(anyhow!(arg.unexpected()));
                }
            }
        }

        Ok((Options { rid }, vec![]))
    }
}

pub fn run(options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    let rid = match options.rid {
        Some(rid) => rid,
        None => radicle::rad::cwd()
            .map(|(_, rid)| rid)
            .context("Current directory is not a Radicle repository")?,
    };

    let repo = profile.storage.repository_mut(rid)?;
    let mut identity = Identity::load_mut(&repo)?;
    let doc = identity.doc();

    if doc.is_public() {
        return Err(Error::WithHint {
            err: anyhow!("repository is already public"),
            hint: "to announce the repository to the network, run `rad sync --inventory`",
        }
        .into());
    }
    if !doc.is_delegate(&profile.id().into()) {
        return Err(anyhow!("only the repository delegate can publish it"));
    }
    if doc.delegates().len() > 1 {
        return Err(Error::WithHint {
            err: anyhow!(
                "only repositories with a single delegate can be published with this command"
            ),
            hint: "see `rad id --help` to publish repositories with more than one delegate",
        }
        .into());
    }
    let signer = profile.signer()?;

    // Update identity document.
    let doc = doc.clone().with_edits(|doc| {
        doc.visibility = Visibility::Public;
    })?;

    identity.update("Publish repository", "", &doc, &signer)?;
    repo.sign_refs(&signer)?;
    repo.set_identity_head()?;
    let validations = repo.validate()?;

    if !validations.is_empty() {
        for err in validations {
            term::error!(format!("validation error: {err}"));
        }
        anyhow::bail!("fatal: repository storage is corrupt");
    }
    let mut node = radicle::Node::new(profile.socket());
    let spinner = term::spinner("Updating inventory..");

    // The repository is now part of our inventory.
    profile.add_inventory(rid, &mut node)?;
    spinner.finish();

    term::success!(
        "Repository is now {}",
        term::format::visibility(doc.visibility())
    );

    if !node.is_running() {
        term::warning!(term,
            "Your node is not running. Start your node with {} to announce your repository \
            to the network",
            &term::format::command("rad node start")
        );
    }

    Ok(())
}

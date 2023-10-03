use std::ffi::OsString;

use anyhow::{anyhow, Context as _};

use radicle::identity::{Identity, Visibility};
use radicle::node::Handle as _;
use radicle::prelude::Id;
use radicle::storage::{ReadRepository, SignRepository, WriteRepository, WriteStorage};

use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};

pub const HELP: Help = Help {
    name: "publish",
    description: "Publish a repository to the network",
    version: env!("CARGO_PKG_VERSION"),
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
    pub rid: Option<Id>,
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
            .context("Current directory is not a radicle project")?,
    };

    let repo = profile.storage.repository_mut(rid)?;
    let mut identity = Identity::load_mut(&repo)?;
    let mut doc = identity.doc().clone();

    if doc.visibility.is_public() {
        return Err(Error::WithHint {
            err: anyhow!("repository is already public"),
            hint: "to announce the repository to the network, run `rad sync --inventory`",
        }
        .into());
    }
    if !doc.is_delegate(profile.id()) {
        return Err(anyhow!("only the repository delegate can publish it"));
    }
    if doc.delegates.len() > 1 {
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
    doc.visibility = Visibility::Public;

    identity.update("Publish repository", "", &doc, &signer)?;
    repo.sign_refs(&signer)?;
    repo.set_identity_head()?;
    repo.validate()?;

    term::success!(
        "Repository is now {}",
        term::format::visibility(&doc.visibility)
    );

    let mut node = radicle::Node::new(profile.socket());
    if node.is_running() {
        let spinner = term::spinner("Announcing to network..");
        node.announce_inventory()?;
        spinner.finish();
    } else {
        term::warning(format!(
            "Your node is not running. Start your node with {} to announce your repository \
            to the network",
            term::format::command("rad node start")
        ));
    }

    Ok(())
}

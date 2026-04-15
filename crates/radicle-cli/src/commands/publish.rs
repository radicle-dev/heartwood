mod args;

use anyhow::anyhow;

use radicle::cob;
use radicle::identity::{Identity, Visibility};
use radicle::node::Handle as _;
use radicle::storage::{SignRepository, ValidateRepository, WriteRepository, WriteStorage};

use crate::terminal as term;
use crate::terminal::args::rid_or_cwd;

pub use args::Args;

pub fn run(args: Args, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    let (_, rid) = rid_or_cwd(args.repo)?;

    let repo = profile.storage.repository_mut(rid)?;
    let signer = profile.signer()?;
    let mut identity = Identity::load_mut(&repo, &signer)?;
    let doc = identity.doc();

    if doc.is_public() {
        return Err(term::Error::with_hint(
            anyhow!("repository is already public"),
            "to announce the repository to the network, run `rad sync --inventory`",
        )
        .into());
    }
    if !doc.is_delegate(&profile.id().into()) {
        return Err(anyhow!("only the repository delegate can publish it"));
    }
    if doc.delegates().len() > 1 {
        return Err(term::Error::with_hint(
            anyhow!("only repositories with a single delegate can be published with this command"),
            "see `rad id --help` to publish repositories with more than one delegate",
        )
        .into());
    }
    let signer = profile.signer()?;

    // Update identity document.
    let doc = doc.clone().with_edits(|doc| {
        doc.visibility = Visibility::Public;
    })?;

    // SAFETY: the `Title` here is guaranteed to be nonempty and does not
    // contain `\n` or `\r`.
    #[allow(clippy::unwrap_used)]
    identity.update(cob::Title::new("Publish repository").unwrap(), "", &doc)?;
    repo.sign_refs(&signer)?;
    repo.set_identity_head()?;
    let validations = repo.validate()?;

    if !validations.is_empty() {
        for err in validations {
            term::error(format!("validation error: {err}"));
        }
        anyhow::bail!("fatal: repository storage is corrupt");
    }
    let mut node = radicle::Node::new(profile.socket_from_env());
    let spinner = term::spinner("Updating inventory..");

    // The repository is now part of our inventory.
    profile.add_inventory(rid, &mut node)?;
    spinner.finish();

    term::success!(
        "Repository is now {}",
        term::format::visibility(doc.visibility())
    );

    if !node.is_running() {
        term::warning(format!(
            "Your node is not running. Start your node with {} to announce your repository \
            to the network",
            term::format::command("rad node start")
        ));
    }

    Ok(())
}

use std::path::Path;

use radicle::{node::Handle, storage::WriteRepository, storage::WriteStorage};

fn main() -> anyhow::Result<()> {
    let cwd = Path::new(".").canonicalize()?;
    let repo = radicle::git::raw::Repository::open(&cwd)?;
    let profile = radicle::Profile::load()?;
    let (_, id) = radicle::rad::remote(&repo)?;

    let output = radicle::git::run::<_, _, &str, &str>(&cwd, ["push", "rad"], None)?;
    println!("{output}");

    let signer = profile.signer()?;
    let project = profile.storage.repository(id)?;
    let sigrefs = project.sign_refs(&signer)?;
    let head = project.set_head()?;

    radicle::Node::new(profile.socket()).announce_refs(id)?;

    println!("head: {head}");
    println!("ok: {}", sigrefs.signature);

    Ok(())
}

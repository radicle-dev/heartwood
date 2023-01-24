use std::env;
use std::path::Path;

use radicle::identity::Id;

fn main() -> anyhow::Result<()> {
    let cwd = Path::new(".").canonicalize()?;
    let profile = radicle::Profile::load()?;
    let signer = profile.signer()?;

    if let Some(id) = env::args().nth(1) {
        let id = Id::from_urn(&id)?;
        let mut node = radicle::Node::new(profile.socket());
        let repo = radicle::rad::clone(id, &cwd, &signer, &profile.storage, &mut node)?;

        println!(
            "ok: project {id} cloned into `{}`",
            repo.workdir().unwrap().display()
        );
    } else {
        anyhow::bail!("Error: a project id must be specified");
    }

    Ok(())
}

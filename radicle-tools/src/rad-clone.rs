use std::env;
use std::path::Path;
use std::str::FromStr;

use radicle::identity::Id;

fn main() -> anyhow::Result<()> {
    let cwd = Path::new(".").canonicalize()?;
    let profile = radicle::Profile::load()?;

    if let Some(id) = env::args().nth(1) {
        let id = Id::from_str(&id)?;
        let node = profile.node()?;
        let repo = radicle::rad::clone(id, &cwd, &profile.signer, &profile.storage, &node)?;

        println!(
            "ok: project {id} cloned into `{}`",
            repo.workdir().unwrap().display()
        );
    } else {
        anyhow::bail!("Error: a project id must be specified");
    }

    Ok(())
}

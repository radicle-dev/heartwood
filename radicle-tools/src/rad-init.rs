use std::path::Path;

use radicle::{git, Profile};

fn main() -> anyhow::Result<()> {
    let cwd = Path::new(".").canonicalize()?;
    let name = cwd.file_name().unwrap().to_string_lossy().to_string();
    let repo = radicle::git::raw::Repository::open(cwd)?;
    let profile = Profile::load()?;
    let signer = profile.signer()?;
    let (id, _, _) = radicle::rad::init(
        &repo,
        &name,
        "",
        git::refname!("master"),
        &signer,
        &profile.storage,
    )?;

    println!("ok: {id}");

    Ok(())
}

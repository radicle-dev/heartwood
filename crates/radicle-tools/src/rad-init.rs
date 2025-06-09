use std::path::Path;

use radicle::{git, identity::Visibility, Profile};

fn main() -> anyhow::Result<()> {
    let cwd = Path::new(".").canonicalize()?;
    let name = cwd.file_name().unwrap().to_string_lossy().to_string();
    let repo = radicle::git::raw::Repository::open(cwd)?;
    let profile = Profile::load()?;
    let signer = profile.signer()?;
    let (id, _, _) = radicle::rad::init(
        &repo,
        name.try_into()?,
        "",
        git::refname!("master"),
        Visibility::default(),
        &signer,
        &profile.storage,
    )?;

    println!("ok: {id}");

    Ok(())
}

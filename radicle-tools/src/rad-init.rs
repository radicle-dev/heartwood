use std::path::Path;

use radicle::{cob, git, identity::Visibility, Profile};

fn main() -> anyhow::Result<()> {
    let cwd = Path::new(".").canonicalize()?;
    let name = cwd.file_name().unwrap().to_string_lossy().to_string();
    let repo = radicle::git::raw::Repository::open(cwd)?;
    let profile = Profile::load()?;
    let signer = profile.signer()?;
    let mut cache = cob::cache::Store::open(profile.cobs().join(cob::cache::COBS_DB_FILE))?;
    let (id, _, _) = radicle::rad::init(
        &repo,
        &mut cache,
        &name,
        "",
        git::refname!("master"),
        Visibility::default(),
        &signer,
        &profile.storage,
    )?;

    println!("ok: {id}");

    Ok(())
}

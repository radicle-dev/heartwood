use std::path::Path;

fn main() -> anyhow::Result<()> {
    let cwd = Path::new(".").canonicalize()?;
    let name = cwd.file_name().unwrap().to_string_lossy().to_string();
    let repo = radicle::git::raw::Repository::open(cwd)?;
    let profile = radicle::Profile::load()?;
    let (id, _) = radicle::rad::init(
        &repo,
        &name,
        "",
        radicle::git::BranchName::from("master"),
        &profile.signer,
        &profile.storage,
    )?;

    println!("ok: {}", id);

    Ok(())
}

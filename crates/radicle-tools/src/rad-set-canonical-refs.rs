use radicle::{
    storage::{WriteRepository, WriteStorage},
    Profile,
};

fn main() -> anyhow::Result<()> {
    let profile = Profile::load()?;

    let (_, rid) = radicle::rad::cwd()?;
    let repo = profile.storage.repository_mut(rid)?;

    let id_oid = repo.set_identity_head()?;
    let branch = repo.set_head()?;

    println!("ok: identity: {id_oid}");
    println!("ok: branch: {}", branch.new);

    Ok(())
}

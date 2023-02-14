use anyhow::Context as _;
use radicle::{
    prelude::Id,
    storage::{WriteRepository as _, WriteStorage},
    Profile,
};
use radicle_crypto::PublicKey;

use crate::terminal as term;

pub fn run<S>(profile: &Profile, storage: &S, id: Id, key: &PublicKey) -> anyhow::Result<()>
where
    S: WriteStorage,
{
    let signer = term::signer(profile)?;
    let me = signer.public_key();

    let mut project = storage
        .get(&profile.public_key, id)?
        .context("No project with such ID exists")?;

    let repo = storage.repository_mut(id)?;

    if !project.is_delegate(me) {
        return Err(anyhow::anyhow!(
            "'{}' is not a delegate of the project, only a delegate may remove this key",
            me
        ));
    }

    if project.threshold > 1 {
        return Err(anyhow::anyhow!("project threshold > 1"));
    }

    match project.rescind(key)? {
        Some(delegate) => {
            project.sign(&signer).and_then(|(_, sig)| {
                project.update(
                    signer.public_key(),
                    "Updated payload",
                    &[(signer.public_key(), sig)],
                    repo.raw(),
                )
            })?;
            term::info!("Removed delegate '{}'", delegate);
            term::success!("Update successful!");
            Ok(())
        }
        None => {
            term::info!("the delegate for '{}' did not exist", key);
            Ok(())
        }
    }
}

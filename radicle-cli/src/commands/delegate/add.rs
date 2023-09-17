use anyhow::Context as _;

use radicle::{
    prelude::{Did, Id},
    storage::{SignRepository, WriteRepository as _, WriteStorage},
    Profile,
};
use radicle_crypto::PublicKey;

use crate::terminal as term;

pub fn run<S>(id: Id, key: PublicKey, profile: &Profile, storage: &S) -> anyhow::Result<()>
where
    S: WriteStorage,
{
    let signer = term::signer(profile)?;
    let me = signer.public_key();

    let mut project = storage
        .get(&profile.public_key, id)?
        .context("No project with the given RID exists")?;

    let repo = storage.repository_mut(id)?;

    if !project.is_delegate(me) {
        return Err(anyhow::anyhow!(
            "'{}' is not a delegate of the project, only a delegate may add this key",
            me
        ));
    }

    if project.threshold > 1 {
        return Err(anyhow::anyhow!("project threshold > 1"));
    }

    if project.delegate(&key) {
        project.sign(&signer).and_then(|(_, sig)| {
            project.update(
                signer.public_key(),
                "Updated payload",
                &[(signer.public_key(), sig)],
                repo.raw(),
            )
        })?;
        repo.sign_refs(&signer)?;
        repo.set_identity_head()?;
        term::info!("Added delegate '{}'", Did::from(key));
        term::success!("Update successful!");
        Ok(())
    } else {
        term::info!("the delegate for '{}' already exists", key);
        Ok(())
    }
}

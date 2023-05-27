use anyhow::Context as _;

use radicle::{prelude::Id, storage::ReadStorage, Profile};

use crate::terminal as term;

pub fn run<S>(id: Id, profile: &Profile, storage: &S) -> anyhow::Result<()>
where
    S: ReadStorage,
{
    let project = storage
        .get(&profile.public_key, id)?
        .context("No project with the given RID exists")?;

    term::info!("{}", serde_json::to_string_pretty(&project.delegates)?);
    Ok(())
}

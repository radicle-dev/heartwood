use radicle::{git::Url, prelude::Id, Profile};
use radicle_crypto::PublicKey;

use crate::git::add_remote;
use crate::{git, terminal as term};

pub fn run(
    id: Id,
    pubkey: &PublicKey,
    name: Option<String>,
    profile: &Profile,
    repository: &git::Repository,
) -> anyhow::Result<()> {
    let name = match name {
        Some(name) => name,
        _ => profile
            .tracking()?
            .node_policy(pubkey)?
            .and_then(|node| node.alias)
            .ok_or(anyhow::anyhow!("a `name` needs to be specified"))?,
    };
    if git::is_remote(repository, &name)? {
        anyhow::bail!("remote `{name}` already exists");
    }

    let url = Url::from(id).with_namespace(*pubkey);
    let remote = add_remote(repository, &name, &url)?;
    term::success!("Remote {} added with {url}", remote.name,);

    Ok(())
}

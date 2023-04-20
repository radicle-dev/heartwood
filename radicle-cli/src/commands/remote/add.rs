use radicle::node::tracking::store::Config;
use radicle::{git::Url, node::TRACKING_DB_FILE, prelude::Id, Profile};
use radicle_crypto::PublicKey;

use crate::git::add_remote;
use crate::{git, terminal as term};

pub fn run(
    repository: &git::Repository,
    profile: &Profile,
    pubkey: &PublicKey,
    name: Option<String>,
    id: Id,
) -> anyhow::Result<()> {
    let name = match name {
        Some(name) => name,
        _ => get_remote_name(profile, pubkey)?
            .ok_or(anyhow::anyhow!("a `name` needs to be specified"))?,
    };
    if git::is_remote(repository, &name)? {
        anyhow::bail!("remote `{name}` already exists");
    }

    let url = Url::from(id).with_namespace(*pubkey);
    let remote = add_remote(repository, &name, &url)?;
    term::success!(
        "Remote {} added with {url}",
        remote.name,
    );
    Ok(())
}

/// Get the `git remote` name from the command `Options` and `pubkey`.
fn get_remote_name(profile: &Profile, pubkey: &PublicKey) -> anyhow::Result<Option<String>> {
    let path = profile.home.node().join(TRACKING_DB_FILE);
    let storage = Config::reader(path)?;
    Ok(storage.node_policy(pubkey)?.and_then(|node| node.alias))
}

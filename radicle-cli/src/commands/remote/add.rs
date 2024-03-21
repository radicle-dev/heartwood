use std::str::FromStr;

use radicle::git;
use radicle::git::RefString;
use radicle::prelude::*;
use radicle::Profile;
use radicle_crypto::PublicKey;

use crate::commands::rad_checkout as checkout;
use crate::commands::rad_follow as follow;
use crate::commands::rad_sync as sync;
use crate::node::SyncSettings;
use crate::project::SetupRemote;

pub fn run(
    rid: RepoId,
    nid: &PublicKey,
    name: Option<RefString>,
    tracking: Option<BranchName>,
    profile: &Profile,
    repo: &git::raw::Repository,
    fetch: bool,
    sync: bool,
) -> anyhow::Result<()> {
    if sync {
        let mut node = radicle::Node::new(profile.socket());

        if !profile.policies()?.is_following(nid)? {
            let alias = name.as_ref().and_then(|n| Alias::from_str(n.as_str()).ok());

            follow::follow(*nid, alias, &mut node, profile)?;
            sync::fetch(
                rid,
                SyncSettings::default().with_profile(profile),
                &mut node,
            )?;
        }
    }
    let aliases = profile.aliases();
    let setup = SetupRemote {
        rid,
        tracking,
        fetch,
        repo,
    };
    checkout::setup_remote(&setup, nid, name, &aliases)?;

    Ok(())
}

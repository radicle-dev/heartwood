use radicle::git::RefString;
use radicle::prelude::*;
use radicle::Profile;
use radicle_crypto::PublicKey;

use crate::commands::rad_checkout as checkout;
use crate::git;
use crate::project::SetupRemote;

pub fn run(
    rid: Id,
    nid: &PublicKey,
    name: Option<RefString>,
    tracking: Option<BranchName>,
    profile: &Profile,
    repo: &git::Repository,
) -> anyhow::Result<()> {
    let aliases = profile.aliases();
    let setup = SetupRemote {
        rid,
        tracking,
        fetch: false,
        repo,
    };
    checkout::setup_remote(&setup, nid, name, &aliases)
}
